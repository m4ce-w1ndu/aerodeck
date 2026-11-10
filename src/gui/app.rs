use eframe::egui::{self, RichText, Ui};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tokio::sync::mpsc;

use crate::gui::backend::{GuiCommand, GuiEvent};
use crate::gui::views::aircraft_profiles_view::{
    render_aircraft_profiles_view, AircraftProfilesViewState,
};
use crate::gui::views::log_view::{render_log_view, LogEntry};
use crate::gui::views::lvar_view::{render_lvar_view, LvarViewState};
use crate::gui::views::mapping_view::{render_mapping_view, MappingViewState};
use crate::gui::views::proton_view::{render_proton_view, ProtonViewState};
use crate::gui::views::virtual_panel_view::{render_virtual_panel_view, VirtualPanelViewState};
use crate::hardware::enumerator::{ConnectedDevice, DeviceEnumerator};
use crate::profile::schema::SpadProfile;
use crate::profile::{
    AircraftAssignmentManager, AircraftScanner, DiscoveredAircraft, ProfileRepository,
};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum ActiveTab {
    Mapping,
    AircraftAndProfiles,
    Lvars,
    VirtualPanels,
    ProtonSetup,
    Logs,
}

#[derive(Debug, Clone)]
pub struct LiveAxisState {
    pub raw: i32,
    pub normalized: f64,
    pub min: i32,
    pub max: i32,
    pub last_updated: Instant,
}

pub struct SpadLinxApp {
    // Channels
    cmd_tx: mpsc::UnboundedSender<GuiCommand>,
    event_rx: mpsc::UnboundedReceiver<GuiEvent>,

    // App state
    active_tab: ActiveTab,
    simconnect_host: String,
    simconnect_port: u16,
    simconnect_connected: bool,
    #[allow(dead_code)]
    simconnect_msg: String,

    bridge_active: bool,
    bridge_lvar_count: usize,
    aircraft_name: String,

    profile: Option<SpadProfile>,
    profile_path_input: String,
    profile_name: String,
    profile_is_dirty: bool,
    profile_is_auto_matched: bool,

    // Modals & Popovers
    show_connection_dialog: bool,
    show_options_menu: bool,
    show_about_dialog: bool,
    ui_scale: f32,

    // Real connected devices & discovered MSFS aircraft
    devices: Vec<ConnectedDevice>,
    discovered_aircrafts: Vec<DiscoveredAircraft>,
    aircraft_assignments: AircraftAssignmentManager,

    // Real-time telemetry
    last_input: Option<(String, String, String, Instant)>,
    discovered_lvars: Vec<String>,
    live_lvar_values: HashMap<String, f64>,
    live_simvars: Vec<(String, Option<f64>)>,
    pmdg_status: Vec<crate::gui::backend::PmdgUiStatus>,
    pub live_axes: HashMap<String, LiveAxisState>,
    logs: Vec<LogEntry>,
    auto_scroll_logs: bool,

    // View states
    mapping_state: MappingViewState,
    aircraft_profiles_state: AircraftProfilesViewState,
    lvar_state: LvarViewState,
    virtual_state: VirtualPanelViewState,
    proton_state: ProtonViewState,
    pub theme: crate::gui::theme::AeroTheme,
}

