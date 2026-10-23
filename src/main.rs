// FARO QEMU MANAGER (C) R.Trunk 2026
// Native Application Entry Point

use eframe::egui;
use faro_qem::app::FaroQemApp;
use faro_qem::ui::theme::load_app_icon;

fn main() -> eframe::Result<()> {
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1120.0, 720.0])
        .with_min_inner_size([880.0, 600.0])
        .with_title("FARO QEMU MANAGER (C) R.Trunk 2026");

    if let Some(icon) = load_app_icon() {
        viewport = viewport.with_icon(icon);
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "FARO QEMU MANAGER (C) R.Trunk 2026",
        options,
        Box::new(|cc| Ok(Box::new(FaroQemApp::new(cc)))),
    )
}
