// FARO QEMU MANAGER (C) R.Trunk 2026
// Unit tests for VM Detail Tabs and Hardware Controls component

use egui::Context;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use uuid::Uuid;

use faro_qem::config::{
    Accelerator, Arch, DriveConfig, DriveFormat, DriveInterface, Firmware, PortForwardRule,
    VmConfig,
};
use faro_qem::i18n::I18n;
use faro_qem::qemu::QemuCapabilities;
use faro_qem::ui::{
    offered_accelerators, offered_machine_types, offered_netdev_backends, CreateDiskState,
    DetailTab, VmDetailAction, VmDetailView, VmStatus, SHUTDOWN_GRACE,
};

mod common;

#[test]
fn test_vm_detail_action_variants_and_helpers() {
    let id = Uuid::new_v4();

    let launch = VmDetailAction::Launch(id);
    let shutdown = VmDetailAction::Shutdown(id);
    let force = VmDetailAction::ForceStop(id);
    let save = VmDetailAction::Save(id);

    assert_eq!(launch.vm_id(), id);
    assert_eq!(shutdown.vm_id(), id);
    assert_eq!(force.vm_id(), id);
    assert_eq!(save.vm_id(), id);

    assert_eq!(launch, VmDetailAction::Launch(id));
    assert_ne!(launch, shutdown);
    assert_ne!(shutdown, save);
    // The two stop paths are different things, and nothing may treat one as
    // the other: spec section 7.1.
    assert_ne!(shutdown, force);
}

#[test]
fn test_detail_tab_defaults_and_all() {
    let default_tab = DetailTab::default();
    assert_eq!(default_tab, DetailTab::System);

    let all_tabs = DetailTab::all();
    assert_eq!(all_tabs.len(), 5);
    assert!(all_tabs.contains(&DetailTab::System));
    assert!(all_tabs.contains(&DetailTab::Storage));
    assert!(all_tabs.contains(&DetailTab::Display));
    assert!(all_tabs.contains(&DetailTab::Network));
    assert!(all_tabs.contains(&DetailTab::Console));
}

#[test]
fn test_create_disk_state_lifecycle() {
    let mut state = CreateDiskState::default();
    assert!(!state.open);
    assert_eq!(state.format, DriveFormat::Qcow2);
    assert_eq!(state.size_gb, 20);
    assert!(state.path_buf.is_empty());
    assert_eq!(state.status_msg, None);

    state.open_dialog(None, "ubuntu_disk.qcow2");
    assert!(state.open);
    assert_eq!(state.path_buf, "ubuntu_disk.qcow2");
    assert_eq!(state.status_msg, None);

    state.status_msg = Some(Err("Disk path invalid".to_string()));
    assert!(state.status_msg.is_some());

    state.close();
    assert!(!state.open);

    // Reopening clears previous status message
    state.open_dialog(None, "");
    assert!(state.open);
    assert_eq!(state.status_msg, None);
}

#[test]
fn test_a_second_disk_is_proposed_inside_the_machines_own_folder() {
    // Spec 5.7: a bare filename lands wherever the app was launched from,
    // which is how a stray debian_12.qcow2 ends up in the project directory.
    let base = PathBuf::from(r"C:\Users\someone\FARO-VMs\Debian");
    let mut state = CreateDiskState::default();

    state.open_dialog(Some(&base), "second.qcow2");

    let proposed = PathBuf::from(&state.path_buf);
    assert!(
        proposed.is_absolute(),
        "a bare name is written next to the executable, got: {}",
        state.path_buf
    );
    assert!(
        proposed.starts_with(&base),
        "the machine's own folder is where its disks belong, got: {}",
        state.path_buf
    );
}

#[test]
fn test_vm_detail_system_tab_rendering() {
    let ctx = Context::default();
    let i18n = I18n::new("en");
    let mut vm = VmConfig::new_default("Debian Workstation", Arch::X86_64);
    vm.cpu_cores = 4;
    vm.memory_mb = 4096;
    vm.accel = Accelerator::WHPX;
    vm.firmware = Firmware::Bios {
        rom: Some(PathBuf::from(r"C:\Firmware\OVMF.fd")),
    };

    let mut active_tab = DetailTab::System;
    let mut disk_modal = CreateDiskState::default();

    let _ = ctx.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let mut view = VmDetailView::new(
                &mut vm,
                false,
                &i18n,
                &mut active_tab,
                None,
                &mut disk_modal,
            );
            let action = view.show(ui);
            assert_eq!(action, None);
        });
    });

    assert_eq!(vm.name, "Debian Workstation");
    assert_eq!(vm.cpu_cores, 4);
    assert_eq!(vm.memory_mb, 4096);
    assert_eq!(vm.accel, Accelerator::WHPX);
}

