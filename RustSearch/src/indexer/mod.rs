// ==============================================================================
// ARCHIVO: src/indexer/mod.rs
// DESCRIPCIÓN: Escaneo, caché en disco y status compartido.
//
// ─── CAMBIO RESPECTO A LA VERSIÓN ANTERIOR ────────────────────────────────────
//
// `FileEntry` ahora tiene `trash_path: Option<PathBuf>`.
//
//   · `trash_path = Some(p)` → el archivo está en la papelera interna de
//     RustSearch en la ruta `p`. Es RESTAURABLE vía `fs::rename`.
//   · `trash_path = None`    → el archivo NO está en la papelera interna:
//     o no está borrado (`is_deleted = false`), o fue eliminado
//     permanentemente (`is_deleted = true`).
//
// `#[serde(default)]` permite deserializar cachés antiguas: los campos
// faltantes se rellenan con `None`. Es decir, si el usuario tenía un
// `index.bin` viejo, no se rompe: los `FileEntry` cargados tendrán
// `trash_path = None` automáticamente.
// ==============================================================================

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;

// ==============================================================================
// SECCIÓN 1: ESTRUCTURAS
// ==============================================================================
#[derive(Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub name: String,
    pub name_lower: String,
    pub path: PathBuf,
    pub path_lower: String,
    pub is_dir: bool,
    pub size: u64,
    pub is_deleted: bool,

    /// Si `Some`, el archivo está en la papelera interna de RustSearch.
    /// Si `None`, no está borrado o fue eliminado permanentemente.
    #[serde(default)]
    pub trash_path: Option<PathBuf>,
}

pub struct IndexerStatus {
    pub count: AtomicUsize,
    pub is_done: AtomicBool,
}

impl IndexerStatus {
    pub fn new() -> Self {
        Self {
            count: AtomicUsize::new(0),
            is_done: AtomicBool::new(false),
        }
    }
}

// ==============================================================================
// SECCIÓN 2: CACHÉ EN DISCO
// ==============================================================================
fn cache_path() -> Option<PathBuf> {
    dirs::data_local_dir().map(|p| p.join("RustSearch").join("index.bin"))
}

fn load_cache() -> Option<Vec<FileEntry>> {
    let path = cache_path()?;
    let data = std::fs::read(&path).ok()?;
    bincode::deserialize(&data).ok()
}

/// Guarda la caché (público para invocar tras F5).
pub fn save_cache(entries: &[FileEntry]) {
    if let Some(path) = cache_path() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(data) = bincode::serialize(entries) {
            let _ = std::fs::write(path, data);
        }
    }
}

// ==============================================================================
// SECCIÓN 3: INDEXADOR
// ==============================================================================
pub fn start_indexer(tx: Sender<Vec<FileEntry>>, status: Arc<IndexerStatus>) {
    std::thread::spawn(move || {
        // ---------- 3.1: CARGA DESDE CACHÉ (arranque instantáneo) ----------
        if let Some(mut entries) = load_cache() {
            entries.retain(|e| !e.is_deleted); // no reintroducir eliminados
            for chunk in entries.chunks(20_000) {
                let _ = tx.send(chunk.to_vec());
                status.count.fetch_add(chunk.len(), Ordering::Relaxed);
            }
            status.is_done.store(true, Ordering::Relaxed);
            return;
        }

        // ---------- 3.2: ESCANEO FRESCO CON JWALK ----------
        let mut chunk: Vec<FileEntry> = Vec::with_capacity(20_000);
        let mut all_entries: Vec<FileEntry> = Vec::new();

        for entry in jwalk::WalkDir::new("C:\\").skip_hidden(false).into_iter() {
            if let Ok(e) = entry {
                if let Ok(metadata) = e.metadata() {
                    let name = e.file_name().to_string_lossy().to_string();
                    if name.is_empty() {
                        continue;
                    }
                    let path = e.path();
                    let path_lower = path.to_string_lossy().to_lowercase();

                    chunk.push(FileEntry {
                        name_lower: name.to_lowercase(),
                        name,
                        path,
                        path_lower,
                        is_dir: metadata.is_dir(),
                        size: metadata.len(),
                        is_deleted: false,
                        trash_path: None,
                    });

                    if chunk.len() >= 20_000 {
                        let c = std::mem::take(&mut chunk);
                        status.count.fetch_add(c.len(), Ordering::Relaxed);
                        all_entries.extend(c.iter().cloned());
                        let _ = tx.send(c);
                        chunk.reserve(20_000);
                    }
                }
            }
        }

        if !chunk.is_empty() {
            status.count.fetch_add(chunk.len(), Ordering::Relaxed);
            all_entries.extend(chunk.iter().cloned());
            let _ = tx.send(chunk);
        }

        // ---------- 3.3: GUARDAR CACHÉ ----------
        std::thread::spawn(move || save_cache(&all_entries));
        status.is_done.store(true, Ordering::Relaxed);
    });
}