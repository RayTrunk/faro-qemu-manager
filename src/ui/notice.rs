// FARO QEMU MANAGER (C) R.Trunk 2026
// Notices about problems the user needs to know about

use egui::{Color32, Margin, Rounding, Stroke};

use crate::i18n::I18n;
use crate::ui::theme::{icons, COLOR_ERROR, COLOR_PRIMARY, COLOR_WARNING, ROUNDING_BUTTON};

/// How much attention a notice deserves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeLevel {
    Info,
    Warning,
    Error,
}

impl NoticeLevel {
    /// Accent colour for the notice frame and its icon.
    pub fn color(&self) -> Color32 {
        match self {
            NoticeLevel::Info => COLOR_PRIMARY,
            NoticeLevel::Warning => COLOR_WARNING,
            NoticeLevel::Error => COLOR_ERROR,
        }
    }

    /// Leading glyph for the notice row.
    pub fn icon(&self) -> &'static str {
        match self {
            NoticeLevel::Info => icons::CHECK,
            NoticeLevel::Warning => icons::WARNING,
            NoticeLevel::Error => icons::ERROR,
        }
    }
}

/// A message for the user about something that happened outside their control.
///
/// `text` is already translated and formatted: the caller resolves the
/// catalogue key, because only the caller knows which values go into it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub level: NoticeLevel,
    pub text: String,
}

impl Notice {
    pub fn info(text: impl Into<String>) -> Self {
        Self {
            level: NoticeLevel::Info,
            text: text.into(),
        }
    }

    pub fn warning(text: impl Into<String>) -> Self {
        Self {
            level: NoticeLevel::Warning,
            text: text.into(),
        }
    }

    pub fn error(text: impl Into<String>) -> Self {
        Self {
            level: NoticeLevel::Error,
            text: text.into(),
        }
    }
}

/// Dismissible band of notices, drawn above the rest of the interface.
pub struct NoticeBar<'a> {
    pub notices: &'a mut Vec<Notice>,
}

impl<'a> NoticeBar<'a> {
    pub fn new(notices: &'a mut Vec<Notice>) -> Self {
        Self { notices }
    }

    /// Draw one row per notice, each with a dismiss button.
    ///
    /// Draws nothing at all when there is nothing to say, so an empty band
    /// takes no height. At most one notice is dismissed per frame, which is all
    /// a single click can ask for.
    pub fn show(self, ui: &mut egui::Ui, i18n: &I18n) {
        if self.notices.is_empty() {
            return;
        }

        let mut dismissed: Option<usize> = None;

        for (idx, notice) in self.notices.iter().enumerate() {
            let color = notice.level.color();
            egui::Frame::none()
                .fill(color.linear_multiply(0.12))
                .stroke(Stroke::new(1.0, color))
                .rounding(Rounding::same(ROUNDING_BUTTON))
                .inner_margin(Margin::symmetric(10.0, 6.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(notice.level.icon()).color(color));
                        ui.label(egui::RichText::new(&notice.text).size(12.0));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button(i18n.t("notice_dismiss")).clicked() {
                                dismissed = Some(idx);
                            }
                        });
                    });
                });
            ui.add_space(4.0);
        }

        if let Some(idx) = dismissed {
            self.notices.remove(idx);
        }
    }
}
