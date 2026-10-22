// FARO QEM (C) R.Trunk 2026
// Unit tests for VM Detail Tabs and Hardware Controls component

use egui::Context;
use std::path::PathBuf;
use uuid::Uuid;

use faro_qem::config::{
    Accelerator, Arch, DriveConfig, DriveFormat, DriveInterface, PortForwardRule, VmConfig,
};
use faro_qem::i18n::I18n;
use faro_qem::ui::{render_vm_detail, CreateDiskState, DetailTab, VmDetailAction, VmDetailView};

#[test]
fn test_vm_detail_action_variants_and_helpers() {
    let id = Uuid::new_v4();

    let launch = VmDetailAction::Launch(id);
    let stop = VmDetailAction::Stop(id);
    let save = VmDetailAction::Save(id);

    assert_eq!(launch.vm_id(), id);
    assert_eq!(stop.vm_id(), id);
    assert_eq!(save.vm_id(), id);

    assert_eq!(launch, VmDetailAction::Launch(id));
    assert_ne!(launch, stop);
    assert_ne!(stop, save);
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

    state.open_dialog("ubuntu_disk.qcow2");
    assert!(state.open);
    assert_eq!(state.path_buf, "ubuntu_disk.qcow2");
    assert_eq!(state.status_msg, None);

    state.status_msg = Some(Err("Disk path invalid".to_string()));
    assert!(state.status_msg.is_some());

    state.close();
    assert!(!state.open);

    // Reopening clears previous status message
    state.open_dialog("");
    assert!(state.open);
    assert_eq!(state.status_msg, None);
}

#[test]
fn test_vm_detail_system_tab_rendering() {
    let ctx = Context::default();
    let i18n = I18n::new("en");
    let mut vm = VmConfig::new_default("Debian Workstation", Arch::X86_64);
    vm.cpu_cores = 4;
    vm.memory_mb = 4096;
    vm.accel = Accelerator::WHPX;
    vm.bios_uefi = Some(PathBuf::from(r"C:\Firmware\OVMF.fd"));

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
fn test_render_vm_detail_wrapper_function() {
    let ctx = Context::default();
    let i18n = I18n::new("de");
    let mut vm = VmConfig::new_default("Wrapper Test VM", Arch::Riscv64);
    let mut active_tab = DetailTab::System;
    let mut disk_modal = CreateDiskState::default();

    let _ = ctx.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let action = render_vm_detail(
                ui,
                &mut vm,
                false,
                &i18n,
                &mut active_tab,
                None,
                &mut disk_modal,
            );
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
