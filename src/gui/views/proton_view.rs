use eframe::egui::{self, Color32, RichText, Ui};
use std::path::{Path, PathBuf};

use crate::gui::backend::GuiCommand;
use crate::proton::ProtonConfigurator;

pub struct ProtonViewState {
    pub custom_prefix: String,
    pub port: u16,
    pub scope: String,
    pub status_message: Option<String>,
}

impl Default for ProtonViewState {
    fn default() -> Self {
        let initial_prefix = ProtonConfigurator::find_msfs_proton_prefix()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();

        // Check if existing SimConnect.xml already defines an unprivileged IPv4 port
        let mut initial_port = 45454;
        let mut initial_scope = "local".to_string();
        if !initial_prefix.is_empty() {
            if let Some(roaming) = ProtonConfigurator::find_msfs_roaming_path(Some(PathBuf::from(&initial_prefix))) {
                let ports = ProtonConfigurator::read_configured_ipv4_ports(&roaming);
                let min_unpriv = ProtonConfigurator::get_linux_unprivileged_port_start();
                if let Some((sc, p)) = ports.iter().find(|(_, p)| *p >= min_unpriv) {
                    initial_port = *p;
                    initial_scope = sc.clone();
                }
            }
        }

        Self {
            custom_prefix: initial_prefix,
            port: initial_port,
            scope: initial_scope,
            status_message: None,
        }
    }
}

