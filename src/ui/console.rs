// FARO QEM (C) R.Trunk 2026
// Live Console & Logs Viewer Component

use egui::{Color32, Frame, Margin, Rounding, Stroke, Ui};
use std::time::SystemTime;

use crate::i18n::I18n;
use crate::qemu::QemuEvent;
use crate::ui::theme::{
    icons, status_badge, COLOR_DARK_BORDER, COLOR_ERROR, COLOR_RUNNING, COLOR_STOPPED,
    ROUNDING_BUTTON,
};

/// Log level categorization for virtual machine console entries
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Stdout,
    Stderr,
    System,
    Error,
}

impl LogLevel {
    /// Short 3-letter label for compact console badges
    pub fn label(&self) -> &'static str {
        match self {
            LogLevel::Stdout => "OUT",
            LogLevel::Stderr => "ERR",
            LogLevel::System => "SYS",
            LogLevel::Error => "ERR",
        }
    }

    /// Color associated with this log level in console output
    pub fn color(&self) -> Color32 {
        match self {
            LogLevel::Stdout => Color32::from_rgb(220, 224, 230), // Neutral soft light-gray / white
            LogLevel::Stderr => Color32::from_rgb(245, 158, 11),  // Amber / warning orange
            LogLevel::System => Color32::from_rgb(56, 189, 248),  // Sky cyan / blue
            LogLevel::Error => COLOR_ERROR,                       // Crimson red
        }
    }

    /// Badge background fill color for the tag pill
    pub fn badge_bg(&self) -> Color32 {
        match self {
            LogLevel::Stdout => Color32::from_rgb(35, 40, 50),
            LogLevel::Stderr => Color32::from_rgb(70, 45, 10),
            LogLevel::System => Color32::from_rgb(15, 45, 65),
            LogLevel::Error => Color32::from_rgb(60, 20, 20),
        }
    }
}

/// Helper to remove ANSI escape sequences from terminal log output
pub fn strip_ansi_codes(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if let Some(&'[') = chars.peek() {
                chars.next();
                while let Some(&next_c) = chars.peek() {
                    chars.next();
                    if next_c.is_ascii_alphabetic() {
                        break;
                    }
                }
                continue;
            }
        }
        out.push(c);
    }
    out
}

/// Generate a clean HH:MM:SS.mmm timestamp string from the system clock
pub fn current_timestamp() -> String {
    let now = SystemTime::now();
    let duration = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let total_secs = duration.as_secs();
    let millis = duration.subsec_millis();
    let seconds = total_secs % 60;
    let minutes = (total_secs / 60) % 60;
    let hours = (total_secs / 3600) % 24;
    format!("{:02}:{:02}:{:02}.{:03}", hours, minutes, seconds, millis)
}

/// A single timestamped console log entry
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEntry {
    pub text: String,
    pub level: LogLevel,
    pub timestamp: String,
}

impl LogEntry {
    /// Create a new log entry with the current system timestamp
    pub fn new(level: LogLevel, text: impl Into<String>) -> Self {
        Self {
            text: strip_ansi_codes(&text.into()),
            level,
            timestamp: current_timestamp(),
        }
    }

    /// Create a new log entry with an explicit timestamp string
    pub fn with_timestamp(
        level: LogLevel,
        text: impl Into<String>,
        timestamp: impl Into<String>,
    ) -> Self {
        Self {
            text: strip_ansi_codes(&text.into()),
            level,
            timestamp: timestamp.into(),
        }
    }

    /// Formats the log entry into a standard log line: `[12:34:56.789] [OUT] text`
    pub fn formatted(&self) -> String {
        if self.timestamp.is_empty() {
            format!("[{}] {}", self.level.label(), self.text)
        } else {
            format!(
                "[{}] [{}] {}",
                self.timestamp,
                self.level.label(),
                self.text
            )
        }
    }
}

/// In-memory ring buffer holding real-time console log entries
#[derive(Debug, Clone)]
pub struct ConsoleLogBuffer {
    pub entries: Vec<LogEntry>,
    pub max_entries: usize,
    pub auto_scroll: bool,
    pub filter: String,
}

impl Default for ConsoleLogBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl ConsoleLogBuffer {
    /// Default buffer size limit in lines
    pub const DEFAULT_MAX_ENTRIES: usize = 5000;

