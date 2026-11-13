// ==============================================================================
// ARCHIVO: src/ui/mod.rs
// DESCRIPCIÓN: UI completa. Añade dos botones (Abrir papelera / Vaciar papelera)
//              con conteo en vivo y modal de confirmación.
//
// ─── NUEVOS CAMPOS DE ESTADO ──────────────────────────────────────────────────
//
//   trash_count: usize          → cuántos archivos hay en la papelera interna
//                                 (se recalcula por eventos, no por frame).
//   pending_empty_trash: bool   → si el modal de "Vaciar papelera" está abierto.
//
// ─── NUEVOS MÉTODOS ───────────────────────────────────────────────────────────
//
//   refresh_trash_count() → escanea la carpeta de la papelera y actualiza
//                           `trash_count`. Se llama al arrancar y tras cada
//                           operación que modifique la papelera.
//
//   empty_trash()         → borra TODO el contenido de la carpeta de la papelera
//                           con fs::remove_file / remove_dir_all, marca
//                           `trash_path = None` en los FileEntry afectados
//                           (para que ya no puedan restaurarse), limpia
//                           `last_deleted` y refresca `trash_count`.
//
// ─── DOBLE CONTABILIDAD ───────────────────────────────────────────────────────
//
// La papelera vive en `%LOCALAPPDATA%\RustSearch\Trash\`. Se cuenta ESCANEANDO
// ESA CARPETA (no contando entradas con `trash_path.is_some()`), porque:
//   · El indexer filtra entradas `is_deleted` al cargar la caché, así que en
//     un reinicio no tendríamos constancia de los archivos borrados en la
//     sesión anterior. Pero los archivos SÍ siguen en disco.
//   · Escanear el disco garantiza que el usuario vacíe lo que hay realmente.
// ==============================================================================

use crate::indexer::{FileEntry, IndexerStatus};
use eframe::egui;
use egui_extras::{Column, TableBuilder};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, RwLock};

#[cfg(target_os = "windows")]
use windows::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_FILENAME, SND_NODEFAULT};
#[cfg(target_os = "windows")]
use windows::core::PCWSTR;

// ==============================================================================
// SECCIÓN 1: TIPOS AUXILIARES Y SONIDO NATIVO
// ==============================================================================

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SortColumn { Name, Size, Location }

#[derive(Clone)]
enum CtxAction {
    Open,
    OpenLocation,
    CopyText(String),
    DeleteTrash,
    DeletePermanent,
    Restore,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum KeyboardTarget {
    Search,
    Table,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ModalChoice {
    Yes,
    No,
}

fn search_bar_id() -> egui::Id {
    egui::Id::new("search_bar")
}

/// Ruta de la carpeta de la papelera interna.
fn trash_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap()
        .join("RustSearch")
        .join("Trash")
}

/// Botón de modal pintado a mano (evita el bug del "cuadro blanco" que
/// aparecía con `egui::Button::fill()` al navegar con las flechas).
fn draw_modal_button(ui: &mut egui::Ui, label: &str, selected: bool) -> bool {
    let desired = egui::vec2(80.0, 28.0);
    let (rect, response) = ui.allocate_exact_size(desired, egui::Sense::click());
    let hovered = response.hovered();
    let pressed = response.is_pointer_button_down_on();

    let bg = if pressed {
        egui::Color32::from_rgb(0, 80, 150)
    } else if selected {
        egui::Color32::from_rgb(0, 120, 215)
    } else if hovered {
        egui::Color32::from_rgb(60, 60, 72)
    } else {
        egui::Color32::from_rgb(42, 42, 52)
    };

    ui.painter().rect_filled(rect, egui::CornerRadius::same(4), bg);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(14.0),
        egui::Color32::WHITE,
    );

    response.clicked()
}

fn play_system_sound(filename: &str) {
    #[cfg(target_os = "windows")]
    {
        let path = format!("C:\\Windows\\Media\\{}", filename);
        let mut path_u16: Vec<u16> = path.encode_utf16().collect();
        path_u16.push(0);
        unsafe {
            let _ = PlaySoundW(
                PCWSTR(path_u16.as_ptr()),
                None,
                SND_FILENAME | SND_ASYNC | SND_NODEFAULT,
            );
        }
    }
}

fn play_error_sound() {
    #[cfg(target_os = "windows")]
    unsafe {
        let _ = windows::Win32::System::Diagnostics::Debug::Beep(400, 150);
    }
}

// ==============================================================================
// SECCIÓN 2: ESTADO
// ==============================================================================
pub struct RustSearchApp {
    pub entries: Arc<RwLock<Vec<FileEntry>>>,
    pub filtered_indices: Arc<Vec<usize>>,
    pub rx: Receiver<Vec<FileEntry>>,
    pub search_query: String,
    pub last_sent_query: String,
    pub search_tx: Sender<String>,
    pub search_rx: Receiver<Vec<usize>>,

    pub selected_index: Option<usize>,
    pub focused_column: usize,
    pub sort_column: Option<SortColumn>,
    pub sort_ascending: bool,
    pub needs_sort: bool,

    pub last_deleted: Option<(PathBuf, PathBuf)>,
    pub pending_delete: Option<(usize, bool)>,
    pub pending_restore: Option<PathBuf>,
    /// Si es true, el modal de "Vaciar papelera" está abierto.
    pub pending_empty_trash: bool,

    pub pending_choice: ModalChoice,

    /// Cuántos archivos hay actualmente en la papelera interna.
    /// Se actualiza con `refresh_trash_count()`.
    pub trash_count: usize,

    pub status: Arc<IndexerStatus>,
    pub vibrancy_applied: bool,
    pub startup_sound_played: bool,
    pub keyboard_target: KeyboardTarget,
}

