// src/main.rs
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod indexer;
mod ui;

use eframe::egui;
use std::sync::{mpsc, Arc};

fn main() -> eframe::Result<()> {
    let (tx, rx) = mpsc::channel();
    let status = Arc::new(indexer::IndexerStatus::new());

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 650.0])
            .with_min_inner_size([600.0, 400.0])
            .with_transparent(true)
            .with_icon(Arc::new(load_icon())),
        ..Default::default()
    };

    let status_for_app = Arc::clone(&status);

    eframe::run_native(
        "RustSearch",
        options,
        Box::new(move |cc| {
            let mut fonts = egui::FontDefinitions::default();

            if let Ok(data) = std::fs::read("C:\\Windows\\Fonts\\seguiemj.ttf") {
                fonts
                    .font_data
                    .insert("emoji".to_owned(), egui::FontData::from_owned(data).into());
                fonts
                    .families
                    .get_mut(&egui::FontFamily::Proportional)
                    .unwrap()
                    .insert(0, "emoji".to_owned());
            }
            if let Ok(data) = std::fs::read("C:\\Windows\\Fonts\\segoeui.ttf") {
                fonts
                    .font_data
                    .insert("segoe".to_owned(), egui::FontData::from_owned(data).into());
                fonts
                    .families
                    .get_mut(&egui::FontFamily::Proportional)
                    .unwrap()
                    .insert(0, "segoe".to_owned());
            }
            cc.egui_ctx.set_fonts(fonts);

            indexer::start_indexer(tx, status);
            Ok(Box::new(ui::RustSearchApp::new(cc, rx, status_for_app)))
        }),
    )
}

/// Carga `icon.ico` EMBEBIDO en el binario en tiempo de compilación
/// y lo convierte a `egui::IconData` para la ventana / barra de tareas.
fn load_icon() -> egui::IconData {
    let bytes = include_bytes!("../icon.ico");
    let img = image::load_from_memory_with_format(bytes, image::ImageFormat::Ico)
        .expect("icon.ico inválido")
        .to_rgba8();
    let (w, h) = img.dimensions();
    egui::IconData {
        rgba: img.into_raw(),
        width: w,
        height: h,
    }
}