impl SpadLinxApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        cmd_tx: mpsc::UnboundedSender<GuiCommand>,
        event_rx: mpsc::UnboundedReceiver<GuiEvent>,
        initial_profile: Option<PathBuf>,
        host: String,
        port: u16,
    ) -> Self {
        let theme = crate::gui::theme::AeroTheme::dark();
        theme.apply(&cc.egui_ctx);
        let initial_path_str = initial_profile
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "default_c172.xml".to_string());

        let proton_state = ProtonViewState::default();
        let initial_port = if port == 45454 && proton_state.port != 45454 {
            proton_state.port
        } else {
            port
        };

        // Built-in profiles are no longer written to ./profiles on startup; they are
        // installed only when picked from the repository.
        let discovered_aircrafts = AircraftScanner::discover_installed_aircraft(None);
        let aircraft_assignments = AircraftAssignmentManager::load_or_default();

        let mut app = Self {
            cmd_tx,
            event_rx,
            active_tab: ActiveTab::Mapping,
            simconnect_host: host.clone(),
            simconnect_port: initial_port,
            simconnect_connected: false,
            simconnect_msg: "Not connected".to_string(),
            bridge_active: false,
            bridge_lvar_count: 0,
            aircraft_name: "No Aircraft Detected".to_string(),
            profile: None,
            profile_path_input: initial_path_str,
            profile_name: "No Profile Loaded".to_string(),
            profile_is_dirty: false,
            profile_is_auto_matched: false,
            show_connection_dialog: false,
            show_options_menu: false,
            show_about_dialog: false,
            ui_scale: 1.0,
            devices: DeviceEnumerator::enumerate_all(),
            discovered_aircrafts,
            aircraft_assignments,
            last_input: None,
            discovered_lvars: Vec::new(),
            live_lvar_values: HashMap::new(),
            live_simvars: Vec::new(),
            pmdg_status: Vec::new(),
            live_axes: HashMap::new(),
            logs: Vec::new(),
            auto_scroll_logs: true,
            mapping_state: MappingViewState::default(),
            aircraft_profiles_state: AircraftProfilesViewState::default(),
            lvar_state: LvarViewState::default(),
            virtual_state: VirtualPanelViewState::default(),
            proton_state,
            theme,
        };

        // Try loading initial profile if file exists
        if let Some(ref p) = initial_profile {
            if p.exists() {
                if let Ok(prof) = crate::profile::ProfileParser::parse_file(p) {
                    app.profile_name = prof.name.clone().unwrap_or_else(|| "C172 Profile".to_string());
                    app.profile = Some(prof);
                }
            }
        } else if std::path::Path::new("default_c172.xml").exists() {
            if let Ok(prof) = crate::profile::ProfileParser::parse_file("default_c172.xml") {
                app.profile_name = prof.name.clone().unwrap_or_else(|| "C172 Default".to_string());
                app.profile = Some(prof);
            }
        }

        app
    }

    fn poll_events(&mut self) {
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                GuiEvent::SimConnectStatus { connected, message } => {
                    self.simconnect_connected = connected;
                    self.simconnect_msg = message.clone();
                    self.logs.push(LogEntry {
                        timestamp: chrono_time(),
                        level: if connected { "INFO".to_string() } else { "WARN".to_string() },
                        text: format!("SimConnect status: {}", message),
                    });
                }
                GuiEvent::BridgeStatus { active, lvar_count } => {
                    self.bridge_active = active;
                    self.bridge_lvar_count = lvar_count;
                }
                GuiEvent::AircraftChanged(name) => {
                    self.aircraft_name = name.clone();
                    self.logs.push(LogEntry {
                        timestamp: chrono_time(),
                        level: "INFO".to_string(),
                        text: format!("Active aircraft: {}", name),
                    });

                    // Auto-switch profile matching the detected aircraft
                    if self.aircraft_assignments.auto_switch_enabled && name != "No Aircraft Detected" {
                        let local_profiles = ProfileRepository::list_local_profiles();
                        let mut matched_path = None;
                        let mut fallback_path = None;

                        for p_str in &local_profiles {
                            if let Ok(p) = crate::profile::ProfileParser::parse_file(p_str) {
                                if p.matches_aircraft_title(&name) {
                                    matched_path = Some(p_str.clone());
                                    break;
                                }
                                if p.is_fallback_profile() && fallback_path.is_none() {
                                    fallback_path = Some(p_str.clone());
                                }
                            }
                        }

                        let target_path = matched_path.or(fallback_path);
                        if let Some(target) = target_path {
                            if target != self.profile_path_input {
                                let _ = self.cmd_tx.send(GuiCommand::LoadProfile(PathBuf::from(&target)));
                                self.profile_is_auto_matched = true;
                                self.logs.push(LogEntry {
                                    timestamp: chrono_time(),
                                    level: "INFO".to_string(),
                                    text: format!("Auto-switched profile to '{}' for aircraft '{}'", target, name),
                                });
                            }
                        }
                    }
                }
                GuiEvent::AircraftCatalogUpdated { aircrafts, assignments } => {
                    self.discovered_aircrafts = aircrafts;
                    self.aircraft_assignments = assignments;
                }
                GuiEvent::RepositoryProfileInstalled { id: _, name, path } => {
                    self.logs.push(LogEntry {
                        timestamp: chrono_time(),
                        level: "INFO".to_string(),
                        text: format!("Installed repository profile '{}' ({})", name, path),
                    });
                }
                GuiEvent::OnlineProfilesQueried(resp) => {
                    self.aircraft_profiles_state.online_soap_response = resp;
                }
                GuiEvent::OnlineSpadCatalogLoaded(entries) => {
                    self.aircraft_profiles_state.is_fetching_online = false;
                    self.aircraft_profiles_state.online_catalog = entries;
                }
                GuiEvent::WindowsSpadProfilesUpdated(win_profs) => {
                    self.aircraft_profiles_state.windows_profiles = win_profs;
                }
                GuiEvent::DevicesUpdated(devs) => {
                    self.devices = devs;
                }
                GuiEvent::InputTriggered { device, bound_to, action, time } => {
                    self.last_input = Some((device.clone(), bound_to.clone(), action.clone(), time));
                }
                GuiEvent::AxisMoved { device, bound_to, raw_value, normalized, min, max, time } => {
                    let st = LiveAxisState {
                        raw: raw_value,
                        normalized,
                        min,
                        max,
                        last_updated: time,
                    };
                    self.live_axes.insert(format!("{}:{}", device, bound_to), st.clone());
                    self.live_axes.insert(bound_to.clone(), st);
                    self.last_input = Some((device, bound_to, format!("{:.1}%", normalized * 100.0), time));
                }
                GuiEvent::LvarsDiscovered(lvars) => {
                    self.discovered_lvars = lvars;
                    self.bridge_lvar_count = self.discovered_lvars.len();
                }
                GuiEvent::PmdgStatus(rows) => {
                    self.pmdg_status = rows;
                }
                GuiEvent::SimVarsUpdated(list) => {
                    self.live_simvars = list;
                }
                GuiEvent::LvarUpdated { name, value } => {
                    self.live_lvar_values.insert(name, value);
                }
                GuiEvent::Log { timestamp, level, text } => {
                    self.logs.push(LogEntry { timestamp, level, text });
                }
                GuiEvent::ProfileLoaded { name, path } => {
                    self.profile_name = name.clone();
                    self.profile_path_input = path.clone();
                    if let Ok(prof) = crate::profile::ProfileParser::parse_file(&path) {
                        self.profile = Some(prof);
                    }
                    self.profile_is_dirty = false;
                    self.logs.push(LogEntry {
                        timestamp: chrono_time(),
                        level: "INFO".to_string(),
                        text: format!("Loaded profile '{}' from {}", name, path),
                    });
                }
                GuiEvent::ProfileSaved { path } => {
                    self.profile_is_dirty = false;
                    self.logs.push(LogEntry {
                        timestamp: chrono_time(),
                        level: "INFO".to_string(),
                        text: format!("Successfully saved profile to {}", path),
                    });
                }
                GuiEvent::Error(err) => {
                    self.logs.push(LogEntry {
                        timestamp: chrono_time(),
                        level: "ERROR".to_string(),
                        text: err,
                    });
                }
            }
        }
    }
}

