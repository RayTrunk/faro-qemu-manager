// FARO QEMU MANAGER (C) R.Trunk 2026
// Application behaviour when the stored configuration cannot be read

use egui::Context;
use faro_qem::i18n::I18n;
use faro_qem::ui::notice::{Notice, NoticeBar, NoticeLevel};

#[test]
fn test_notice_bar_renders_and_dismisses() {
    let i18n = I18n::new("en");
    let mut notices = vec![
        Notice::error("something broke"),
        Notice::warning("something is odd"),
        Notice::info("something happened"),
    ];
    assert_eq!(notices[0].level, NoticeLevel::Error);

    let ctx = Context::default();
    let _ = ctx.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            NoticeBar::new(&mut notices).show(ui, &i18n);
        });
    });
    assert_eq!(notices.len(), 3, "rendering alone dismisses nothing");

    notices.clear();
    let ctx2 = Context::default();
    let _ = ctx2.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            NoticeBar::new(&mut notices).show(ui, &i18n);
        });
    });
}
