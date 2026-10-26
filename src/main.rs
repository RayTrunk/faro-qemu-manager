// FARO QEMU MANAGER (C) R.Trunk 2026
// Native Application Entry Point
//
// This program is free software: you can redistribute it and/or modify it
// under the terms of the GNU General Public License, version 3, as
// published by the Free Software Foundation. It is distributed WITHOUT ANY
// WARRANTY, without even the implied warranty of MERCHANTABILITY or
// FITNESS FOR A PARTICULAR PURPOSE. See the LICENSE file in this
// repository, or <https://www.gnu.org/licenses/>.

// No console window behind the main window. Windows allocates one for any
// program whose PE subsystem says CONSOLE, which is what Rust emits unless
// asked otherwise -- that is why 0.9.0 opened a black window on every
// start.
//
// Only for release builds: a debug build keeps its console, because that is
// where `cargo run` prints and where a panic message can still be read.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use faro_qem::app::FaroQemApp;
use faro_qem::ui::theme::load_app_icon;

fn main() -> eframe::Result<()> {
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1120.0, 720.0])
        // Measured headlessly: below about 900 points the content of the
        // window no longer fits and is laid out past the right edge, where
        // egui does not paint it at all. The old floor of 880 was inside
        // that range.
        .with_min_inner_size([960.0, 640.0])
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