#[test]
fn test_vm_detail_storage_tab_with_drives_and_cdroms() {
    let ctx = Context::default();
    let i18n = I18n::new("en");
    let mut vm = VmConfig::new_default("FreeBSD Server", Arch::X86_64);

    vm.drives.push(DriveConfig {
        path: PathBuf::from(r"C:\VMs\rootfs.qcow2"),
        format: DriveFormat::Qcow2,
        interface: DriveInterface::VirtIO,
        boot_index: Some(0),
    });

    vm.drives.push(DriveConfig {
        path: PathBuf::from(r"C:\VMs\data.raw"),
        format: DriveFormat::Raw,
        interface: DriveInterface::Nvme,
        boot_index: None,
    });

    vm.cdroms.push(PathBuf::from(r"C:\ISOs\FreeBSD-14.iso"));

    let mut active_tab = DetailTab::Storage;
    let mut disk_modal = CreateDiskState::default();

    let _ = ctx.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let mut view = VmDetailView::new(
                &mut vm,
                false,
                &i18n,
                &mut active_tab,
                None,
                &mut disk_modal,
            );
            let action = view.show(ui);
            assert_eq!(action, None);
        });
    });

    assert_eq!(vm.drives.len(), 2);
    assert_eq!(vm.cdroms.len(), 1);
    assert_eq!(vm.drives[0].format, DriveFormat::Qcow2);
    assert_eq!(vm.drives[1].interface, DriveInterface::Nvme);
}

#[test]
fn test_vm_detail_display_tab_rendering() {
    let ctx = Context::default();
    let i18n = I18n::new("en");
    let mut vm = VmConfig::new_default("Windows Guest", Arch::X86_64);
    vm.display.vga_type = "std".to_string();
    vm.display.display_backend = "sdl".to_string();
    vm.display.fullscreen = true;

    let mut active_tab = DetailTab::Display;
    let mut disk_modal = CreateDiskState::default();

    let _ = ctx.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let mut view = VmDetailView::new(
                &mut vm,
                false,
                &i18n,
                &mut active_tab,
                None,
                &mut disk_modal,
            );
            let action = view.show(ui);
            assert_eq!(action, None);
        });
    });

    assert_eq!(vm.display.vga_type, "std");
    assert_eq!(vm.display.display_backend, "sdl");
    assert!(vm.display.fullscreen);
}

#[test]
fn test_vm_detail_network_tab_with_port_forwards() {
    let ctx = Context::default();
    let i18n = I18n::new("en");
    let mut vm = VmConfig::new_default("Ubuntu Cloud", Arch::Aarch64);
    vm.network.backend = "user".to_string();
    vm.network.nic_model = "virtio-net-pci".to_string();
    vm.network.port_forwards.push(PortForwardRule {
        host_port: 2222,
        guest_port: 22,
        proto: "tcp".to_string(),
    });
    vm.network.port_forwards.push(PortForwardRule {
        host_port: 8080,
        guest_port: 80,
        proto: "tcp".to_string(),
    });
    vm.extra_args = "-usb -device usb-tablet".to_string();

    let mut active_tab = DetailTab::Network;
    let mut disk_modal = CreateDiskState::default();

    let _ = ctx.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let mut view = VmDetailView::new(
                &mut vm,
                false,
                &i18n,
                &mut active_tab,
                None,
                &mut disk_modal,
            );
            let action = view.show(ui);
            assert_eq!(action, None);
        });
    });

    assert_eq!(vm.network.port_forwards.len(), 2);
    assert_eq!(vm.network.port_forwards[0].host_port, 2222);
    assert_eq!(vm.network.port_forwards[1].guest_port, 80);
    assert_eq!(vm.extra_args, "-usb -device usb-tablet");
}