impl eframe::App for SpadLinxApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_events();

        // Check keyboard shortcuts for navigation (Ctrl+1 .. Ctrl+6)
        ctx.input(|i| {
            if i.modifiers.command || i.modifiers.ctrl {
                if i.key_pressed(egui::Key::Num1) {
                    self.active_tab = ActiveTab::Mapping;
                } else if i.key_pressed(egui::Key::Num2) {
                    self.active_tab = ActiveTab::AircraftAndProfiles;
                } else if i.key_pressed(egui::Key::Num3) {
                    self.active_tab = ActiveTab::Lvars;
                } else if i.key_pressed(egui::Key::Num4) {
                    self.active_tab = ActiveTab::VirtualPanels;
                } else if i.key_pressed(egui::Key::Num5) {
                    self.active_tab = ActiveTab::ProtonSetup;
                } else if i.key_pressed(egui::Key::Num6) {
                    self.active_tab = ActiveTab::Logs;
                }
            }
        });

        // Top Status & Navigation Bar: Reduced to max 2 rows
        egui::TopBottomPanel::top("top_header_panel")
            .frame(
                egui::Frame::none()
                    .fill(self.theme.bg_surface)
                    .stroke(egui::Stroke::new(1.0_f32, self.theme.border_subtle))
                    .inner_margin(egui::Margin::symmetric(14.0, 7.0)),
            )
            .show(ctx, |ui| {
                // Row 1: Logo, Version, Tabs, Active Profile, Connection Badge, Options
                render_header_row_1(self, ui);

                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);

                // Row 2: Thin context bar (Active aircraft with match source, live input strip, Proton ready, LVAR count)
                render_context_row_2(self, ui);
            });

        // Main Central Workspace
        egui::CentralPanel::default().show(ctx, |ui| {
            match self.active_tab {
                ActiveTab::Mapping => {
                    render_mapping_view(
                        ui,
                        &mut self.mapping_state,
                        &mut self.profile,
                        &self.profile_name,
                        &self.profile_path_input,
                        &self.aircraft_name,
                        &self.discovered_aircrafts,
                        &self.aircraft_assignments,
                        &self.cmd_tx,
                        &self.last_input,
                        &self.discovered_lvars,
                        &self.devices,
                        &self.live_axes,
                        &self.theme,
                    );
                }
                ActiveTab::AircraftAndProfiles => {
                    render_aircraft_profiles_view(
                        ui,
                        &mut self.aircraft_profiles_state,
                        &self.discovered_aircrafts,
                        &self.aircraft_assignments,
                        &self.devices,
                        &self.aircraft_name,
                        &self.profile_name,
                        &self.profile_path_input,
                        &self.proton_state.custom_prefix,
                        &self.cmd_tx,
                        &self.theme,
                    );
                }
                ActiveTab::Lvars => {
                    render_lvar_view(
                        ui,
                        &mut self.lvar_state,
                        &self.cmd_tx,
                        &self.discovered_lvars,
                        &self.live_lvar_values,
                        &self.live_simvars,
                        &self.pmdg_status,
                        self.bridge_active,
                        &self.theme,
                    );
                }
                // Single-column views scroll as a whole, using the full panel height.
                ActiveTab::VirtualPanels => {
                    egui::ScrollArea::vertical().id_salt("panels_page").auto_shrink([false, false]).show(ui, |ui| {
                        render_virtual_panel_view(ui, &mut self.virtual_state, &self.cmd_tx);
                    });
                }
                ActiveTab::ProtonSetup => {
                    egui::ScrollArea::vertical().id_salt("proton_page").auto_shrink([false, false]).show(ui, |ui| {
                        render_proton_view(ui, &mut self.proton_state, &mut self.simconnect_port, &self.cmd_tx);
                    });
                }
                ActiveTab::Logs => {
                    render_log_view(ui, &self.logs, &mut self.auto_scroll_logs);
                }
            }
        });

        // Modals & Popovers
        render_connection_dialog(self, ctx);
        render_options_menu(self, ctx);
        render_about_dialog(self, ctx);

        // On-demand repaint throttling: continuous 25-30 FPS only while axes/inputs move or sim sends data; idle otherwise
        let has_recent_input = self
            .last_input
            .as_ref()
            .map(|(_, _, _, inst)| inst.elapsed().as_millis() < 1200)
            .unwrap_or(false);
        let is_sim_active = self.simconnect_connected && (self.bridge_active || has_recent_input);

        let repaint_interval = if has_recent_input || is_sim_active {
            std::time::Duration::from_millis(40)
        } else {
            std::time::Duration::from_millis(600)
        };
        ctx.request_repaint_after(repaint_interval);
    }
}