pub fn render_proton_view(
    ui: &mut Ui,
    state: &mut ProtonViewState,
    app_simconnect_port: &mut u16,
    _cmd_tx: &tokio::sync::mpsc::UnboundedSender<GuiCommand>,
) {
    ui.heading(RichText::new("Steam Proton MSFS Auto-Detector & SimConnect Network Setup").color(Color32::from_rgb(180, 140, 255)));
    ui.add_space(4.0);

    ui.label("Under Linux Steam Proton, Windows Named Pipes (Protocol=Pipe) only work for processes running inside the same Wine prefix.");
    ui.label("AeroDeck parses Steam's libraryfolders.vdf across all internal and external drives to locate your MSFS prefix and configures an IPv4 TCP listener in SimConnect.xml.");
    ui.add_space(8.0);

    // If custom_prefix is empty, try auto-discovery
    if state.custom_prefix.trim().is_empty() {
        if let Some(auto_pfx) = ProtonConfigurator::find_msfs_proton_prefix() {
            state.custom_prefix = auto_pfx.to_string_lossy().to_string();
        }
    }

    let custom_opt = if state.custom_prefix.trim().is_empty() {
        None
    } else {
        Some(PathBuf::from(state.custom_prefix.trim()))
    };

    let active_roaming_path = ProtonConfigurator::find_msfs_roaming_path(custom_opt.clone());
    let min_unpriv_port = ProtonConfigurator::get_linux_unprivileged_port_start();

    ui.group(|ui| {
        ui.label(RichText::new("1. Proton Prefix & SimConnect.xml Status:").strong());

        if let Some(ref path) = active_roaming_path {
            ui.label(RichText::new("✔ MSFS Proton Roaming Directory Detected & Active:").color(Color32::from_rgb(0, 255, 120)).strong());
            ui.label(RichText::new(path.to_string_lossy()).monospace());

            let simconnect_xml = path.join("SimConnect.xml");
            if simconnect_xml.exists() {
                let configured_ports = ProtonConfigurator::read_configured_ipv4_ports(path);
                if configured_ports.is_empty() {
                    ui.label(RichText::new("⚠ SimConnect.xml exists, but no static IPv4 <Port> is defined (only Pipe or Dynamic Port 0).").color(Color32::from_rgb(255, 190, 50)));
                } else {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new("✔ Configured IPv4 Port(s) in SimConnect.xml:").color(Color32::from_rgb(0, 230, 255)));
                        for (sc, p) in &configured_ports {
                            ui.label(RichText::new(format!("Port {} (Scope: {})", p, sc)).strong());
                            if *app_simconnect_port != *p {
                                if ui.small_button(format!("Use Port {} in AeroDeck", p)).clicked() {
                                    *app_simconnect_port = *p;
                                    state.port = *p;
                                }
                            } else {
                                ui.label(RichText::new("[Active in Top Bar]").color(Color32::from_rgb(0, 255, 120)).small());
                            }
                        }
                    });

                    // Check if all configured ports are privileged (< 1024) on Linux
                    if min_unpriv_port > 0 && configured_ports.iter().all(|(_, p)| *p < min_unpriv_port) {
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new(format!(
                                "⚠ Linux Port Restriction Notice: SimConnect.xml only defines port(s) below {} (standard Windows convention Port 500). \
                                On Linux, non-root processes (including Steam Proton) cannot bind TCP ports < {}. \
                                Select port 2048 or 45454 below and click Configure so MSFS can open the TCP socket under Proton!",
                                min_unpriv_port, min_unpriv_port
                            ))
                            .color(Color32::from_rgb(255, 190, 50)),
                        );
                    }
                }
            } else {
                ui.label(RichText::new("○ SimConnect.xml not yet created in this prefix. Click Configure below to install.").color(Color32::from_rgb(255, 190, 50)));
            }
        } else {
            ui.label(RichText::new("✖ MSFS Proton prefix not found at the path below. Use 'Browse...' or 'Auto-Detect' to locate your MSFS compatdata/1250410/pfx folder.").color(Color32::LIGHT_RED).strong());
        }

        ui.add_space(8.0);

        // Path input with Browse and Auto-detect buttons
        ui.horizontal(|ui| {
            ui.label("Prefix Path:");
            ui.add(egui::TextEdit::singleline(&mut state.custom_prefix).desired_width(420.0));

            if ui.button("📁 Browse...").clicked() {
                let mut dialog = rfd::FileDialog::new()
                    .set_title("Select MSFS Steam Proton Prefix Folder (pfx or compatdata/1250410)");

                if !state.custom_prefix.trim().is_empty() {
                    let current = PathBuf::from(state.custom_prefix.trim());
                    if current.exists() {
                        dialog = dialog.set_directory(&current);
                    } else if let Some(parent) = current.parent() {
                        if parent.exists() {
                            dialog = dialog.set_directory(parent);
                        }
                    }
                } else if let Some(ref auto) = ProtonConfigurator::find_msfs_proton_prefix() {
                    if let Some(parent) = auto.parent() {
                        dialog = dialog.set_directory(parent);
                    }
                } else if Path::new("/mnt").exists() {
                    dialog = dialog.set_directory("/mnt");
                }

                if let Some(folder) = dialog.pick_folder() {
                    state.custom_prefix = folder.to_string_lossy().to_string();
                    state.status_message = None;
                }
            }

            if ui.button("🔄 Auto-Detect via Steam").clicked() {
                if let Some(auto_pfx) = ProtonConfigurator::find_msfs_proton_prefix() {
                    state.custom_prefix = auto_pfx.to_string_lossy().to_string();
                    state.status_message = Some(format!("Success! Auto-detected prefix from Steam library metadata: {:?}", auto_pfx));
                } else {
                    state.status_message = Some("No prefix detected automatically. Please click 'Browse...' to select your prefix folder.".to_string());
                }
            }
        });
    });

    ui.add_space(8.0);

    // 2. Port & Scope Configuration
    ui.group(|ui| {
        ui.label(RichText::new("2. SimConnect TCP Port & Scope Configuration:").strong());
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.label("SimConnect TCP Port:");
            let mut port_str = state.port.to_string();
            if ui.add(egui::TextEdit::singleline(&mut port_str).desired_width(65.0)).changed() {
                if let Ok(p) = port_str.parse::<u16>() {
                    state.port = p;
                }
            }

            ui.separator();
            ui.label("Presets:");
            if ui.selectable_label(state.port == 45454, "45454 (Linux Proton Recommended)").clicked() {
                state.port = 45454;
            }
            if ui.selectable_label(state.port == 2048, "2048 (FS-FlightControl Convention)").clicked() {
                state.port = 2048;
            }
            if ui.selectable_label(state.port == 500, "500 (Windows SDK Convention)").clicked() {
                state.port = 500;
            }
        });

        if min_unpriv_port > 0 && state.port < min_unpriv_port {
            ui.label(
                RichText::new(format!(
                    "⚠ Warning: Port {} is below Linux's unprivileged port start ({}). Unless you have lowered net.ipv4.ip_unprivileged_port_start, MSFS under Proton will fail to bind this port. Port 2048 or 45454 is recommended on Linux.",
                    state.port, min_unpriv_port
                ))
                .color(Color32::from_rgb(255, 170, 50))
                .small(),
            );
        }

        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.label("Network Scope:");
            ui.selectable_value(&mut state.scope, "local".to_string(), "local (127.0.0.1 - Same PC)");
            ui.selectable_value(&mut state.scope, "global".to_string(), "global (0.0.0.0 - Allow Remote LAN PCs)");
        });

        ui.add_space(8.0);

        ui.horizontal(|ui| {
            if ui
                .button(
                    RichText::new("🚀 Configure SimConnect.xml & Sync AeroDeck Port")
                        .color(Color32::from_rgb(0, 255, 120))
                        .strong(),
                )
                .clicked()
            {
                let custom_opt = if state.custom_prefix.trim().is_empty() {
                    None
                } else {
                    Some(PathBuf::from(state.custom_prefix.trim()))
                };

                match ProtonConfigurator::configure_simconnect_with_scope(custom_opt, state.port, &state.scope) {
                    Ok(path) => {
                        *app_simconnect_port = state.port;
                        state.status_message = Some(format!(
                            "Success! Updated {:?} (Port {}, Scope {}) and synced AeroDeck connection port to {}.",
                            path, state.port, state.scope, state.port
                        ));
                    }
                    Err(e) => {
                        state.status_message = Some(format!("Error: {}", e));
                    }
                }
            }

            if *app_simconnect_port != state.port {
                if ui.button(format!("Sync Top Bar to Port {}", state.port)).clicked() {
                    *app_simconnect_port = state.port;
                }
            }
        });

        if let Some(ref msg) = state.status_message {
            ui.add_space(6.0);
            let color = if msg.starts_with("Success") { Color32::from_rgb(0, 255, 120) } else { Color32::LIGHT_RED };
            ui.label(RichText::new(msg).color(color).strong());
        }
    });

    ui.add_space(8.0);

    // 3. Collapsible Reference Snippets & Discovered Steam Libraries
    let discovered_libs = ProtonConfigurator::discover_steam_libraries();
    if !discovered_libs.is_empty() {
        ui.collapsing("Discovered Steam Libraries (from libraryfolders.vdf)", |ui| {
            for lib in &discovered_libs {
                ui.label(RichText::new(format!("• {}", lib.display())).small().color(Color32::LIGHT_GRAY));
            }
        });
    }

    ui.collapsing("Preview Server SimConnect.xml & Client SimConnect.cfg Snippets", |ui| {
        ui.label(RichText::new("Server (SimConnect.xml in MSFS AppData\\Roaming):").strong().small());
        let xml_preview = ProtonConfigurator::generate_simconnect_xml_with_scope(state.port, &state.scope);
        egui::ScrollArea::vertical().max_height(140.0).id_salt("xml_preview_scroll").show(ui, |ui| {
            ui.add(egui::TextEdit::multiline(&mut xml_preview.as_str()).font(egui::TextStyle::Monospace).desired_width(f32::INFINITY));
        });

        ui.add_space(4.0);
        ui.label(RichText::new("Client (SimConnect.cfg for remote/third-party tools):").strong().small());
        let cfg_preview = ProtonConfigurator::generate_simconnect_cfg("127.0.0.1", state.port);
        ui.add(egui::TextEdit::multiline(&mut cfg_preview.as_str()).font(egui::TextStyle::Monospace).desired_width(f32::INFINITY).desired_rows(6));
    });
}