#[test]
fn test_vm_detail_console_tab_rendering() {
    let ctx = Context::default();
    let i18n = I18n::new("en");
    let mut vm = VmConfig::new_default("Console VM", Arch::X86_64);

    let mut active_tab = DetailTab::Console;
    let mut disk_modal = CreateDiskState::default();

    // 1. VM stopped console rendering
    let _ = ctx.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let mut view = VmDetailView::new(
                &mut vm,
                false,
                &i18n,
                &mut active_tab,
                None,
                &mut disk_modal,
            );
            let action = view.show(ui);
            assert_eq!(action, None);
        });
    });

    // 2. VM running console rendering
    let _ = ctx.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let mut view =
                VmDetailView::new(&mut vm, true, &i18n, &mut active_tab, None, &mut disk_modal);
            let action = view.show(ui);
            assert_eq!(action, None);
        });
    });
}

#[test]
fn test_create_disk_modal_validation_and_rendering() {
    let ctx = Context::default();
    let i18n = I18n::new("en");
    let mut vm = VmConfig::new_default("Disk Test VM", Arch::X86_64);

    let mut active_tab = DetailTab::Storage;
    let mut disk_modal = CreateDiskState {
        open: true,
        path_buf: "".to_string(),
        format: DriveFormat::Qcow2,
        size_gb: 40,
        status_msg: Some(Err("Path is required".to_string())),
        busy: false,
    };

    let _ = ctx.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let mut view = VmDetailView::new(
                &mut vm,
                false,
                &i18n,
                &mut active_tab,
                None,
                &mut disk_modal,
            );
            let _ = view.show(ui);
        });
    });

    assert!(disk_modal.open);
    assert_eq!(disk_modal.size_gb, 40);
    assert!(disk_modal.status_msg.is_some());

    // Test with success status message
    disk_modal.status_msg = Some(Ok("Disk created successfully!".to_string()));
    let _ = ctx.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let mut view = VmDetailView::new(
                &mut vm,
                false,
                &i18n,
                &mut active_tab,
                None,
                &mut disk_modal,
            );
            let _ = view.show(ui);
        });
    });
    assert!(disk_modal.open);
}

#[test]
fn test_vm_detail_direct_rendering() {
    let ctx = Context::default();
    let i18n = I18n::new("de");
    let mut vm = VmConfig::new_default("Direct View Test VM", Arch::Riscv64);
    let mut active_tab = DetailTab::System;
    let mut disk_modal = CreateDiskState::default();

    let _ = ctx.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let action = VmDetailView::new(
                &mut vm,
                false,
                &i18n,
                &mut active_tab,
                None,
                &mut disk_modal,
            )
            .show(ui);
            assert_eq!(action, None);
        });
    });

    assert_eq!(vm.arch, Arch::Riscv64);
    assert_eq!(active_tab, DetailTab::System);
}

#[test]
fn test_vm_detail_i18n_locales() {
    let ctx = Context::default();
    let mut vm = VmConfig::new_default("i18n Test VM", Arch::Arm);
    let mut active_tab = DetailTab::System;
    let mut disk_modal = CreateDiskState::default();

    for lang in &["en", "de", "fr"] {
        let i18n = I18n::new(lang);
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let mut view = VmDetailView::new(
                    &mut vm,
                    false,
                    &i18n,
                    &mut active_tab,
                    None,
                    &mut disk_modal,
                );
                let _ = view.show(ui);
            });
        });
    }
}

// --------------------------------------------------------------------------
// Part 2: only offer what the installed QEMU actually has
// --------------------------------------------------------------------------
//
// These assert on the offered lists directly, not on what a frame drew. A
// closed ComboBox paints only its selected value, so "bridge is not on screen"
// would hold even if bridge were on offer — a test without teeth.

#[test]
fn test_bridge_is_dropped_when_qemu_lacks_it() {
    let without = QemuCapabilities {
        netdev_backends: vec!["user".into(), "tap".into()],
        ..Default::default()
    };
    let offered = offered_netdev_backends(Some(&without));
    assert!(!offered.contains(&"bridge"), "offered: {:?}", offered);
    assert!(offered.contains(&"user"));
    assert!(
        offered.contains(&"none"),
        "none is ours, not QEMU's, and always available"
    );
}