fn render_header_row_1(app: &mut SpadLinxApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        // App title & version pill
        ui.horizontal(|ui| {
            ui.heading(
                RichText::new("AERODECK")
                    .color(app.theme.accent)
                    .strong(),
            );
            crate::gui::widgets::ui_tag_pill(
                ui,
                concat!("v", env!("CARGO_PKG_VERSION")),
                app.theme.accent,
                &app.theme,
            );
        });

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(6.0);

        // Navigation Tabs
        let tabs = [
            (ActiveTab::Mapping, "🎮 Mapping", "Ctrl+1"),
            (ActiveTab::AircraftAndProfiles, "✈ Profiles", "Ctrl+2"),
            (ActiveTab::Lvars, "🔍 LVARs", "Ctrl+3"),
            (ActiveTab::VirtualPanels, "🕹 Panels", "Ctrl+4"),
            (ActiveTab::ProtonSetup, "⚙ Proton", "Ctrl+5"),
            (ActiveTab::Logs, "📋 Logs", "Ctrl+6"),
        ];

        for (tab, label, shortcut) in tabs {
            let is_active = app.active_tab == tab;
            let text = if is_active {
                RichText::new(label)
                    .color(app.theme.accent)
                    .strong()
            } else {
                RichText::new(label).color(app.theme.text_secondary)
            };

            if ui
                .selectable_label(is_active, text)
                .on_hover_text(format!("Navigate to {} ({})", label, shortcut))
                .clicked()
            {
                app.active_tab = tab;
            }
        }

        // Right side: Active profile dropdown, connection badge, options menu
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Options Menu Button
            if ui
                .button(RichText::new("☰ Menu").color(app.theme.text_primary).strong())
                .on_hover_text("Open theme, UI scaling, and about dialog")
                .clicked()
            {
                app.show_options_menu = !app.show_options_menu;
            }

            ui.add_space(6.0);
            ui.separator();
            ui.add_space(6.0);

            // Active Profile Dropdown with dirty star and Save button
            if app.profile_is_dirty
                && ui
                    .button(RichText::new("💾 Save").color(app.theme.success).strong())
                    .on_hover_text("Save modified profile to disk")
                    .clicked()
            {
                let path = PathBuf::from(&app.profile_path_input);
                if let Some(ref prof) = app.profile {
                    let _ = app.cmd_tx.send(GuiCommand::UpdateAndSaveProfile {
                        profile: prof.clone(),
                        path,
                        bind_to_aircraft: None,
                    });
                } else {
                    let _ = app.cmd_tx.send(GuiCommand::SaveProfile(path));
                }
                app.profile_is_dirty = false;
            }

            let local_profiles = ProfileRepository::list_local_profiles();
            let display_name = if app.profile_is_dirty {
                format!("📁 {} *", app.profile_name)
            } else {
                format!("📁 {}", app.profile_name)
            };

            egui::ComboBox::from_id_salt("header_active_profile_select")
                .width(180.0)
                .selected_text(
                    RichText::new(display_name).color(if app.profile_is_dirty {
                        app.theme.accent
                    } else {
                        app.theme.text_primary
                    }),
                )
                .show_ui(ui, |ui| {
                    for p_path in local_profiles {
                        let name = Path::new(&p_path)
                            .file_stem()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_else(|| p_path.clone());
                        let is_cur = app.profile_path_input == p_path;
                        if ui.selectable_label(is_cur, name).clicked() {
                            let _ = app
                                .cmd_tx
                                .send(GuiCommand::LoadProfile(PathBuf::from(&p_path)));
                            app.profile_is_auto_matched = false; // User manually chose this profile
                        }
                    }
                });

            ui.add_space(6.0);
            ui.separator();
            ui.add_space(6.0);

            // Connection Badge Button
            let status_text = if app.simconnect_connected {
                "⏺ SimConnect Online"
            } else {
                "○ SimConnect Offline"
            };
            let status_color = if app.simconnect_connected {
                app.theme.success
            } else {
                app.theme.text_muted
            };
            let conn_btn = egui::Button::new(
                RichText::new(status_text)
                    .color(status_color)
                    .strong()
                    .small(),
            );
            if ui
                .add(conn_btn)
                .on_hover_text("Click to configure SimConnect host and TCP port")
                .clicked()
            {
                app.show_connection_dialog = !app.show_connection_dialog;
            }
        });
    });
}

