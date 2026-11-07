use eframe::egui::{self, Color32, RichText, Ui};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;
use tokio::sync::mpsc;

use crate::gui::backend::{GuiCommand, GuiEvent};
use crate::gui::views::log_view::{render_log_view, LogEntry};
use crate::gui::views::lvar_view::{render_lvar_view, LvarViewState};
use crate::gui::views::mapping_view::{render_mapping_view, MappingViewState};
use crate::gui::views::proton_view::{render_proton_view, ProtonViewState};
use crate::gui::views::virtual_panel_view::{render_virtual_panel_view, VirtualPanelViewState};
use crate::profile::schema::SpadProfile;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum ActiveTab {
    Mapping,
    Lvars,
    VirtualPanels,
    ProtonSetup,
    Logs,
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
    simconnect_msg: String,

    bridge_active: bool,
    bridge_lvar_count: usize,
    aircraft_name: String,

    profile: Option<SpadProfile>,
    profile_path_input: String,
    profile_name: String,

    // Real-time telemetry
    last_input: Option<(String, String, String, Instant)>,
    discovered_lvars: Vec<String>,
    live_lvar_values: HashMap<String, f64>,
    logs: Vec<LogEntry>,
    auto_scroll_logs: bool,

    // View states
    mapping_state: MappingViewState,
    lvar_state: LvarViewState,
    virtual_state: VirtualPanelViewState,
    proton_state: ProtonViewState,
}

impl SpadLinxApp {
    pub fn new(
        _cc: &eframe::CreationContext<'_>,
        cmd_tx: mpsc::UnboundedSender<GuiCommand>,
        event_rx: mpsc::UnboundedReceiver<GuiEvent>,
        initial_profile: Option<PathBuf>,
        host: String,
        port: u16,
    ) -> Self {
        let initial_path_str = initial_profile
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "default_c172.xml".to_string());

        let mut app = Self {
            cmd_tx,
            event_rx,
            active_tab: ActiveTab::Mapping,
            simconnect_host: host.clone(),
            simconnect_port: port,
            simconnect_connected: false,
            simconnect_msg: "Not connected".to_string(),
            bridge_active: false,
            bridge_lvar_count: 0,
            aircraft_name: "No Aircraft Detected".to_string(),
            profile: None,
            profile_path_input: initial_path_str,
            profile_name: "No Profile Loaded".to_string(),
            last_input: None,
            discovered_lvars: Vec::new(),
            live_lvar_values: HashMap::new(),
            logs: Vec::new(),
            auto_scroll_logs: true,
            mapping_state: MappingViewState::default(),
            lvar_state: LvarViewState::default(),
            virtual_state: VirtualPanelViewState::default(),
            proton_state: ProtonViewState::default(),
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
                }
                GuiEvent::InputTriggered { device, bound_to, action, time } => {
                    self.last_input = Some((device.clone(), bound_to.clone(), action.clone(), time));
                }
                GuiEvent::LvarsDiscovered(lvars) => {
                    self.discovered_lvars = lvars;
                    self.bridge_lvar_count = self.discovered_lvars.len();
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
                    self.logs.push(LogEntry {
                        timestamp: chrono_time(),
                        level: "INFO".to_string(),
                        text: format!("Loaded profile '{}' from {}", name, path),
                    });
                }
                GuiEvent::ProfileSaved { path } => {
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

        // Top Status & Navigation Bar
        egui::TopBottomPanel::top("top_header_panel").show(ctx, |ui| {
            ui.add_space(4.0);
            render_top_bar(self, ui);
            ui.add_space(4.0);
            ui.separator();
            ui.add_space(4.0);
            render_nav_tabs(self, ui);
            ui.add_space(4.0);
        });

        // Bottom Status Bar
        egui::TopBottomPanel::bottom("bottom_status_panel").show(ctx, |ui| {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("SPAD-LINX v0.1.0").color(Color32::from_rgb(140, 140, 180)).small());
                ui.separator();
                ui.label(RichText::new(format!("Aircraft: {}", self.aircraft_name)).color(Color32::from_rgb(0, 220, 255)).small());
                ui.separator();
                ui.label(RichText::new(format!("LVARs: {}", self.bridge_lvar_count)).small());
                ui.separator();

                if let Some((_, bound, act, _)) = &self.last_input {
                    ui.label(RichText::new(format!("Latest Event: {} [{}]", bound, act)).color(Color32::from_rgb(0, 255, 120)).small());
                } else {
                    ui.label(RichText::new("Awaiting Input").color(Color32::GRAY).small());
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let proton_color = if crate::proton::ProtonConfigurator::find_msfs_roaming_path(None).is_some() {
                        Color32::from_rgb(0, 255, 120)
                    } else {
                        Color32::GRAY
                    };
                    ui.label(RichText::new("● Steam Proton MSFS Ready").color(proton_color).small());
                });
            });
            ui.add_space(2.0);
        });

        // Main Central Workspace
        egui::CentralPanel::default().show(ctx, |ui| {
            match self.active_tab {
                ActiveTab::Mapping => {
                    render_mapping_view(
                        ui,
                        &mut self.mapping_state,
                        &mut self.profile,
                        &self.cmd_tx,
                        &self.last_input,
                        &self.discovered_lvars,
                    );
                }
                ActiveTab::Lvars => {
                    render_lvar_view(
                        ui,
                        &mut self.lvar_state,
                        &self.cmd_tx,
                        &self.discovered_lvars,
                        &self.live_lvar_values,
                        self.bridge_active,
                    );
                }
                ActiveTab::VirtualPanels => {
                    render_virtual_panel_view(
                        ui,
                        &mut self.virtual_state,
                        &self.cmd_tx,
                    );
                }
                ActiveTab::ProtonSetup => {
                    render_proton_view(
                        ui,
                        &mut self.proton_state,
                        &self.cmd_tx,
                    );
                }
                ActiveTab::Logs => {
                    render_log_view(ui, &self.logs, &mut self.auto_scroll_logs);
                }
            }
        });

        // Request continuous repaint for smooth animations and live input monitoring
        ctx.request_repaint_after(std::time::Duration::from_millis(50));
    }
}