impl RustSearchApp {
    pub fn new(
        _cc: &eframe::CreationContext<'_>,
        rx: Receiver<Vec<FileEntry>>,
        status: Arc<IndexerStatus>,
    ) -> Self {
        let entries: Arc<RwLock<Vec<FileEntry>>> = Arc::new(RwLock::new(Vec::new()));
        let entries_clone = Arc::clone(&entries);

        let (search_tx, search_rx_worker) = std::sync::mpsc::channel::<String>();
        let (result_tx, search_rx_ui) = std::sync::mpsc::channel::<Vec<usize>>();

        std::thread::spawn(move || {
            while let Ok(query) = search_rx_worker.recv() {
                let guard = entries_clone.read().unwrap();
                let mut result = Vec::new();
                if query.is_empty() {
                    result = (0..guard.len()).collect();
                } else {
                    let q = query.to_lowercase().replace('/', "\\");
                    for (i, e) in guard.iter().enumerate() {
                        if e.name_lower.contains(&q) || e.path_lower.contains(&q) {
                            result.push(i);
                        }
                    }
                }
                let _ = result_tx.send(result);
            }
        });

        // Conteo inicial de la papelera escaneando el disco.
        let trash_count = std::fs::read_dir(trash_dir())
            .map(|rd| rd.count())
            .unwrap_or(0);

        Self {
            entries,
            filtered_indices: Arc::new(Vec::new()),
            rx,
            search_query: String::new(),
            last_sent_query: String::new(),
            search_tx,
            search_rx: search_rx_ui,
            selected_index: None,
            focused_column: 0,
            sort_column: None,
            sort_ascending: true,
            needs_sort: false,
            last_deleted: None,
            pending_delete: None,
            pending_restore: None,
            pending_empty_trash: false,
            pending_choice: ModalChoice::No,
            trash_count,
            status,
            vibrancy_applied: false,
            startup_sound_played: false,
            keyboard_target: KeyboardTarget::Search,
        }
    }

    // ==========================================================================
    // SECCIÓN 3: ACCIONES
    // ==========================================================================

    /// Recalcula `trash_count` escaneando la carpeta de la papelera.
    /// Operación barata: solo se llama tras cambios en la papelera, nunca
    /// por frame.
    fn refresh_trash_count(&mut self) {
        self.trash_count = std::fs::read_dir(trash_dir())
            .map(|rd| rd.count())
            .unwrap_or(0);
    }

    /// Vacía por completo la papelera interna:
    ///   · Borra físicamente cada archivo/carpeta.
    ///   · Marca `trash_path = None` en los FileEntry afectados (para que no
    ///     se puedan restaurar).
    ///   · Limpia `last_deleted` (no hay nada restaurable).
    ///   · Refresca el conteo.
    fn empty_trash(&mut self) {
        let dir = trash_dir();
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for entry in rd.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let _ = std::fs::remove_dir_all(&path);
                } else {
                    let _ = std::fs::remove_file(&path);
                }
            }
        }
        // Marcar los entries en memoria como permanentemente eliminados.
        {
            let mut guard = self.entries.write().unwrap();
            for entry in guard.iter_mut() {
                if entry.trash_path.is_some() {
                    entry.trash_path = None;
                    // is_deleted sigue siendo true.
                }
            }
        }
        self.last_deleted = None;
        self.needs_sort = true;
        self.refresh_trash_count();
    }

    fn perform_delete(&mut self, idx: usize, permanent: bool) {
        {
            let mut guard = self.entries.write().unwrap();
            if let Some(&real_idx) = self.filtered_indices.get(idx) {
                let entry = &mut guard[real_idx];
                if entry.is_deleted { return; }
                let path = entry.path.clone();

                if permanent {
                    // ---------------------------------------------------------
                    // BORRADO PERMANENTE REAL (no va a la papelera).
                    // ---------------------------------------------------------
                    let result = if entry.is_dir {
                        std::fs::remove_dir_all(&path)
                    } else {
                        std::fs::remove_file(&path)
                    };

                    match result {
                        Ok(_) => {
                            entry.is_deleted = true;
                            entry.trash_path = None;
                        }
                        Err(e) if e.raw_os_error() == Some(5) => {
                            let path_str = path.to_string_lossy().to_string();
                            let args: Vec<&str> = if entry.is_dir {
                                vec!["/C", "rmdir", "/S", "/Q", &path_str]
                            } else {
                                vec!["/C", "del", "/F", "/Q", &path_str]
                            };
                            if let Ok(status) = runas::Command::new("cmd").args(&args).status() {
                                if status.success() {
                                    entry.is_deleted = true;
                                    entry.trash_path = None;
                                }
                            }
                        }
                        _ => { play_error_sound(); }
                    }
                } else {
                    // ---------------------------------------------------------
                    // MOVER A LA PAPELERA INTERNA (restaurable).
                    // ---------------------------------------------------------
                    let trash = trash_dir();
                    let _ = std::fs::create_dir_all(&trash);
                    let file_name = path.file_name().unwrap_or_default().to_string_lossy();
                    let timestamp = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_millis();
                    let trash_path = trash.join(format!("{}_{}", timestamp, file_name));

                    match std::fs::rename(&path, &trash_path) {
                        Ok(_) => {
                            entry.is_deleted = true;
                            entry.trash_path = Some(trash_path.clone());
                            self.last_deleted = Some((path, trash_path));
                        }
                        Err(e) if e.raw_os_error() == Some(5) => {
                            let path_str = path.to_string_lossy().to_string();
                            let trash_str = trash_path.to_string_lossy().to_string();
                            if let Ok(status) = runas::Command::new("cmd")
                                .args(&["/C", "move", "/Y", &path_str, &trash_str])
                                .status()
                            {
                                if status.success() {
                                    entry.is_deleted = true;
                                    entry.trash_path = Some(trash_path.clone());
                                    self.last_deleted = Some((path, trash_path));
                                }
                            }
                        }
                        _ => { play_error_sound(); }
                    }
                }
            }
        }
        if !permanent {
            self.refresh_trash_count();
        }
    }

    /// Restaura el último archivo borrado con Supr (Ctrl+Z).
    fn perform_restore(&mut self) {
        let Some((orig_path, trash_path)) = self.last_deleted.take() else {
            return;
        };
        if !trash_path.exists() {
            play_error_sound();
            return;
        }
        if std::fs::rename(&trash_path, &orig_path).is_err() {
            play_error_sound();
            return;
        }
        {
            let mut guard = self.entries.write().unwrap();
            if let Some(entry) = guard.iter_mut().find(|e| e.path == orig_path) {
                entry.is_deleted = false;
                entry.trash_path = None;
            }
        }
        self.needs_sort = true;
        self.refresh_trash_count();
    }

    /// Restaura una entrada concreta (menú contextual). Solo funciona si
    /// la entrada tiene `trash_path = Some(...)`.
    fn perform_restore_entry(&mut self, idx: usize) {
        {
            let mut guard = self.entries.write().unwrap();
            let Some(&real_idx) = self.filtered_indices.get(idx) else { return; };
            let entry = &mut guard[real_idx];
            if !entry.is_deleted { return; }

            let Some(trash_path) = entry.trash_path.clone() else {
                play_error_sound();
                return;
            };
            if !trash_path.exists() {
                play_error_sound();
                return;
            }
            let orig_path = entry.path.clone();
            if std::fs::rename(&trash_path, &orig_path).is_err() {
                play_error_sound();
                return;
            }
            entry.is_deleted = false;
            entry.trash_path = None;
        }
        self.needs_sort = true;
        self.refresh_trash_count();
    }
}