fn render_context_row_2(app: &mut SpadLinxApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        // Active Aircraft + Match Source
        let ac_detected = app.aircraft_name != "No Aircraft Detected";
        let ac_color = if ac_detected {
            app.theme.accent
        } else {
            app.theme.text_muted
        };
        ui.label(
            RichText::new(format!("✈ Aircraft: {}", app.aircraft_name))
                .color(ac_color)
                .small()
                .strong(),
        );

        if ac_detected {
            if app.profile_is_auto_matched {
                crate::gui::widgets::ui_tag_pill(ui, "Auto-matched", app.theme.success, &app.theme);
            } else {
                crate::gui::widgets::ui_tag_pill(ui, "Manual override", app.theme.accent, &app.theme);
            }
        }

        ui.add_space(6.0);
        ui.separator();
        ui.add_space(6.0);

        // Hardware Input Monitor Strip
        if let Some((_, bound, act, inst)) = &app.last_input {
            let recent = inst.elapsed().as_millis() < 1200;
            let col = if recent {
                app.theme.success
            } else {
                app.theme.text_secondary
            };
            ui.label(
                RichText::new(format!("⏺ Input: {} [{}]", bound, act))
                    .color(col)
                    .small()
                    .strong(),
            );
        } else {
            ui.label(
                RichText::new("Awaiting hardware input...")
                    .color(app.theme.text_muted)
                    .small()
                    .italics(),
            );
        }

        ui.add_space(6.0);
        ui.separator();
        ui.add_space(6.0);

        ui.label(
            RichText::new(format!("LVARs: {}", app.bridge_lvar_count))
                .small()
                .color(app.theme.text_muted),
        );

        // Right side: Steam Proton indicator
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let custom_pfx = if app.proton_state.custom_prefix.trim().is_empty() {
                None
            } else {
                Some(PathBuf::from(app.proton_state.custom_prefix.trim()))
            };
            let proton_ready =
                crate::proton::ProtonConfigurator::find_msfs_roaming_path(custom_pfx).is_some();
            let p_color = if proton_ready {
                app.theme.success
            } else {
                app.theme.text_muted
            };
            ui.label(
                RichText::new(if proton_ready {
                    "⏺ Steam Proton Ready"
                } else {
                    "○ Steam Proton Offline"
                })
                .color(p_color)
                .small(),
            );
        });
    });
}