#[test]
fn test_bridge_is_offered_when_qemu_has_it() {
    let with = QemuCapabilities {
        netdev_backends: vec!["user".into(), "tap".into(), "bridge".into()],
        ..Default::default()
    };
    assert!(offered_netdev_backends(Some(&with)).contains(&"bridge"));
}

#[test]
fn test_without_a_probe_capability_dependent_backends_stay_on_offer() {
    let offered = offered_netdev_backends(None);
    for expected in ["user", "bridge", "none"] {
        assert!(
            offered.contains(&expected),
            "not knowing must not look like unavailable: {:?}",
            offered
        );
    }
}

#[test]
fn test_tap_is_never_offered_because_it_cannot_work_yet() {
    let with_tap = QemuCapabilities {
        netdev_backends: vec!["user".into(), "tap".into()],
        ..Default::default()
    };
    for caps in [None, Some(&with_tap)] {
        assert!(
            !offered_netdev_backends(caps).contains(&"tap"),
            "tap answers \"no interface name\"; the configuration has no field for one"
        );
    }
}

#[test]
fn test_microvm_is_not_suggested_because_it_has_no_pci_bus() {
    let caps = QemuCapabilities {
        machines: vec!["q35".into(), "microvm".into()],
        ..Default::default()
    };
    assert!(
        !offered_machine_types(Arch::X86_64, Some(&caps)).contains(&"microvm"),
        "the default virtio-vga, virtio-net-pci and usb-tablet all need PCI"
    );
}

#[test]
fn test_no_machine_type_is_suggested_that_qemu_rejects() {
    let caps = QemuCapabilities {
        machines: vec!["q35".into(), "pc-i440fx-9.2".into(), "microvm".into()],
        ..Default::default()
    };
    let offered = offered_machine_types(Arch::X86_64, Some(&caps));
    assert!(offered.contains(&"q35"));
    assert!(
        !offered.contains(&"pc-i440fx"),
        "the bare i440fx name is rejected by QEMU: {:?}",
        offered
    );
}

#[test]
fn test_a_machine_this_qemu_lacks_is_not_suggested() {
    let caps = QemuCapabilities {
        machines: vec!["q35".into()],
        ..Default::default()
    };
    let offered = offered_machine_types(Arch::X86_64, Some(&caps));
    assert_eq!(offered, vec!["q35"]);
}

#[test]
fn test_haxm_is_not_offered_on_windows() {
    let offered = offered_accelerators();
    assert!(offered.contains(&Accelerator::TCG));

    #[cfg(target_os = "windows")]
    assert!(
        !offered.contains(&Accelerator::HAXM),
        "HAXM used to be bent silently onto WHPX: {:?}",
        offered
    );
    #[cfg(not(target_os = "windows"))]
    assert!(offered.contains(&Accelerator::HAXM));
}

#[test]
fn test_system_tab_renders_the_firmware_selector() {
    let i18n = I18n::new("en");
    let caps = QemuCapabilities::default();
    let mut vm = VmConfig::new_default("probe", Arch::X86_64);
    let mut active = DetailTab::System;
    let mut modal = CreateDiskState::default();
    let ctx = Context::default();

    let _ = ctx.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            VmDetailView::new(&mut vm, false, &i18n, &mut active, None, &mut modal)
                .with_capabilities(&caps)
                .show(ui);
        });
    });
    let output = ctx.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            VmDetailView::new(&mut vm, false, &i18n, &mut active, None, &mut modal)
                .with_capabilities(&caps)
                .show(ui);
        });
    });

    let drawn = common::rendered_text(&output);
    assert!(
        drawn.contains(i18n.t("firmware_bios")) && drawn.contains(i18n.t("firmware_uefi")),
        "both firmware kinds must be choosable, drew:
{}",
        drawn
    );
}

