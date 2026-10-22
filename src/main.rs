// FARO QEM (C) R.Trunk 2026
// Native Application Entry Point

use eframe::egui;
use faro_qem::app::FaroQemApp;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1120.0, 720.0])
            .with_min_inner_size([880.0, 600.0])
            .with_title("FARO QEM (C) R.Trunk 2026"),
        ..Default::default()
    };

    eframe::run_native(
        "FARO QEM (C) R.Trunk 2026",
        options,
        Box::new(|cc| Ok(Box::new(FaroQemApp::new(cc)))),
    )
}