fn render_connection_dialog(app: &mut SpadLinxApp, ctx: &egui::Context) {
    if !app.show_connection_dialog {
        return;
    }

    let mut is_open = true;
    let mut close_clicked = false;
    egui::Window::new("SimConnect Connection")
        .id(egui::Id::new("simconnect_conn_dialog"))
        .collapsible(false)
        .resizable(false)
        .open(&mut is_open)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.set_min_width(320.0);
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                ui.label(RichText::new("Status:").color(app.theme.text_secondary).strong());
                let (status_str, col) = if app.simconnect_connected {
                    ("Connected to Simulator", app.theme.success)
                } else {
                    ("Disconnected", app.theme.text_muted)
                };
                ui.label(RichText::new(status_str).color(col).strong());
            });

            ui.add_space(8.0);

            ui.horizontal(|ui| {
                ui.label(RichText::new("Host IP:").color(app.theme.text_secondary));
                ui.add(egui::TextEdit::singleline(&mut app.simconnect_host).desired_width(140.0));
            });

            ui.add_space(4.0);

            ui.horizontal(|ui| {
                ui.label(RichText::new("TCP Port:").color(app.theme.text_secondary));
                let mut port_str = app.simconnect_port.to_string();
                if ui.add(egui::TextEdit::singleline(&mut port_str).desired_width(80.0)).changed() {
                    if let Ok(p) = port_str.parse::<u16>() {
                        app.simconnect_port = p;
                    }
                }
            });

            ui.add_space(12.0);
            ui.separator();
            ui.add_space(6.0);

            ui.horizontal(|ui| {
                if app.simconnect_connected {
                    if ui.button(RichText::new("Disconnect").color(app.theme.danger).strong()).clicked() {
                        let _ = app.cmd_tx.send(GuiCommand::DisconnectSimConnect);
                    }
                } else if ui.button(RichText::new("⚡ Connect Now").color(app.theme.success).strong()).clicked() {
                    let _ = app.cmd_tx.send(GuiCommand::ConnectSimConnect {
                        host: app.simconnect_host.clone(),
                        port: app.simconnect_port,
                    });
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Close").clicked() {
                        close_clicked = true;
                    }
                });
            });
        });
    app.show_connection_dialog = is_open && !close_clicked;
}