// Important 7
#[test]
fn test_the_missing_firmware_hint_follows_what_the_app_resolved() {
    let i18n = I18n::new("en");
    let caps = QemuCapabilities::default();
    let mut vm = VmConfig::new_default("Windows 11", Arch::X86_64);
    vm.firmware = Firmware::Uefi {
        nvram: std::path::PathBuf::from("nvram.fd"),
    };
    let mut active = DetailTab::System;
    let mut modal = CreateDiskState::default();

    // The panel must not go looking for the firmware itself: that is a
    // handful of is_file calls on every frame it draws. It is told once.
    for (available, expected) in [(false, true), (true, false)] {
        let ctx = Context::default();
        let mut draw = || {
            ctx.run(Default::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    VmDetailView::new(&mut vm, false, &i18n, &mut active, None, &mut modal)
                        .with_capabilities(&caps)
                        .with_firmware(available)
                        .show(ui);
                });
            })
        };
        let _ = draw();
        let output = draw();
        let drawn = common::rendered_text(&output);
        assert_eq!(
            drawn.contains(i18n.t("firmware_missing")),
            expected,
            "firmware available = {}, drew:
{}",
            available,
            drawn
        );
    }
}

// Important 9
#[test]
fn test_port_forwards_say_so_when_the_backend_cannot_apply_them() {
    let i18n = I18n::new("en");
    let caps = QemuCapabilities::default();
    let mut vm = VmConfig::new_default("probe", Arch::X86_64);
    vm.network.backend = "none".to_string();
    vm.network.port_forwards.push(PortForwardRule {
        host_port: 2222,
        guest_port: 22,
        proto: "tcp".to_string(),
    });
    let mut active = DetailTab::Network;
    let mut modal = CreateDiskState::default();
    let ctx = Context::default();
    let mut draw = || {
        ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                VmDetailView::new(&mut vm, false, &i18n, &mut active, None, &mut modal)
                    .with_capabilities(&caps)
                    .show(ui);
            });
        })
    };
    let _ = draw();
    let drawn = common::rendered_text(&draw());
    assert!(
        drawn.contains(i18n.t("forwards_backend_only")),
        "rules that cannot apply must say so, drew:\n{}",
        drawn
    );
}

#[test]
fn test_unprobed_options_are_marked_as_unverified() {
    let i18n = I18n::new("en");
    let mut vm = VmConfig::new_default("probe", Arch::X86_64);
    let mut active = DetailTab::System;
    let mut modal = CreateDiskState::default();

    // An unprobed capability set permits everything, so the lists are not
    // filtered. The user is told that rather than left to assume they were.
    let unprobed = QemuCapabilities::default();
    let ctx = Context::default();
    let mut draw = || {
        ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                VmDetailView::new(&mut vm, false, &i18n, &mut active, None, &mut modal)
                    .with_capabilities(&unprobed)
                    .show(ui);
            });
        })
    };
    let _ = draw();
    let drawn = common::rendered_text(&draw());
    assert!(
        drawn.contains(i18n.t("option_unverified")),
        "drew:\n{}",
        drawn
    );
}

#[test]
fn test_a_stored_accelerator_that_is_gone_is_marked_not_silently_replaced() {
    let i18n = I18n::new("en");
    let caps = QemuCapabilities::default();
    let mut vm = VmConfig::new_default("probe", Arch::X86_64);
    // HAXM is absent from modern Windows builds and is bent to WHPX at
    // start. The stored value stays, so it has to be labelled.
    vm.accel = Accelerator::HAXM;
    let mut active = DetailTab::System;
    let mut modal = CreateDiskState::default();
    let ctx = Context::default();
    let mut draw = || {
        ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                VmDetailView::new(&mut vm, false, &i18n, &mut active, None, &mut modal)
                    .with_capabilities(&caps)
                    .show(ui);
            });
        })
    };
    let _ = draw();
    let drawn = common::rendered_text(&draw());
    assert!(
        drawn.contains(i18n.t("option_substituted")),
        "drew:\n{}",
        drawn
    );
}

// Part 3a, task 6
#[test]
fn test_the_panel_shows_what_would_stop_the_start() {
    let i18n = I18n::new("en");
    let caps = QemuCapabilities::default();
    let mut vm = VmConfig::new_default("probe", Arch::X86_64);
    let mut active = DetailTab::System;
    let mut modal = CreateDiskState::default();
    let findings = vec![
        faro_qem::preflight::Finding {
            severity: faro_qem::preflight::Severity::Blocking,
            text: "NO DISK AT C:/nope/disk.qcow2".to_string(),
        },
        faro_qem::preflight::Finding {
            severity: faro_qem::preflight::Severity::Warning,
            text: "MORE MEMORY THAN THE HOST HAS".to_string(),
        },
    ];

    let ctx = Context::default();
    let mut draw = || {
        ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                VmDetailView::new(&mut vm, false, &i18n, &mut active, None, &mut modal)
                    .with_capabilities(&caps)
                    .with_findings(&findings)
                    .show(ui);
            });
        })
    };
    let _ = draw();
    let drawn = common::rendered_text(&draw());
    for finding in &findings {
        assert!(
            drawn.contains(&finding.text),
            "missing {:?}, drew:\n{}",
            finding.text,
            drawn
        );
    }
}

