// FARO QEMU MANAGER (C) R.Trunk 2026
// Unit tests for Live Console & Logs Viewer Component

mod common;

use egui::Context;
use faro_qem::config::{Arch, VmConfig};
use faro_qem::i18n::I18n;
use faro_qem::qemu::QemuEvent;
use faro_qem::ui::{
    current_timestamp, strip_ansi_codes, ConsoleAction, ConsoleLogBuffer, ConsoleView,
    CreateDiskState, DetailTab, LogEntry, LogLevel, VmDetailView,
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

    // Built through the constructor: the lower-cased copy the filter uses
    // is the type's own business, not the caller's.
    let entry_no_ts = LogEntry::with_timestamp(LogLevel::Error, "Direct message", "");
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
            let action = ConsoleView::new(&mut logs, &i18n)
                .is_running(true)
                .pid(Some(12345))
                .cli_command(Some(cmd))
                .show(ui);
            assert_eq!(action, None);
        });
    });

    // Verify exit code status rendering (code 0)
    let _ = ctx.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let action = ConsoleView::new(&mut logs, &i18n)
                .is_running(false)
                .exit_code(Some(Some(0)))
                .cli_command(Some(cmd))
                .show(ui);
            assert_eq!(action, None);
        });
    });

    // Verify non-zero exit code rendering (code 139 segfault)
    let _ = ctx.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let action = ConsoleView::new(&mut logs, &i18n)
                .is_running(false)
                .exit_code(Some(Some(139)))
                .cli_command(Some(cmd))
                .show(ui);
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
            let action = ConsoleView::new(&mut logs, &i18n)
                .is_running(false)
                .show(ui);
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
                let action = ConsoleView::new(&mut logs, &i18n)
                    .is_running(false)
                    .show(ui);
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
            let mut view =
                VmDetailView::new(&mut vm, true, &i18n, &mut active_tab, None, &mut disk_modal)
                    .with_console(&mut logs)
                    .with_pid(7712)
                    .with_cli_command(cli_cmd);

            let action = view.show(ui);
            assert_eq!(action, None);
        });
    });

    assert_eq!(logs.len(), 2);
}

// ---------------------------------------------------------------------------
// Part 4, task 7: a long console does not cost a long frame
// ---------------------------------------------------------------------------

#[test]
fn test_the_console_lays_out_only_what_is_visible() {
    let mut buf = ConsoleLogBuffer::new();
    for n in 0..5000 {
        buf.push_stdout(format!("line {}", n));
    }
    let shown = buf.visible_slice(4900, 50);
    assert_eq!(
        shown.len(),
        50,
        "a 5000-line buffer must not lay out 5000 lines per frame"
    );
    assert!(shown[0].text.contains("4900"));

    // Asking past the end is what a shrinking buffer does to a scroll
    // position, and must not panic.
    assert!(buf.visible_slice(4990, 50).len() <= 10);
    assert!(buf.visible_slice(9000, 50).is_empty());
}

#[test]
fn test_the_filter_is_not_lowercased_per_line_per_frame() {
    let mut buf = ConsoleLogBuffer::new();
    for n in 0..2000 {
        buf.push_stdout(format!("Line {} WARNING", n));
    }
    let started = std::time::Instant::now();
    for _ in 0..20 {
        assert_eq!(buf.matching("warning").len(), 2000);
    }
    assert!(
        started.elapsed() < std::time::Duration::from_millis(400),
        "took {:?}",
        started.elapsed()
    );
}

#[test]
fn test_the_filter_matches_regardless_of_case_and_misses_what_it_should() {
    let mut buf = ConsoleLogBuffer::new();
    buf.push_stdout("ACPI table WARNING");
    buf.push_stdout("all quiet");
    assert_eq!(buf.matching("warning").len(), 1);
    assert_eq!(buf.matching("WARNING").len(), 1);
    assert_eq!(buf.matching("acpi").len(), 1);
    assert!(buf.matching("kernel panic").is_empty());
    assert_eq!(buf.matching("").len(), 2, "an empty filter hides nothing");
}

#[test]
fn test_the_console_really_draws_its_lines() {
    // show_rows draws only what is on screen, which is the point -- but a
    // view that draws nothing at all would pass every other test here.
    let ctx = Context::default();
    let i18n = I18n::new("en");
    let mut logs = ConsoleLogBuffer::new();
    logs.push_stdout("SeaBIOS rel-1.16.3");
    logs.push_stderr("ACPI table warning");

    let mut draw = || {
        ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ConsoleView::new(&mut logs, &i18n).is_running(true).show(ui);
            });
        })
    };
    let _ = draw();
    let drawn = common::rendered_text(&draw());
    assert!(
        drawn.contains("SeaBIOS rel-1.16.3"),
        "the console drew no lines:
{}",
        drawn
    );
    assert!(
        drawn.contains("ACPI table warning"),
        "drew:
{}",
        drawn
    );
}

#[test]
fn test_a_filtered_console_draws_only_the_matching_lines() {
    let ctx = Context::default();
    let i18n = I18n::new("en");
    let mut logs = ConsoleLogBuffer::new();
    logs.push_stdout("SeaBIOS rel-1.16.3");
    logs.push_stderr("ACPI table warning");
    logs.filter = "acpi".to_string();

    let mut draw = || {
        ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ConsoleView::new(&mut logs, &i18n).is_running(true).show(ui);
            });
        })
    };
    let _ = draw();
    let drawn = common::rendered_text(&draw());
    assert!(
        drawn.contains("ACPI table warning"),
        "drew:
{}",
        drawn
    );
    assert!(
        !drawn.contains("SeaBIOS"),
        "the filter must hide what does not match, drew:
{}",
        drawn
    );
}