fn render_options_menu(app: &mut SpadLinxApp, ctx: &egui::Context) {
    if !app.show_options_menu {
        return;
    }

    let mut is_open = true;
    let mut open_about = false;
    egui::Window::new("AeroDeck Options")
        .id(egui::Id::new("aerodeck_options_window"))
        .collapsible(false)
        .resizable(false)
        .open(&mut is_open)
        .anchor(egui::Align2::RIGHT_TOP, [-16.0, 50.0])
        .show(ctx, |ui| {
            ui.set_min_width(260.0);
            ui.add_space(4.0);

            ui.heading(RichText::new("Appearance").color(app.theme.accent).strong());
            ui.add_space(4.0);

            // Theme switch
            ui.horizontal(|ui| {
                ui.label(RichText::new("Theme:").color(app.theme.text_secondary));
                let is_dark = app.theme.mode == crate::gui::theme::ThemeMode::Dark;
                if ui.selectable_label(is_dark, "🌙 Dark").clicked() {
                    app.theme = crate::gui::theme::AeroTheme::dark();
                    app.theme.apply(ctx);
                }
                if ui.selectable_label(!is_dark, "☀ Light").clicked() {
                    app.theme = crate::gui::theme::AeroTheme::light();
                    app.theme.apply(ctx);
                }
            });

            ui.add_space(6.0);

            // UI Scale selector (90% / 100% / 115% / 130%)
            ui.horizontal(|ui| {
                ui.label(RichText::new("Scale:").color(app.theme.text_secondary));
                let scales = [
                    ("90%", 0.90_f32),
                    ("100%", 1.0_f32),
                    ("115%", 1.15_f32),
                    ("130%", 1.30_f32),
                ];
                for (name, val) in scales {
                    let is_sel = (app.ui_scale - val).abs() < 0.02;
                    if ui.selectable_label(is_sel, name).clicked() {
                        app.ui_scale = val;
                        ctx.set_zoom_factor(val);
                    }
                }
            });

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(6.0);

            if ui
                .button(RichText::new("ℹ About AeroDeck...").color(app.theme.text_primary))
                .clicked()
            {
                open_about = true;
            }
        });
    app.show_options_menu = is_open && !open_about;
    if open_about {
        app.show_about_dialog = true;
    }
}

fn render_about_dialog(app: &mut SpadLinxApp, ctx: &egui::Context) {
    if !app.show_about_dialog {
        return;
    }

    let mut is_open = true;
    let mut close_clicked = false;
    egui::Window::new("About AeroDeck")
        .id(egui::Id::new("about_aerodeck_window"))
        .collapsible(false)
        .resizable(false)
        .open(&mut is_open)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.set_max_width(480.0);
            ui.add_space(6.0);

            ui.vertical_centered(|ui| {
                ui.heading(RichText::new("AERODECK").color(app.theme.accent).strong());
                ui.label(
                    RichText::new(concat!("Version ", env!("CARGO_PKG_VERSION")))
                        .color(app.theme.text_secondary)
                        .strong(),
                );
                ui.add_space(4.0);
                ui.label(
                    RichText::new(
                        "High-Performance Flight Simulator Hardware Engine & Controller Bridge",
                    )
                    .color(app.theme.text_primary),
                );
            });

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                ui.label(RichText::new("Author:").color(app.theme.text_secondary));
                ui.label(RichText::new("Simone Rolando").strong());
            });

            ui.horizontal(|ui| {
                ui.label(RichText::new("Repository:").color(app.theme.text_secondary));
                ui.hyperlink_to(
                    "github.com/m4ce-w1ndu/aerodeck",
                    "https://github.com/m4ce-w1ndu/aerodeck",
                );
            });

            ui.horizontal(|ui| {
                ui.label(RichText::new("License:").color(app.theme.text_secondary));
                ui.label("Dual-licensed under MIT OR Apache-2.0");
            });

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(6.0);

            ui.label(
                RichText::new(
                    "Disclaimer: AeroDeck is an independent open-source project and is not affiliated with, authorized, maintained, sponsored, or endorsed by Microsoft Corporation, Asobo Studio, Lockheed Martin, Laminar Research, or SPAD.neXt. All product and company names are trademarks™ or registered® trademarks of their respective holders.",
                )
                .color(app.theme.text_muted)
                .small(),
            );

            ui.add_space(12.0);
            ui.vertical_centered(|ui| {
                if ui.button(RichText::new("Close").strong()).clicked() {
                    close_clicked = true;
                }
            });
        });
    app.show_about_dialog = is_open && !close_clicked;
}

fn chrono_time() -> String {
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let hours = (now / 3600) % 24;
    let mins = (now / 60) % 60;
    let secs = now % 60;
    format!("{:02}:{:02}:{:02}", hours, mins, secs)
}