#[test]
fn test_a_panel_without_findings_draws_none_of_them() {
    let i18n = I18n::new("en");
    let caps = QemuCapabilities::default();
    let mut vm = VmConfig::new_default("probe", Arch::X86_64);
    let mut active = DetailTab::System;
    let mut modal = CreateDiskState::default();
    let ctx = Context::default();
    let mut draw = || {
        ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                VmDetailView::new(&mut vm, false, &i18n, &mut active, None, &mut modal)
                    .with_capabilities(&caps)
                    .show(ui);
            });
        })
    };
    let _ = draw();
    let drawn = common::rendered_text(&draw());
    assert!(!drawn.contains("NO DISK AT"));
}

// --------------------------------------------------------------------------
// Part 4, task 2: shutdown first, the hard way as a second, named step
// --------------------------------------------------------------------------

/// Everything the panel draws for a machine in this state.
///
/// `qmp_available` says whether a graceful path exists at all: without one
/// the panel has to say so rather than offer a button that would kill.
fn render_detail_with_status(status: VmStatus, qmp_available: bool, i18n: &I18n) -> String {
    let mut vm = VmConfig::new_default("probe", Arch::X86_64);
    let mut active = DetailTab::System;
    let mut modal = CreateDiskState::default();
    let since = match &status {
        VmStatus::ShuttingDown { since, .. } => Some(*since),
        _ => None,
    };
    let ctx = Context::default();
    let mut draw = || {
        ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                VmDetailView::new(
                    &mut vm,
                    status.is_running(),
                    i18n,
                    &mut active,
                    None,
                    &mut modal,
                )
                .with_shutdown(qmp_available, since)
                .show(ui);
            });
        })
    };
    let _ = draw();
    common::rendered_text(&draw())
}

#[test]
fn test_the_panel_offers_shutdown_first_and_force_only_while_waiting() {
    let i18n = I18n::new("en");
    let drawn = render_detail_with_status(VmStatus::Running { pid: 1 }, true, &i18n);
    assert!(drawn.contains(i18n.t("shutdown")), "drew:\n{}", drawn);
    assert!(
        !drawn.contains(i18n.t("force_stop")),
        "the hard way is a second step, drew:\n{}",
        drawn
    );

    let since = SystemTime::now() - SHUTDOWN_GRACE - Duration::from_secs(1);
    let drawn = render_detail_with_status(VmStatus::ShuttingDown { pid: 1, since }, true, &i18n);
    assert!(drawn.contains(i18n.t("shutting_down")), "drew:\n{}", drawn);
    assert!(drawn.contains(i18n.t("force_stop")), "drew:\n{}", drawn);
}

#[test]
fn test_a_guest_still_within_the_grace_period_is_not_offered_the_plug() {
    // Review Focus 2 cuts both ways: the hard way must appear for a hanging
    // guest, and must not tempt anyone while a normal shutdown is running.
    let i18n = I18n::new("en");
    let drawn = render_detail_with_status(
        VmStatus::ShuttingDown {
            pid: 1,
            since: SystemTime::now(),
        },
        true,
        &i18n,
    );
    assert!(drawn.contains(i18n.t("shutting_down")), "drew:\n{}", drawn);
    assert!(
        !drawn.contains(i18n.t("force_stop")),
        "a guest given one second has not hung yet, drew:\n{}",
        drawn
    );
}

#[test]
fn test_a_vm_that_cannot_shut_down_gracefully_says_so_in_the_panel() {
    let i18n = I18n::new("en");
    let drawn = render_detail_with_status(VmStatus::Running { pid: 1 }, false, &i18n);
    assert!(
        drawn.contains(i18n.t("shutdown_unavailable")),
        "drew:\n{}",
        drawn
    );
    assert!(
        drawn.contains(i18n.t("force_stop")),
        "and offers the hard way, drew:\n{}",
        drawn
    );
}
