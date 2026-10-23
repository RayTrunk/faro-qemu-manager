// FARO QEMU MANAGER (C) R.Trunk 2026
// VM Detail Tabs and Hardware Controls component

// Network tab of the VM detail panel.

use crate::config::PortForwardRule;
use crate::ui::theme::{card_frame, danger_button, icons, COLOR_WARNING};

/// Graphics adapters worth offering for `arch`.
///
/// `-vga` and its named types are an x86 convenience. On every other
/// architecture QEMU has no VGA adapter and answers `Virtio VGA not
use super::offers::*;
use super::VmDetailView;

impl<'a> VmDetailView<'a> {
    /// Tab 4: Network & Advanced
    pub(super) fn show_network_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading(self.i18n.t("tab_network"));
        ui.add_space(6.0);

        // Network Hardware Config
        card_frame(ui.visuals()).show(ui, |ui| {
            egui::Grid::new("vm_network_grid")
                .num_columns(2)
                .spacing([20.0, 14.0])
                .show(ui, |ui| {
                    // Backend
                    ui.label(egui::RichText::new(self.i18n.t("backend")).strong());
                    ui.horizontal(|ui| {
                        // "none" is ours, not QEMU's, so it is always on offer.
                        let net_backends = offered_netdev_backends(self.capabilities);
                        egui::ComboBox::from_id_salt("detail_net_backend_combo")
                            .selected_text(&self.vm.network.backend)
                            .show_ui(ui, |ui| {
                                for opt in &net_backends {
                                    ui.selectable_value(
                                        &mut self.vm.network.backend,
                                        opt.to_string(),
                                        *opt,
                                    );
                                }
                            });
                        let hint = match self.vm.network.backend.as_str() {
                            "user" => "(SLIRP NAT user-mode network with Internet access)",
                            "bridge" => "(Host bridged interface)",
                            "none" => "(No network connectivity)",
                            _ => "",
                        };
                        ui.label(
                            egui::RichText::new(hint)
                                .size(11.0)
                                .color(ui.visuals().weak_text_color()),
                        );
                    });
                    if !self.vm.network.port_forwards.is_empty()
                        && !self.vm.network.backend.eq_ignore_ascii_case("user")
                    {
                        // The rules stay in the configuration, so say plainly
                        // that this backend will not act on them.
                        ui.label(
                            egui::RichText::new(self.i18n.t("forwards_backend_only"))
                                .size(11.0)
                                .color(COLOR_WARNING),
                        );
                    }
                    ui.end_row();

                    // NIC model
                    ui.label(egui::RichText::new(self.i18n.t("nic_model")).strong());
                    ui.horizontal(|ui| {
                        let nic_options =
                            ["virtio-net-pci", "e1000", "rtl8139", "e1000e", "vmxnet3"];
                        egui::ComboBox::from_id_salt("detail_nic_model_combo")
                            .selected_text(&self.vm.network.nic_model)
                            .show_ui(ui, |ui| {
                                for opt in &nic_options {
                                    ui.selectable_value(
                                        &mut self.vm.network.nic_model,
                                        opt.to_string(),
                                        *opt,
                                    );
                                }
                            });
                        let hint = match self.vm.network.nic_model.as_str() {
                            "virtio-net-pci" => "(High performance VirtIO virtual NIC)",
                            "e1000" => "(Intel PRO/1000 Gigabit Ethernet - highly compatible)",
                            "rtl8139" => "(Realtek 8139 Fast Ethernet)",
                            _ => "",
                        };
                        ui.label(
                            egui::RichText::new(hint)
                                .size(11.0)
                                .color(ui.visuals().weak_text_color()),
                        );
                    });
                    ui.end_row();
                });
        });

        ui.add_space(8.0);

        // Port Forwarding Table
        card_frame(ui.visuals()).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading(self.i18n.t("port_forwards"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(format!("{} {}", icons::ADD, self.i18n.t("add_port_forward"))).clicked() {
                        self.vm.network.port_forwards.push(PortForwardRule {
                            host_port: 2222,
                            guest_port: 22,
                            proto: "tcp".to_string(),
                        });
                    }
                });
            });

            ui.add_space(4.0);

            if self.vm.network.port_forwards.is_empty() {
                ui.label(
                    egui::RichText::new("No port forwarding rules configured. Add rules to forward host ports into VM services (e.g. 2222 -> 22 for SSH, 8080 -> 80 for HTTP).")
                        .size(11.0)
                        .color(ui.visuals().weak_text_color())
                        .italics(),
                );
            } else {
                let mut del_idx = None;
                egui::Grid::new("port_forwards_table_grid")
                    .num_columns(5)
                    .spacing([14.0, 8.0])
                    .striped(true)
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new(self.i18n.t("host_port")).strong());
                        ui.label("->");
                        ui.label(egui::RichText::new(self.i18n.t("guest_port")).strong());
                        ui.label(egui::RichText::new(self.i18n.t("proto")).strong());
                        ui.label(egui::RichText::new(self.i18n.t("action")).strong());
                        ui.end_row();

                        for (idx, rule) in self.vm.network.port_forwards.iter_mut().enumerate() {
                            ui.add(egui::DragValue::new(&mut rule.host_port).range(1..=65535));
                            ui.label("->");
                            ui.add(egui::DragValue::new(&mut rule.guest_port).range(1..=65535));

                            let cur_proto = rule.proto.to_lowercase();
                            egui::ComboBox::from_id_salt(format!("proto_combo_{}", idx))
                                .selected_text(cur_proto.to_uppercase())
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut rule.proto, "tcp".to_string(), "TCP");
                                    ui.selectable_value(&mut rule.proto, "udp".to_string(), "UDP");
                                });

                            if danger_button(ui, icons::DELETE).on_hover_text(self.i18n.t("delete")).clicked() {
                                del_idx = Some(idx);
                            }
                            ui.end_row();
                        }
                    });

                if let Some(i) = del_idx {
                    self.vm.network.port_forwards.remove(i);
                }
            }
        });

        ui.add_space(8.0);

        // Extra Arguments Section
        card_frame(ui.visuals()).show(ui, |ui| {
            ui.heading(self.i18n.t("extra_args"));
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("Additional command-line flags passed directly to QEMU (e.g. -usb -device usb-tablet -audiodev dsound,id=snd0)")
                    .size(11.0)
                    .color(ui.visuals().weak_text_color()),
            );
            ui.add_space(4.0);
            ui.add(
                egui::TextEdit::multiline(&mut self.vm.extra_args)
                    .hint_text("-usb -device usb-tablet")
                    .desired_rows(3)
                    .desired_width(ui.available_width()),
            );
        });
    }
}