    /// Creates an empty log buffer with default capacity (5000) and auto-scroll enabled
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            max_entries: Self::DEFAULT_MAX_ENTRIES,
            auto_scroll: true,
            filter: String::new(),
        }
    }

    /// Set a custom maximum capacity for entries in the buffer
    pub fn with_max_entries(mut self, max_entries: usize) -> Self {
        self.max_entries = max_entries;
        self.truncate_if_needed();
        self
    }

    /// Append a log message with the specified level, splitting multiple lines if present
    pub fn push(&mut self, level: LogLevel, msg: impl Into<String>) {
        let text = msg.into();
        let trimmed = text
            .strip_suffix("\r\n")
            .or_else(|| text.strip_suffix('\n'))
            .unwrap_or(&text);

        if trimmed.contains('\n') {
            for line in trimmed.lines() {
                self.entries.push(LogEntry::new(level, line));
            }
        } else {
            self.entries.push(LogEntry::new(level, trimmed));
        }
        self.truncate_if_needed();
    }

    /// Push an already constructed LogEntry directly
    pub fn push_entry(&mut self, entry: LogEntry) {
        self.entries.push(entry);
        self.truncate_if_needed();
    }

    /// Push standard output message
    pub fn push_stdout(&mut self, msg: impl Into<String>) {
        self.push(LogLevel::Stdout, msg);
    }

    /// Push standard error message
    pub fn push_stderr(&mut self, msg: impl Into<String>) {
        self.push(LogLevel::Stderr, msg);
    }

    /// Push internal system event message
    pub fn push_system(&mut self, msg: impl Into<String>) {
        self.push(LogLevel::System, msg);
    }

    /// Push fatal or supervisor error message
    pub fn push_error(&mut self, msg: impl Into<String>) {
        self.push(LogLevel::Error, msg);
    }

    /// Stream in a supervisor `QemuEvent`
    pub fn handle_event(&mut self, event: &QemuEvent) {
        match event {
            QemuEvent::Started(pid) => {
                self.push_system(format!("[PROCESS] QEMU started with PID {}", pid));
            }
            QemuEvent::Stdout(msg) => {
                self.push_stdout(msg);
            }
            QemuEvent::Stderr(msg) => {
                self.push_stderr(msg);
            }
            QemuEvent::Exited(Some(code)) => {
                if *code == 0 {
                    self.push_system(format!("[PROCESS] QEMU exited normally (code: {})", code));
                } else {
                    self.push_error(format!(
                        "[PROCESS] QEMU exited with non-zero exit code: {}",
                        code
                    ));
                }
            }
            QemuEvent::Exited(None) => {
                self.push_system("[PROCESS] QEMU process terminated by signal/killed");
            }
            QemuEvent::Error(err) => {
                self.push_error(format!("[SUPERVISOR ERROR] {}", err));
            }
        }
    }

    /// Clears all stored log entries
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Returns the number of log lines in the buffer
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns true if the buffer has no log lines
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns all log entries formatted as newline-delimited text
    pub fn all_text(&self) -> String {
        self.entries
            .iter()
            .map(|e| e.formatted())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Returns raw log lines without timestamp prefixes
    pub fn raw_text(&self) -> String {
        self.entries
            .iter()
            .map(|e| e.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn truncate_if_needed(&mut self) {
        if self.max_entries > 0 && self.entries.len() > self.max_entries {
            let excess = self.entries.len() - self.max_entries;
            self.entries.drain(0..excess);
        }
    }
}

/// Actions emitted by the console viewer
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleAction {
    Cleared,
    CopiedLogs,
    CopiedCommand,
}

/// Live Console & Logs Viewer UI component
pub struct ConsoleView<'a> {
    pub logs: &'a mut ConsoleLogBuffer,
    pub is_running: bool,
    pub pid: Option<u32>,
    pub exit_code: Option<Option<i32>>,
    pub cli_command: Option<&'a str>,
    pub i18n: &'a I18n,
}

impl<'a> ConsoleView<'a> {
    /// Create a new `ConsoleView` builder
    pub fn new(logs: &'a mut ConsoleLogBuffer, i18n: &'a I18n) -> Self {
        Self {
            logs,
            is_running: false,
            pid: None,
            exit_code: None,
            cli_command: None,
            i18n,
        }
    }

    /// Set whether the virtual machine is actively executing
    pub fn is_running(mut self, running: bool) -> Self {
        self.is_running = running;
        self
    }

    /// Set operating system process ID
    pub fn pid(mut self, pid: Option<u32>) -> Self {
        self.pid = pid;
        self
    }

    /// Set exit code state: `None` = not exited, `Some(Some(code))` = exited with code, `Some(None)` = exited by signal
    pub fn exit_code(mut self, exit_code: Option<Option<i32>>) -> Self {
        self.exit_code = exit_code;
        self
    }

    /// Convenience setter for exit code
    pub fn with_exit_code(mut self, exit_code: Option<i32>) -> Self {
        self.exit_code = Some(exit_code);
        self
    }

    /// Set QEMU command line string for preview
    pub fn cli_command(mut self, cli_command: Option<&'a str>) -> Self {
        self.cli_command = cli_command;
        self
    }

    /// Render the live console viewer inside egui UI
    pub fn show(&mut self, ui: &mut Ui) -> Option<ConsoleAction> {
        let mut action = None;

        // --- Status Bar & Top Controls ---
        ui.horizontal(|ui| {
            // Status Indicator Badge
            if self.is_running {
                let pid_str = self
                    .pid
                    .map(|p| format!(" (PID: {})", p))
                    .unwrap_or_default();
                status_badge(
                    ui,
                    &format!("● {}{}", self.i18n.t("status_running"), pid_str),
                    COLOR_RUNNING,
                );
            } else if let Some(code_opt) = self.exit_code {
                match code_opt {
                    Some(0) => status_badge(
                        ui,
                        &format!("● {} (code 0)", self.i18n.t("status_stopped")),
                        COLOR_STOPPED,
                    ),
                    Some(code) => {
                        status_badge(ui, &format!("⚠ Exited (code {})", code), COLOR_ERROR)
                    }
                    None => status_badge(
                        ui,
                        &format!("● {}", self.i18n.t("status_stopped")),
                        COLOR_STOPPED,
                    ),
                }
            } else {
                status_badge(
                    ui,
                    &format!("● {}", self.i18n.t("status_stopped")),
                    COLOR_STOPPED,
                );
            }

            // Auto-scroll checkbox
            ui.checkbox(&mut self.logs.auto_scroll, self.i18n.t("auto_scroll"));

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Clear Button
                let clear_text = format!("{} {}", icons::DELETE, self.i18n.t("clear"));
                if ui
                    .add(egui::Button::new(
                        egui::RichText::new(clear_text).size(12.0),
                    ))
                    .on_hover_text("Clear console buffer")
                    .clicked()
                {
                    self.logs.clear();
                    action = Some(ConsoleAction::Cleared);
                }

                // Copy All Logs Button
                let copy_logs_label = format!("{} {}", icons::COPY, self.i18n.t("copy_logs"));
                if ui
                    .add(egui::Button::new(
                        egui::RichText::new(copy_logs_label).size(12.0),
                    ))
                    .on_hover_text("Copy all console logs to clipboard")
                    .clicked()
                {
                    let full_log = self.logs.all_text();
                    ui.output_mut(|o| o.copied_text = full_log);
                    action = Some(ConsoleAction::CopiedLogs);
                }

                // Line count label
                let lines_suffix = self.i18n.t("lines_label");

                let count_str = if self.logs.filter.is_empty() {
                    format!("{} {}", self.logs.len(), lines_suffix)
                } else {
                    let matching = self
                        .logs
                        .entries
                        .iter()
                        .filter(|e| {
                            e.text
                                .to_lowercase()
                                .contains(&self.logs.filter.to_lowercase())
                        })
                        .count();
                    format!("{}/{} {}", matching, self.logs.len(), lines_suffix)
                };
                ui.label(egui::RichText::new(count_str).size(12.0).weak());

                // Filter search text edit
                ui.add(
                    egui::TextEdit::singleline(&mut self.logs.filter)
                        .hint_text(self.i18n.t("filter_placeholder"))
                        .desired_width(120.0),
                );
            });
        });

        // --- Collapsible CLI Command Preview ---
        if let Some(cmd) = self.cli_command {
            ui.add_space(4.0);
            let header_title = format!("{} QEMU Command Line Preview", icons::CONSOLE);
            egui::CollapsingHeader::new(egui::RichText::new(header_title).size(12.0).strong())
                .default_open(false)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let copy_btn_text = format!("{} Copy Command", icons::COPY);
                        if ui
                            .add(egui::Button::new(
                                egui::RichText::new(copy_btn_text).size(11.0),
                            ))
                            .clicked()
                        {
                            ui.output_mut(|o| o.copied_text = cmd.to_string());
                            action = Some(ConsoleAction::CopiedCommand);
                        }
                    });

                    ui.add_space(2.0);
                    let cmd_frame = Frame::none()
                        .fill(Color32::from_rgb(15, 17, 21))
                        .stroke(Stroke::new(1.0, COLOR_DARK_BORDER))
                        .rounding(Rounding::same(ROUNDING_BUTTON))
                        .inner_margin(Margin::same(8.0));

                    cmd_frame.show(ui, |ui| {
                        egui::ScrollArea::horizontal().show(ui, |ui| {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(cmd)
                                        .monospace()
                                        .size(11.0)
                                        .color(Color32::from_rgb(180, 220, 250)),
                                )
                                .selectable(true),
                            );
                        });
                    });
                });
        }

        // --- Terminal Console Frame ---
        ui.add_space(6.0);
        let terminal_bg = Color32::from_rgb(10, 11, 14);
        let terminal_frame = Frame::none()
            .fill(terminal_bg)
            .stroke(Stroke::new(1.0, COLOR_DARK_BORDER))
            .rounding(Rounding::same(ROUNDING_BUTTON))
            .inner_margin(Margin::same(10.0));

        terminal_frame.show(ui, |ui| {
            let available_height = ui.available_height().max(180.0);
            let mut scroll = egui::ScrollArea::both()
                .max_height(available_height)
                .auto_shrink([false, false]);

            if self.logs.auto_scroll {
                scroll = scroll.stick_to_bottom(true);
            }

            scroll.show(ui, |ui| {
                ui.set_width(ui.available_width());

                if self.logs.is_empty() {
                    ui.add_space(30.0);
                    ui.vertical_centered(|ui| {
                        ui.label(
                            egui::RichText::new("[FARO QEM] Console output buffer is empty.")
                                .monospace()
                                .size(12.0)
                                .color(Color32::from_rgb(120, 125, 135)),
                        );
                        ui.add_space(4.0);
                        if self.is_running {
                            ui.label(
                                egui::RichText::new(
                                    "VM is actively running. Awaiting stdout / stderr messages...",
                                )
                                .monospace()
                                .size(11.0)
                                .color(COLOR_RUNNING),
                            );
                        } else {
                            ui.label(
                                egui::RichText::new(
                                    "Virtual machine is stopped. Click 'Launch VM' to initialize execution.",
                                )
                                .monospace()
                                .size(11.0)
                                .color(Color32::from_rgb(90, 95, 105)),
                            );
                        }
                    });
                    ui.add_space(30.0);
                } else {
                    let filter_lower = self.logs.filter.to_lowercase();
                    let filter_active = !filter_lower.is_empty();

                    for entry in &self.logs.entries {
                        if filter_active && !entry.text.to_lowercase().contains(&filter_lower) {
                            continue;
                        }

                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;

                            // Timestamp
                            if !entry.timestamp.is_empty() {
                                ui.label(
                                    egui::RichText::new(&entry.timestamp)
                                        .monospace()
                                        .size(11.0)
                                        .color(Color32::from_rgb(110, 115, 128)),
                                );
                            }

                            // Level Tag Pill
                            let tag_frame = Frame::none()
                                .fill(entry.level.badge_bg())
                                .rounding(Rounding::same(ROUNDING_BUTTON))
                                .inner_margin(Margin::symmetric(4.0, 1.0));

                            tag_frame.show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new(entry.level.label())
                                        .monospace()
                                        .size(10.0)
                                        .strong()
                                        .color(entry.level.color()),
                                );
                            });

                            // Selectable Text Line
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(&entry.text)
                                        .monospace()
                                        .size(12.0)
                                        .color(entry.level.color()),
                                )
                                .selectable(true),
                            );
                        });
                    }
                }
            });
        });

        action
    }
}