// ==============================================================================
// SECCIÓN 4: INTERFAZ GRÁFICA
// ==============================================================================
impl eframe::App for RustSearchApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // -------- 4.0: EFECTO CRISTAL Y SONIDO DE INICIO --------
        if !self.vibrancy_applied {
            #[cfg(target_os = "windows")]
            {
                let _ = window_vibrancy::apply_acrylic(&*frame, Some((16, 16, 22, 160)));
            }
            self.vibrancy_applied = true;
        }
        if !self.startup_sound_played {
            play_system_sound("Windows Proximity Notification.wav");
            self.startup_sound_played = true;
        }

        // -------- 4.1: ESTILO --------
        ctx.style_mut(|style| {
            style.animation_time = 0.15;
            style.interaction.selectable_labels = true;

            style.visuals.selection.bg_fill = egui::Color32::from_rgb(0, 120, 215);
            style.visuals.selection.stroke = egui::Stroke::new(1.0_f32, egui::Color32::WHITE);
            style.visuals.faint_bg_color = egui::Color32::TRANSPARENT;

            style.visuals.widgets.noninteractive.bg_stroke =
                egui::Stroke::new(1.0_f32, egui::Color32::from_rgba_premultiplied(255, 255, 255, 25));
            style.visuals.widgets.inactive.bg_stroke = egui::Stroke::NONE;
            style.visuals.widgets.hovered.bg_stroke = egui::Stroke::NONE;
            style.visuals.widgets.active.bg_stroke = egui::Stroke::NONE;
            style.visuals.widgets.open.bg_stroke = egui::Stroke::NONE;

            style.visuals.panel_fill = egui::Color32::from_rgba_premultiplied(16, 16, 22, 140);
            style.visuals.extreme_bg_color = egui::Color32::from_rgba_premultiplied(255, 255, 255, 15);
            style.visuals.window_fill = egui::Color32::from_rgba_premultiplied(26, 26, 32, 235);
            style.visuals.window_stroke =
                egui::Stroke::new(1.0_f32, egui::Color32::from_rgba_premultiplied(255, 255, 255, 25));

            style.visuals.widgets.hovered.weak_bg_fill =
                egui::Color32::from_rgba_premultiplied(255, 255, 255, 20);
            style.visuals.widgets.hovered.bg_fill =
                egui::Color32::from_rgba_premultiplied(255, 255, 255, 20);
            style.visuals.widgets.active.weak_bg_fill =
                egui::Color32::from_rgba_premultiplied(255, 255, 255, 35);
            style.visuals.widgets.active.bg_fill =
                egui::Color32::from_rgba_premultiplied(255, 255, 255, 35);

            style.visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(6);
            style.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(6);
            style.visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(6);
            style.visuals.widgets.active.corner_radius = egui::CornerRadius::same(6);
        });

        // -------- 4.2: INGESTA DE DATOS --------
        let mut received_data = false;
        while let Ok(chunk) = self.rx.try_recv() {
            self.entries.write().unwrap().extend(chunk);
            received_data = true;
        }
        if received_data {
            let _ = self.search_tx.send(self.search_query.clone());
            self.last_sent_query = self.search_query.clone();
        }

        // -------- 4.3: BÚSQUEDA --------
        if self.search_query != self.last_sent_query {
            let _ = self.search_tx.send(self.search_query.clone());
            self.last_sent_query = self.search_query.clone();
        }

        if let Ok(indices) = self.search_rx.try_recv() {
            self.filtered_indices = Arc::new(indices);
            self.needs_sort = true;
            ctx.request_repaint();
            if let Some(idx) = self.selected_index {
                if self.filtered_indices.is_empty() {
                    self.selected_index = None;
                } else if idx >= self.filtered_indices.len() {
                    self.selected_index = Some(self.filtered_indices.len() - 1);
                }
            }
        }

        // -------- 4.4: ORDEN --------
        if self.needs_sort {
            if let Some(col) = self.sort_column {
                let guard = self.entries.read().unwrap();
                let mut indices = (*self.filtered_indices).clone();
                let asc = self.sort_ascending;
                indices.sort_by(|&a, &b| {
                    let ea = &guard[a];
                    let eb = &guard[b];
                    let ord = match col {
                        SortColumn::Name => ea.name_lower.cmp(&eb.name_lower),
                        SortColumn::Size => ea.size.cmp(&eb.size),
                        SortColumn::Location => {
                            let pa = ea.path.parent()
                                .map(|p| p.to_string_lossy().to_lowercase())
                                .unwrap_or_default();
                            let pb = eb.path.parent()
                                .map(|p| p.to_string_lossy().to_lowercase())
                                .unwrap_or_default();
                            pa.cmp(&pb)
                        }
                    };
                    if asc { ord } else { ord.reverse() }
                });
                self.filtered_indices = Arc::new(indices);
            }
            self.needs_sort = false;
        }

        // -------- 4.4.5: ESTADO DE MODAL — CONTROL GLOBAL --------
        let modal_open = self.pending_delete.is_some()
            || self.pending_restore.is_some()
            || self.pending_empty_trash;

        if modal_open {
            let escape_pressed = ctx.input(|i| i.key_pressed(egui::Key::Escape));
            if escape_pressed {
                ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
                self.pending_delete = None;
                self.pending_restore = None;
                self.pending_empty_trash = false;
            }

            ctx.memory_mut(|m| {
                if m.focused() == Some(search_bar_id()) {
                    m.surrender_focus(search_bar_id());
                }
            });
        }

        // -------- 4.5: TECLADO DE LA TABLA --------
        let mut scroll_to_selected = false;
        let mut user_pressed_copy = false;
        let mut do_delete = false;
        let mut do_delete_perm = false;

        if !modal_open && self.keyboard_target == KeyboardTarget::Table {
            ctx.memory_mut(|m| {
                if m.focused() == Some(search_bar_id()) {
                    m.surrender_focus(search_bar_id());
                }
            });

            ctx.input(|i| {
                let has_copy_event = i.events.iter().any(|e| matches!(e, egui::Event::Copy));
                let has_ctrl_c = i.key_pressed(egui::Key::C)
                    && i.modifiers.ctrl
                    && !i.modifiers.shift
                    && !i.modifiers.alt;
                if has_copy_event || has_ctrl_c {
                    user_pressed_copy = true;
                }
            });

            ctx.input_mut(|i| {
                i.events.retain(|e| {
                    if matches!(e, egui::Event::Cut) {
                        do_delete_perm = true;
                        return false;
                    }
                    if let egui::Event::Key {
                        key: egui::Key::Delete,
                        pressed: true,
                        modifiers,
                        ..
                    } = e
                    {
                        if !modifiers.ctrl && !modifiers.alt {
                            let shift = modifiers.shift || i.modifiers.shift;
                            if shift { do_delete_perm = true; } else { do_delete = true; }
                            return false;
                        }
                    }
                    true
                });
            });

            ctx.input_mut(|i| {
                let has_data = !self.filtered_indices.is_empty();

                if i.key_pressed(egui::Key::ArrowDown)
                    && !i.modifiers.ctrl && !i.modifiers.shift && !i.modifiers.alt
                    && has_data
                {
                    i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown);
                    let new = match self.selected_index {
                        None => Some(0),
                        Some(idx) if idx + 1 < self.filtered_indices.len() => Some(idx + 1),
                        other => other,
                    };
                    if new != self.selected_index {
                        self.selected_index = new;
                        scroll_to_selected = true;
                    }
                }
                if i.key_pressed(egui::Key::ArrowUp)
                    && !i.modifiers.ctrl && !i.modifiers.shift && !i.modifiers.alt
                    && has_data
                {
                    i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp);
                    let new = match self.selected_index {
                        None => Some(0),
                        Some(idx) if idx > 0 => Some(idx - 1),
                        other => other,
                    };
                    if new != self.selected_index {
                        self.selected_index = new;
                        scroll_to_selected = true;
                    }
                }
                if i.key_pressed(egui::Key::ArrowLeft)
                    && !i.modifiers.ctrl && !i.modifiers.shift && !i.modifiers.alt
                    && has_data
                {
                    i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft);
                    if self.selected_index.is_none() {
                        self.selected_index = Some(0);
                        scroll_to_selected = true;
                    }
                    self.focused_column = 0;
                }
                if i.key_pressed(egui::Key::ArrowRight)
                    && !i.modifiers.ctrl && !i.modifiers.shift && !i.modifiers.alt
                    && has_data
                {
                    i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight);
                    if self.selected_index.is_none() {
                        self.selected_index = Some(0);
                        scroll_to_selected = true;
                    }
                    self.focused_column = 1;
                }

                if i.key_pressed(egui::Key::Enter)
                    && !i.modifiers.ctrl && !i.modifiers.shift && !i.modifiers.alt
                {
                    i.consume_key(egui::Modifiers::NONE, egui::Key::Enter);
                    if let Some(idx) = self.selected_index {
                        if let Some(&real_idx) = self.filtered_indices.get(idx) {
                            let guard = self.entries.read().unwrap();
                            let entry = &guard[real_idx];
                            if !entry.is_deleted {
                                let target = if self.focused_column == 1 {
                                    entry.path.parent()
                                        .map(|p| p.to_path_buf())
                                        .unwrap_or_else(|| entry.path.clone())
                                } else {
                                    entry.path.clone()
                                };
                                let _ = open::that(target);
                            }
                        }
                    }
                }

                if i.key_pressed(egui::Key::Z)
                    && i.modifiers.ctrl && !i.modifiers.shift && !i.modifiers.alt
                {
                    i.consume_key(egui::Modifiers::CTRL, egui::Key::Z);
                    if self.last_deleted.is_some() {
                        self.pending_restore = Some(PathBuf::from("dummy"));
                        self.pending_choice = ModalChoice::No;
                    } else {
                        play_error_sound();
                    }
                }

                if i.key_pressed(egui::Key::F5) {
                    i.consume_key(egui::Modifiers::NONE, egui::Key::F5);
                    self.entries.write().unwrap().retain(|e| !e.is_deleted);
                    self.needs_sort = true;
                    let _ = self.search_tx.send(self.search_query.clone());
                }
            });

            if let Some(idx) = self.selected_index {
                if do_delete {
                    self.pending_delete = Some((idx, false));
                    self.pending_choice = ModalChoice::No;
                }
                if do_delete_perm {
                    self.pending_delete = Some((idx, true));
                    self.pending_choice = ModalChoice::No;
                }
            }
        }

        // -------- 4.6: MODAL DE ELIMINACIÓN --------
        if let Some((idx, perm)) = self.pending_delete {
            let name = if let Some(&real_idx) = self.filtered_indices.get(idx) {
                self.entries.read().unwrap()[real_idx].name.clone()
            } else {
                String::new()
            };

            let mut confirmed = false;
            let mut cancelled = false;

            egui::Window::new("Confirmar Eliminación")
                .collapsible(false)
                .resizable(false)
                .min_width(280.0)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label(format!(
                        "¿Eliminar '{}' {}?",
                        name,
                        if perm { "permanentemente" } else { "a la papelera" }
                    ));
                    ui.add_space(10.0);

                    let key_tab = ctx.input(|i| i.key_pressed(egui::Key::Tab));
                    let key_prev = ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft))
                        || ctx.input(|i| i.key_pressed(egui::Key::ArrowUp));
                    let key_next = ctx.input(|i| i.key_pressed(egui::Key::ArrowRight))
                        || ctx.input(|i| i.key_pressed(egui::Key::ArrowDown));

                    if key_tab || key_prev || key_next {
                        self.pending_choice = match self.pending_choice {
                            ModalChoice::Yes => ModalChoice::No,
                            ModalChoice::No => ModalChoice::Yes,
                        };
                        ctx.input_mut(|i| {
                            i.consume_key(egui::Modifiers::NONE, egui::Key::Tab);
                            i.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab);
                            i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft);
                            i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight);
                            i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp);
                            i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown);
                        });
                    }

                    let key_enter = ctx.input(|i| i.key_pressed(egui::Key::Enter));
                    let key_space = ctx.input(|i| i.key_pressed(egui::Key::Space));
                    if key_enter || key_space {
                        ctx.input_mut(|i| {
                            i.consume_key(egui::Modifiers::NONE, egui::Key::Enter);
                            i.consume_key(egui::Modifiers::NONE, egui::Key::Space);
                        });
                        match self.pending_choice {
                            ModalChoice::Yes => confirmed = true,
                            ModalChoice::No => cancelled = true,
                        }
                    }

                    ui.horizontal(|ui| {
                        if draw_modal_button(ui, "Sí", self.pending_choice == ModalChoice::Yes) {
                            self.pending_choice = ModalChoice::Yes;
                            confirmed = true;
                        }
                        if draw_modal_button(ui, "No", self.pending_choice == ModalChoice::No) {
                            self.pending_choice = ModalChoice::No;
                            cancelled = true;
                        }
                    });
                });

            if confirmed {
                self.perform_delete(idx, perm);
                self.pending_delete = None;
            }
            if cancelled {
                self.pending_delete = None;
            }
        }

        // -------- 4.6b: MODAL DE RESTAURACIÓN --------
        if self.pending_restore.is_some() {
            let restore_name = self.last_deleted.as_ref()
                .and_then(|(p, _)| p.file_name().map(|n| n.to_string_lossy().to_string()))
                .unwrap_or_else(|| "?".to_string());

            let mut confirmed = false;
            let mut cancelled = false;

            egui::Window::new("Confirmar Restauración")
                .collapsible(false)
                .resizable(false)
                .min_width(280.0)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label(format!("¿Restaurar '{}'?", restore_name));
                    ui.add_space(10.0);

                    let key_tab = ctx.input(|i| i.key_pressed(egui::Key::Tab));
                    let key_prev = ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft))
                        || ctx.input(|i| i.key_pressed(egui::Key::ArrowUp));
                    let key_next = ctx.input(|i| i.key_pressed(egui::Key::ArrowRight))
                        || ctx.input(|i| i.key_pressed(egui::Key::ArrowDown));

                    if key_tab || key_prev || key_next {
                        self.pending_choice = match self.pending_choice {
                            ModalChoice::Yes => ModalChoice::No,
                            ModalChoice::No => ModalChoice::Yes,
                        };
                        ctx.input_mut(|i| {
                            i.consume_key(egui::Modifiers::NONE, egui::Key::Tab);
                            i.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab);
                            i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft);
                            i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight);
                            i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp);
                            i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown);
                        });
                    }

                    let key_enter = ctx.input(|i| i.key_pressed(egui::Key::Enter));
                    let key_space = ctx.input(|i| i.key_pressed(egui::Key::Space));
                    if key_enter || key_space {
                        ctx.input_mut(|i| {
                            i.consume_key(egui::Modifiers::NONE, egui::Key::Enter);
                            i.consume_key(egui::Modifiers::NONE, egui::Key::Space);
                        });
                        match self.pending_choice {
                            ModalChoice::Yes => confirmed = true,
                            ModalChoice::No => cancelled = true,
                        }
                    }

                    ui.horizontal(|ui| {
                        if draw_modal_button(ui, "Sí", self.pending_choice == ModalChoice::Yes) {
                            self.pending_choice = ModalChoice::Yes;
                            confirmed = true;
                        }
                        if draw_modal_button(ui, "No", self.pending_choice == ModalChoice::No) {
                            self.pending_choice = ModalChoice::No;
                            cancelled = true;
                        }
                    });
                });

            if confirmed {
                self.perform_restore();
                self.pending_restore = None;
            }
            if cancelled {
                self.pending_restore = None;
            }
        }

        // -------- 4.6c: MODAL DE VACIAR PAPELERA --------
        // Muestra el conteo y pide confirmación antes de eliminar todo
        // PERMANENTEMENTE. Mismo patrón de navegación que los otros modales.
        if self.pending_empty_trash {
            let count = self.trash_count;
            let mut confirmed = false;
            let mut cancelled = false;

            egui::Window::new("Vaciar Papelera")
                .collapsible(false)
                .resizable(false)
                .min_width(340.0)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label("⚠️ ¿Vaciar la papelera permanentemente?");
                    ui.add_space(4.0);
                    ui.label(format!(
                        "Se eliminarán {} archivo(s) para siempre.",
                        count
                    ));
                    ui.add_space(10.0);

                    let key_tab = ctx.input(|i| i.key_pressed(egui::Key::Tab));
                    let key_prev = ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft))
                        || ctx.input(|i| i.key_pressed(egui::Key::ArrowUp));
                    let key_next = ctx.input(|i| i.key_pressed(egui::Key::ArrowRight))
                        || ctx.input(|i| i.key_pressed(egui::Key::ArrowDown));

                    if key_tab || key_prev || key_next {
                        self.pending_choice = match self.pending_choice {
                            ModalChoice::Yes => ModalChoice::No,
                            ModalChoice::No => ModalChoice::Yes,
                        };
                        ctx.input_mut(|i| {
                            i.consume_key(egui::Modifiers::NONE, egui::Key::Tab);
                            i.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab);
                            i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft);
                            i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight);
                            i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp);
                            i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown);
                        });
                    }

                    let key_enter = ctx.input(|i| i.key_pressed(egui::Key::Enter));
                    let key_space = ctx.input(|i| i.key_pressed(egui::Key::Space));
                    if key_enter || key_space {
                        ctx.input_mut(|i| {
                            i.consume_key(egui::Modifiers::NONE, egui::Key::Enter);
                            i.consume_key(egui::Modifiers::NONE, egui::Key::Space);
                        });
                        match self.pending_choice {
                            ModalChoice::Yes => confirmed = true,
                            ModalChoice::No => cancelled = true,
                        }
                    }

                    ui.horizontal(|ui| {
                        if draw_modal_button(ui, "Sí", self.pending_choice == ModalChoice::Yes) {
                            self.pending_choice = ModalChoice::Yes;
                            confirmed = true;
                        }
                        if draw_modal_button(ui, "No", self.pending_choice == ModalChoice::No) {
                            self.pending_choice = ModalChoice::No;
                            cancelled = true;
                        }
                    });
                });

            if confirmed {
                self.empty_trash();
                self.pending_empty_trash = false;
            }
            if cancelled {
                self.pending_empty_trash = false;
            }
        }

        // -------- 4.7: PANEL SUPERIOR — BÚSQUEDA + BOTONES DE PAPELERA --------
        let mut search_took_over = false;

        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("🔍");
                let search_resp = ui.add(
                    egui::TextEdit::singleline(&mut self.search_query)
                        .id(search_bar_id())
                        .hint_text("Buscar...")
                        .desired_width(f32::INFINITY),
                );
                if !modal_open && (search_resp.clicked() || search_resp.changed()) {
                    search_took_over = true;
                }
            });
            ui.add_space(4.0);

            // --- Fila de botones de la papelera ---
            ui.horizontal(|ui| {
                // Botón 1: abrir la carpeta de la papelera en el Explorador.
                let open_resp = ui.add_enabled(
                    !modal_open,
                    egui::Button::new("🗂️ Abrir papelera"),
                );
                if open_resp.clicked() {
                    let dir = trash_dir();
                    let _ = std::fs::create_dir_all(&dir);
                    let _ = open::that(dir);
                }

                // Botón 2: vaciar papelera (con conteo en vivo).
                let empty_label = if self.trash_count == 0 {
                    "🗑️ Vaciar papelera".to_string()
                } else {
                    format!("🗑️ Vaciar papelera ({})", self.trash_count)
                };
                let empty_enabled = !modal_open && self.trash_count > 0;
                let empty_resp = ui.add_enabled(
                    empty_enabled,
                    egui::Button::new(empty_label),
                );
                if empty_resp.clicked() {
                    self.pending_empty_trash = true;
                    self.pending_choice = ModalChoice::No;
                }
            });
            ui.add_space(4.0);
        });

        if search_took_over {
            self.keyboard_target = KeyboardTarget::Search;
        }

        // -------- 4.8: PANEL INFERIOR (ESTADO) --------
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                let count = self.status.count.load(Ordering::Relaxed);
                let is_done = self.status.is_done.load(Ordering::Relaxed);
                if is_done {
                    ui.label(format!("✅ {} archivos indexados", count));
                } else {
                    ui.spinner();
                    ui.label(format!("⏳ Indexando... {}", count));
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new("🟠 Carpeta existe")
                            .color(egui::Color32::from_rgb(255, 165, 0)),
                    );
                    ui.label(egui::RichText::new("🔴 Eliminado").color(egui::Color32::RED));
                    ui.label("Leyenda:");
                });
            });
            ui.add_space(2.0);
        });

        // -------- 4.9: TABLA --------
        egui::CentralPanel::default().show(ctx, |ui| {
            let default_color = ui.style().visuals.text_color();
            let mut clicked_row: Option<(usize, usize)> = None;
            let mut dblclicked_row = None;
            let mut ctx_action = None;
            let mut sort_click = None;

            let filtered = Arc::clone(&self.filtered_indices);
            let entries = self.entries.read().unwrap();
            let pointer_pos = ui.ctx().pointer_hover_pos();
            let search_id = search_bar_id();

            let mut table_builder = TableBuilder::new(ui)
                .resizable(true)
                .sense(egui::Sense::click())
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .column(Column::exact(30.0))
                .column(Column::initial(280.0).at_least(100.0).clip(true))
                .column(Column::initial(90.0).at_least(60.0))
                .column(Column::remainder().at_least(150.0).clip(true));

            if scroll_to_selected {
                if let Some(idx) = self.selected_index {
                    table_builder = table_builder.scroll_to_row(idx, Some(egui::Align::Center));
                }
            }

            table_builder
                .header(22.0, |mut header| {
                    header.col(|ui| { ui.strong(""); });
                    header.col(|ui| {
                        let label = if self.sort_column == Some(SortColumn::Name) {
                            if self.sort_ascending { "Nombre ▲" } else { "Nombre ▼" }
                        } else { "Nombre" };
                        if ui.add(egui::Button::new(label).frame(false)).clicked() {
                            sort_click = Some(SortColumn::Name);
                        }
                    });
                    header.col(|ui| {
                        let label = if self.sort_column == Some(SortColumn::Size) {
                            if self.sort_ascending { "Tamaño ▲" } else { "Tamaño ▼" }
                        } else { "Tamaño" };
                        if ui.add(egui::Button::new(label).frame(false)).clicked() {
                            sort_click = Some(SortColumn::Size);
                        }
                    });
                    header.col(|ui| {
                        let label = if self.sort_column == Some(SortColumn::Location) {
                            if self.sort_ascending { "Ubicación ▲" } else { "Ubicación ▼" }
                        } else { "Ubicación" };
                        if ui.add(egui::Button::new(label).frame(false)).clicked() {
                            sort_click = Some(SortColumn::Location);
                        }
                    });
                })
                .body(|body| {
                    body.rows(22.0, filtered.len(), |mut row| {
                        let row_index = row.index();
                        let entry = &entries[filtered[row_index]];

                        let is_sel = self.selected_index == Some(row_index);
                        let parent_exists = entry.path.parent().map_or(false, |p| p.exists());

                        let name_color = if is_sel {
                            egui::Color32::WHITE
                        } else if entry.is_deleted {
                            egui::Color32::RED
                        } else {
                            default_color
                        };
                        let loc_color = if is_sel {
                            egui::Color32::WHITE
                        } else if entry.is_deleted {
                            if parent_exists {
                                egui::Color32::from_rgb(255, 165, 0)
                            } else {
                                egui::Color32::RED
                            }
                        } else {
                            default_color
                        };

                        let click_sense = egui::Sense::click();
                        let is_restorable = entry.is_deleted && entry.trash_path.is_some();

                        // ---------- COLUMNA 0: ICONO ----------
                        row.col(|ui| {
                            let rect = ui.max_rect();
                            let is_row_hovered = pointer_pos
                                .map_or(false, |pos| pos.y >= rect.min.y && pos.y <= rect.max.y);

                            if is_sel {
                                ui.painter().rect_filled(
                                    rect, 0.0,
                                    egui::Color32::from_rgba_premultiplied(0, 120, 215, 100),
                                );
                            } else if is_row_hovered {
                                ui.painter().rect_filled(
                                    rect, 0.0,
                                    egui::Color32::from_rgba_premultiplied(255, 255, 255, 20),
                                );
                            }

                            let resp = ui.interact(rect, ui.id().with("col_icon"), click_sense);
                            let icon = if entry.is_dir { "📁" } else { "📄" };

                            ui.scope_builder(
                                egui::UiBuilder::new()
                                    .max_rect(rect)
                                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
                                |ui| {
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(icon).color(name_color),
                                        )
                                        .selectable(false),
                                    );
                                },
                            );

                            if resp.clicked() || resp.secondary_clicked() {
                                self.keyboard_target = KeyboardTarget::Table;
                                ui.ctx().memory_mut(|m| m.surrender_focus(search_id));
                                clicked_row = Some((row_index, 0));
                            }
                            if resp.double_clicked() {
                                dblclicked_row = Some(row_index);
                            }
                        });

                        // ---------- COLUMNA 1: NOMBRE ----------
                        row.col(|ui| {
                            let rect = ui.max_rect();
                            let is_row_hovered = pointer_pos
                                .map_or(false, |pos| pos.y >= rect.min.y && pos.y <= rect.max.y);

                            if is_sel {
                                let bg = if self.focused_column == 0 {
                                    egui::Color32::from_rgb(0, 90, 190)
                                } else {
                                    egui::Color32::from_rgba_premultiplied(0, 120, 215, 100)
                                };
                                ui.painter().rect_filled(rect, 0.0, bg);
                            } else if is_row_hovered {
                                ui.painter().rect_filled(
                                    rect, 0.0,
                                    egui::Color32::from_rgba_premultiplied(255, 255, 255, 20),
                                );
                            }

                            let cell_resp =
                                ui.interact(rect, ui.id().with("col_name_bg"), click_sense);

                            let mut txt = egui::RichText::new(&entry.name).color(name_color);
                            if entry.is_deleted { txt = txt.strikethrough(); }
                            if is_sel && self.focused_column == 0 { txt = txt.strong(); }

                            ui.scope_builder(
                                egui::UiBuilder::new()
                                    .max_rect(rect)
                                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
                                |ui| {
                                    ui.add(egui::Label::new(txt).selectable(false));
                                },
                            );

                            if cell_resp.clicked() || cell_resp.secondary_clicked() {
                                self.keyboard_target = KeyboardTarget::Table;
                                ui.ctx().memory_mut(|m| m.surrender_focus(search_id));
                                clicked_row = Some((row_index, 0));
                            }
                            if cell_resp.double_clicked() {
                                dblclicked_row = Some(row_index);
                            }

                            cell_resp.context_menu(|ui| {
                                if is_restorable {
                                    if ui.button("♻️ Restaurar").clicked() {
                                        ctx_action = Some((row_index, CtxAction::Restore));
                                        ui.close();
                                    }
                                    ui.separator();
                                }
                                if !entry.is_deleted {
                                    if ui.button("📂 Abrir").clicked() {
                                        ctx_action = Some((row_index, CtxAction::Open));
                                        ui.close();
                                    }
                                    if ui.button("📁 Abrir ubicación").clicked() {
                                        ctx_action = Some((row_index, CtxAction::OpenLocation));
                                        ui.close();
                                    }
                                }
                                if ui
                                    .add(egui::Button::new("📋 Copiar").shortcut_text("Ctrl+C"))
                                    .clicked()
                                {
                                    ctx_action =
                                        Some((row_index, CtxAction::CopyText(entry.name.clone())));
                                    ui.close();
                                }
                                if !entry.is_deleted {
                                    ui.separator();
                                    if ui
                                        .add(egui::Button::new("🗑️ Eliminar").shortcut_text("Supr"))
                                        .clicked()
                                    {
                                        ctx_action = Some((row_index, CtxAction::DeleteTrash));
                                        ui.close();
                                    }
                                    if ui
                                        .add(
                                            egui::Button::new("❌ Eliminar permanentemente")
                                                .shortcut_text("Shift+Supr"),
                                        )
                                        .clicked()
                                    {
                                        ctx_action = Some((row_index, CtxAction::DeletePermanent));
                                        ui.close();
                                    }
                                }
                            });
                        });

                        // ---------- COLUMNA 2: TAMAÑO ----------
                        row.col(|ui| {
                            let rect = ui.max_rect();
                            let is_row_hovered = pointer_pos
                                .map_or(false, |pos| pos.y >= rect.min.y && pos.y <= rect.max.y);

                            if is_sel {
                                ui.painter().rect_filled(
                                    rect, 0.0,
                                    egui::Color32::from_rgba_premultiplied(0, 120, 215, 100),
                                );
                            } else if is_row_hovered {
                                ui.painter().rect_filled(
                                    rect, 0.0,
                                    egui::Color32::from_rgba_premultiplied(255, 255, 255, 20),
                                );
                            }

                            let resp = ui.interact(rect, ui.id().with("col_size"), click_sense);
                            let s = if entry.is_dir {
                                String::new()
                            } else if entry.size > 1024 * 1024 {
                                format!("{:.1} MB", entry.size as f64 / (1024.0 * 1024.0))
                            } else {
                                format!("{:.1} KB", entry.size as f64 / 1024.0)
                            };

                            ui.scope_builder(
                                egui::UiBuilder::new()
                                    .max_rect(rect)
                                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
                                |ui| {
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(s).color(name_color),
                                        )
                                        .selectable(false),
                                    );
                                },
                            );

                            if resp.clicked() || resp.secondary_clicked() {
                                self.keyboard_target = KeyboardTarget::Table;
                                ui.ctx().memory_mut(|m| m.surrender_focus(search_id));
                                clicked_row = Some((row_index, 0));
                            }
                        });

                        // ---------- COLUMNA 3: UBICACIÓN ----------
                        row.col(|ui| {
                            let rect = ui.max_rect();
                            let is_row_hovered = pointer_pos
                                .map_or(false, |pos| pos.y >= rect.min.y && pos.y <= rect.max.y);

                            if is_sel {
                                let bg = if self.focused_column == 1 {
                                    egui::Color32::from_rgb(0, 90, 190)
                                } else {
                                    egui::Color32::from_rgba_premultiplied(0, 120, 215, 100)
                                };
                                ui.painter().rect_filled(rect, 0.0, bg);
                            } else if is_row_hovered {
                                ui.painter().rect_filled(
                                    rect, 0.0,
                                    egui::Color32::from_rgba_premultiplied(255, 255, 255, 20),
                                );
                            }

                            let cell_resp =
                                ui.interact(rect, ui.id().with("col_loc_bg"), click_sense);

                            let parent = entry.path.parent().unwrap_or(&entry.path).to_string_lossy();
                            let mut txt = egui::RichText::new(parent).color(loc_color);
                            if entry.is_deleted { txt = txt.strikethrough(); }
                            if is_sel && self.focused_column == 1 { txt = txt.strong(); }

                            ui.scope_builder(
                                egui::UiBuilder::new()
                                    .max_rect(rect)
                                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
                                |ui| {
                                    ui.add(egui::Label::new(txt).selectable(false));
                                },
                            );

                            if cell_resp.clicked() || cell_resp.secondary_clicked() {
                                self.keyboard_target = KeyboardTarget::Table;
                                ui.ctx().memory_mut(|m| m.surrender_focus(search_id));
                                clicked_row = Some((row_index, 1));
                            }
                            if cell_resp.double_clicked() {
                                dblclicked_row = Some(row_index);
                            }

                            cell_resp.context_menu(|ui| {
                                if is_restorable {
                                    if ui.button("♻️ Restaurar").clicked() {
                                        ctx_action = Some((row_index, CtxAction::Restore));
                                        ui.close();
                                    }
                                    ui.separator();
                                }
                                if !entry.is_deleted {
                                    if ui.button("📂 Abrir").clicked() {
                                        ctx_action = Some((row_index, CtxAction::Open));
                                        ui.close();
                                    }
                                    if ui.button("📁 Abrir ubicación").clicked() {
                                        ctx_action = Some((row_index, CtxAction::OpenLocation));
                                        ui.close();
                                    }
                                }
                                if ui
                                    .add(egui::Button::new("📋 Copiar").shortcut_text("Ctrl+C"))
                                    .clicked()
                                {
                                    ctx_action = Some((
                                        row_index,
                                        CtxAction::CopyText(
                                            entry.path.to_string_lossy().to_string(),
                                        ),
                                    ));
                                    ui.close();
                                }
                                if !entry.is_deleted {
                                    ui.separator();
                                    if ui
                                        .add(egui::Button::new("🗑️ Eliminar").shortcut_text("Supr"))
                                        .clicked()
                                    {
                                        ctx_action = Some((row_index, CtxAction::DeleteTrash));
                                        ui.close();
                                    }
                                    if ui
                                        .add(
                                            egui::Button::new("❌ Eliminar permanentemente")
                                                .shortcut_text("Shift+Supr"),
                                        )
                                        .clicked()
                                    {
                                        ctx_action = Some((row_index, CtxAction::DeletePermanent));
                                        ui.close();
                                    }
                                }
                            });
                        });
                    });
                });
            drop(entries);

            if let Some((idx, col)) = clicked_row {
                self.selected_index = Some(idx);
                self.focused_column = col;
                ctx.request_repaint();
            }

            if let Some(col) = sort_click {
                if self.sort_column == Some(col) {
                    self.sort_ascending = !self.sort_ascending;
                } else {
                    self.sort_column = Some(col);
                    self.sort_ascending = matches!(col, SortColumn::Name | SortColumn::Location);
                }
                self.needs_sort = true;
            }

            if let Some(idx) = dblclicked_row {
                let entry = &self.entries.read().unwrap()[self.filtered_indices[idx]];
                if !entry.is_deleted {
                    let target = if self.focused_column == 1 {
                        entry.path.parent().unwrap_or(&entry.path)
                    } else {
                        &entry.path
                    };
                    let _ = open::that(target);
                }
            }

            if let Some((idx, action)) = ctx_action {
                match action {
                    CtxAction::Open => {
                        let _ = open::that(
                            &self.entries.read().unwrap()[self.filtered_indices[idx]].path,
                        );
                    }
                    CtxAction::OpenLocation => {
                        let e = &self.entries.read().unwrap()[self.filtered_indices[idx]];
                        let _ = open::that(e.path.parent().unwrap_or(&e.path));
                    }
                    CtxAction::CopyText(txt) => {
                        ctx.copy_text(txt);
                    }
                    CtxAction::DeleteTrash => {
                        self.pending_delete = Some((idx, false));
                        self.pending_choice = ModalChoice::No;
                    }
                    CtxAction::DeletePermanent => {
                        self.pending_delete = Some((idx, true));
                        self.pending_choice = ModalChoice::No;
                    }
                    CtxAction::Restore => {
                        self.perform_restore_entry(idx);
                    }
                }
            }

            if user_pressed_copy && self.keyboard_target == KeyboardTarget::Table {
                if let Some(idx) = self.selected_index {
                    if let Some(&real_idx) = self.filtered_indices.get(idx) {
                        let entry = &self.entries.read().unwrap()[real_idx];
                        let text = if self.focused_column == 1 {
                            entry.path.to_string_lossy().to_string()
                        } else {
                            entry.name.clone()
                        };
                        ctx.copy_text(text);
                    }
                }
            }
        });
    }
}