fn render_top_bar(app: &mut SpadLinxApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        // App title
        ui.heading(RichText::new("SPAD-LINX").color(Color32::from_rgb(0, 230, 255)).strong());
        ui.label(RichText::new("Flight Controller & Profile Engine").color(Color32::GRAY));

        ui.separator();

        // SimConnect connection pill
        let sc_color = if app.simconnect_connected {
            Color32::from_rgb(0, 255, 120)
        } else {
            Color32::from_rgb(255, 70, 70)
        };
        ui.label(RichText::new(if app.simconnect_connected { "● SimConnect Online" } else { "○ SimConnect Offline" }).color(sc_color).strong());

        ui.label("Host:");
        ui.add(egui::TextEdit::singleline(&mut app.simconnect_host).desired_width(90.0));
        ui.label("Port:");
        let mut port_str = app.simconnect_port.to_string();
        if ui.add(egui::TextEdit::singleline(&mut port_str).desired_width(45.0)).changed() {
            if let Ok(p) = port_str.parse::<u16>() {
                app.simconnect_port = p;
            }
        }

        if app.simconnect_connected {
            if ui.button(RichText::new("Disconnect").color(Color32::LIGHT_RED)).clicked() {
                let _ = app.cmd_tx.send(GuiCommand::DisconnectSimConnect);
            }
        } else {
            if ui.button(RichText::new("Connect").color(Color32::from_rgb(0, 255, 120)).strong()).clicked() {
                let _ = app.cmd_tx.send(GuiCommand::ConnectSimConnect {
                    host: app.simconnect_host.clone(),
                    port: app.simconnect_port,
                });
            }
        }

        ui.separator();

        // Profile quick load / save
        ui.label(RichText::new("Profile:").strong());
        ui.add(egui::TextEdit::singleline(&mut app.profile_path_input).desired_width(160.0));

        if ui.button("Load").clicked() {
            let path = PathBuf::from(&app.profile_path_input);
            let _ = app.cmd_tx.send(GuiCommand::LoadProfile(path));
        }

        if ui.button("Save").clicked() {
            let path = PathBuf::from(&app.profile_path_input);
            let _ = app.cmd_tx.send(GuiCommand::SaveProfile(path));
        }
    });
}

fn render_nav_tabs(app: &mut SpadLinxApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.selectable_value(&mut app.active_tab, ActiveTab::Mapping, "🎮 Controller Mapping & Inputs");
        ui.selectable_value(&mut app.active_tab, ActiveTab::Lvars, "🔍 LVAR Explorer & MSFS Bridge");
        ui.selectable_value(&mut app.active_tab, ActiveTab::VirtualPanels, "✈ Interactive Virtual Panels");
        ui.selectable_value(&mut app.active_tab, ActiveTab::ProtonSetup, "⚙ Steam Proton Setup");
        ui.selectable_value(&mut app.active_tab, ActiveTab::Logs, "📋 Event Log");
    });
}

fn chrono_time() -> String {
    use std::time::SystemTime;
    let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs();
    let hours = (now / 3600) % 24;
    let mins = (now / 60) % 60;
    let secs = now % 60;
    format!("{:02}:{:02}:{:02}", hours, mins, secs)
}
