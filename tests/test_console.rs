// FARO QEM (C) R.Trunk 2026
// Unit tests for Live Console & Logs Viewer Component

use egui::Context;
use faro_qem::config::{Arch, VmConfig};
use faro_qem::i18n::I18n;
use faro_qem::qemu::QemuEvent;
use faro_qem::ui::{
    current_timestamp, render_console, render_console_simple, strip_ansi_codes,
    ConsoleAction, ConsoleLogBuffer, ConsoleView, CreateDiskState, DetailTab,
    LogEntry, LogLevel, VmDetailView,
};

#[test]
fn test_console_action_variants() {
    assert_eq!(ConsoleAction::Cleared, ConsoleAction::Cleared);
    assert_ne!(ConsoleAction::Cleared, ConsoleAction::CopiedLogs);
    assert_ne!(ConsoleAction::CopiedLogs, ConsoleAction::CopiedCommand);
}

#[test]
fn test_log_level_properties() {
    assert_eq!(LogLevel::Stdout.label(), "OUT");
    assert_eq!(LogLevel::Stderr.label(), "ERR");
    assert_eq!(LogLevel::System.label(), "SYS");
    assert_eq!(LogLevel::Error.label(), "ERR");

    assert_ne!(LogLevel::Stdout.color(), LogLevel::Stderr.color());
    assert_ne!(LogLevel::System.color(), LogLevel::Error.color());
}

#[test]
fn test_strip_ansi_codes() {
    let plain = "Normal text without colors";
    assert_eq!(strip_ansi_codes(plain), plain);

    let colored = "\x1b[31mRed text\x1b[0m and \x1b[1;32mBold Green\x1b[0m";
    assert_eq!(strip_ansi_codes(colored), "Red text and Bold Green");

    let cursor = "Loading...\x1b[2K\rDone!";
    assert_eq!(strip_ansi_codes(cursor), "Loading...\rDone!");
}

#[test]
fn test_current_timestamp_format() {
    let ts = current_timestamp();
    // Expected format: HH:MM:SS.mmm (12 characters)
    assert_eq!(ts.len(), 12);
    assert_eq!(&ts[2..3], ":");
    assert_eq!(&ts[5..6], ":");
    assert_eq!(&ts[8..9], ".");
}

#[test]
fn test_log_entry_creation_and_formatting() {
    let entry = LogEntry::with_timestamp(LogLevel::Stdout, "QEMU initialized", "12:00:00.000");
    assert_eq!(entry.level, LogLevel::Stdout);
    assert_eq!(entry.text, "QEMU initialized");
    assert_eq!(entry.timestamp, "12:00:00.000");
    assert_eq!(entry.formatted(), "[12:00:00.000] [OUT] QEMU initialized");

    let entry_no_ts = LogEntry {
        text: "Direct message".to_string(),
        level: LogLevel::Error,
        timestamp: String::new(),
    };
    assert_eq!(entry_no_ts.formatted(), "[ERR] Direct message");
}

#[test]
fn test_console_log_buffer_push_and_clear() {
    let mut buf = ConsoleLogBuffer::new();
    assert!(buf.is_empty());
    assert_eq!(buf.len(), 0);
    assert!(buf.auto_scroll);
    assert_eq!(buf.max_entries, ConsoleLogBuffer::DEFAULT_MAX_ENTRIES);

    buf.push_stdout("Booting kernel vmlinuz");
    buf.push_stderr("ACPI Warning: Table not found");
    buf.push_system("Process PID 4512 attached");
    buf.push_error("Failed to map I/O space");

    assert_eq!(buf.len(), 4);
    assert!(!buf.is_empty());
    assert_eq!(buf.entries[0].level, LogLevel::Stdout);
    assert_eq!(buf.entries[1].level, LogLevel::Stderr);
    assert_eq!(buf.entries[2].level, LogLevel::System);
    assert_eq!(buf.entries[3].level, LogLevel::Error);

    let all = buf.all_text();
    assert!(all.contains("[OUT] Booting kernel vmlinuz"));
    assert!(all.contains("[ERR] ACPI Warning: Table not found"));
    assert!(all.contains("[SYS] Process PID 4512 attached"));
    assert!(all.contains("[ERR] Failed to map I/O space"));

    let raw = buf.raw_text();
    assert_eq!(
        raw,
        "Booting kernel vmlinuz\nACPI Warning: Table not found\nProcess PID 4512 attached\nFailed to map I/O space"
    );

    buf.clear();
    assert!(buf.is_empty());
    assert_eq!(buf.len(), 0);
    assert_eq!(buf.all_text(), "");
}

#[test]
fn test_console_log_buffer_multiline_split() {
    let mut buf = ConsoleLogBuffer::new();
    buf.push_stdout("Line 1\nLine 2\nLine 3\n");
    assert_eq!(buf.len(), 3);
    assert_eq!(buf.entries[0].text, "Line 1");
    assert_eq!(buf.entries[1].text, "Line 2");
    assert_eq!(buf.entries[2].text, "Line 3");

    buf.push_stderr("Error A\r\nError B\r\n");
    assert_eq!(buf.len(), 5);
    assert_eq!(buf.entries[3].text, "Error A");
    assert_eq!(buf.entries[4].text, "Error B");
}

#[test]
fn test_console_log_buffer_capacity_truncation() {
    let mut buf = ConsoleLogBuffer::new().with_max_entries(3);
    assert_eq!(buf.max_entries, 3);

    buf.push_stdout("Message 1");
    buf.push_stdout("Message 2");
    buf.push_stdout("Message 3");
    assert_eq!(buf.len(), 3);
    assert_eq!(buf.entries[0].text, "Message 1");

    buf.push_stdout("Message 4");
    assert_eq!(buf.len(), 3);
    assert_eq!(buf.entries[0].text, "Message 2");
    assert_eq!(buf.entries[1].text, "Message 3");
    assert_eq!(buf.entries[2].text, "Message 4");

    // Test multi-line overflow
    buf.push_stdout("Message 5\nMessage 6\nMessage 7");
    assert_eq!(buf.len(), 3);
    assert_eq!(buf.entries[0].text, "Message 5");
    assert_eq!(buf.entries[1].text, "Message 6");
    assert_eq!(buf.entries[2].text, "Message 7");
}

#[test]
fn test_console_log_buffer_qemu_event_handling() {
    let mut buf = ConsoleLogBuffer::new();

    buf.handle_event(&QemuEvent::Started(8888));
    assert_eq!(buf.len(), 1);
    assert_eq!(buf.entries[0].level, LogLevel::System);
    assert!(buf.entries[0].text.contains("8888"));

    buf.handle_event(&QemuEvent::Stdout("SeaBIOS rel-1.16".to_string()));
    assert_eq!(buf.len(), 2);
    assert_eq!(buf.entries[1].level, LogLevel::Stdout);

    buf.handle_event(&QemuEvent::Stderr("Could not open file".to_string()));
    assert_eq!(buf.len(), 3);
    assert_eq!(buf.entries[2].level, LogLevel::Stderr);

    buf.handle_event(&QemuEvent::Exited(Some(0)));
    assert_eq!(buf.len(), 4);
    assert_eq!(buf.entries[3].level, LogLevel::System);
    assert!(buf.entries[3].text.contains("normally"));

    buf.handle_event(&QemuEvent::Exited(Some(1)));
    assert_eq!(buf.len(), 5);
    assert_eq!(buf.entries[4].level, LogLevel::Error);
    assert!(buf.entries[4].text.contains("non-zero exit code: 1"));

    buf.handle_event(&QemuEvent::Exited(None));
    assert_eq!(buf.len(), 6);
    assert_eq!(buf.entries[5].level, LogLevel::System);

    buf.handle_event(&QemuEvent::Error("Thread crashed".to_string()));
    assert_eq!(buf.len(), 7);
    assert_eq!(buf.entries[6].level, LogLevel::Error);
}

#[test]
fn test_console_view_empty_rendering() {
    let ctx = Context::default();
    let i18n = I18n::new("en");
    let mut logs = ConsoleLogBuffer::new();

    // 1. VM stopped empty console
    let _ = ctx.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let mut view = ConsoleView::new(&mut logs, &i18n).is_running(false);
            let action = view.show(ui);
            assert_eq!(action, None);
        });
    });

    // 2. VM running with PID empty console
    let _ = ctx.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let mut view = ConsoleView::new(&mut logs, &i18n)
                .is_running(true)
                .pid(Some(9999));
            let action = view.show(ui);
            assert_eq!(action, None);
        });
    });
}

#[test]
fn test_console_view_with_logs_and_cli_preview() {
    let ctx = Context::default();
    let i18n = I18n::new("en");
    let mut logs = ConsoleLogBuffer::new();

    logs.push_system("[SYSTEM] QEMU started");
    logs.push_stdout("Linux version 6.6.0 (builder@kernel)");
    logs.push_stderr("warning: KVM not available, falling back to TCG");
    logs.push_error("kernel panic - not syncing: VFS: Unable to mount root fs");

    let cmd = "qemu-system-x86_64 -m 4096 -smp 4 -drive file=disk.qcow2,format=qcow2";

    let _ = ctx.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let action = render_console(
                ui,
                &mut logs,
                true,
                Some(12345),
                None,
                Some(cmd),
                &i18n,
            );
            assert_eq!(action, None);
        });
    });

    // Verify exit code status rendering (code 0)
    let _ = ctx.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let action = render_console(
                ui,
                &mut logs,
                false,
                None,
                Some(Some(0)),
                Some(cmd),
                &i18n,
            );
            assert_eq!(action, None);
        });
    });

    // Verify non-zero exit code rendering (code 139 segfault)
    let _ = ctx.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let action = render_console(
                ui,
                &mut logs,
                false,
                None,
                Some(Some(139)),
                Some(cmd),
                &i18n,
            );
            assert_eq!(action, None);
        });
    });
}

#[test]
fn test_console_view_search_filter() {
    let ctx = Context::default();
    let i18n = I18n::new("en");
    let mut logs = ConsoleLogBuffer::new();

    logs.push_stdout("Normal log line");
    logs.push_error("Special Error X");
    logs.push_stdout("Another normal line");

    logs.filter = "special".to_string();

    let _ = ctx.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let action = render_console_simple(ui, &mut logs, false, &i18n);
            assert_eq!(action, None);
        });
    });

    assert_eq!(logs.len(), 3);
}

#[test]
fn test_console_view_locales() {
    let ctx = Context::default();
    let mut logs = ConsoleLogBuffer::new();
    logs.push_stdout("Locale test line");

    for locale in ["en", "de", "fr"] {
        let i18n = I18n::new(locale);
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let action = render_console_simple(ui, &mut logs, false, &i18n);
                assert_eq!(action, None);
            });
        });
    }
}

#[test]
fn test_vm_detail_with_console_logs_integration() {
    let ctx = Context::default();
    let i18n = I18n::new("en");
    let mut vm = VmConfig::new_default("Debian 12 Console", Arch::X86_64);
    let mut active_tab = DetailTab::Console;
    let mut disk_modal = CreateDiskState::default();
    let mut logs = ConsoleLogBuffer::new();

    logs.push_system("[SYSTEM] Process running");
    logs.push_stdout("GRUB loading...");

    let cli_cmd = "qemu-system-x86_64 -name 'Debian 12 Console' -m 2048";

    let _ = ctx.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let mut view = VmDetailView::new(
                &mut vm,
                true,
                &i18n,
                &mut active_tab,
                None,
                &mut disk_modal,
            )
            .with_console(&mut logs)
            .with_pid(7712)
            .with_cli_command(cli_cmd);

            let action = view.show(ui);
            assert_eq!(action, None);
        });
    });

    assert_eq!(logs.len(), 2);
}
