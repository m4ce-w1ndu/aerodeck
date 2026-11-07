use eframe::egui::{Color32, RichText, Ui};
use std::path::PathBuf;

use crate::gui::backend::GuiCommand;
use crate::proton::ProtonConfigurator;

pub struct ProtonViewState {
    pub custom_prefix: String,
    pub port: u16,
    pub status_message: Option<String>,
}

impl Default for ProtonViewState {
    fn default() -> Self {
        Self {
            custom_prefix: String::new(),
            port: 45454,
            status_message: None,
        }
    }
}

pub fn render_proton_view(
    ui: &mut Ui,
    state: &mut ProtonViewState,
    _cmd_tx: &tokio::sync::mpsc::UnboundedSender<GuiCommand>,
) {
    ui.heading(RichText::new("Steam Proton MSFS 2020 Auto-Detector & Network Setup").color(Color32::from_rgb(180, 140, 255)));
    ui.add_space(4.0);

    ui.label("Under Linux Steam Proton, Named Pipes cannot be shared directly with native Linux programs.");
    ui.label("spad-linx automatically detects your MSFS 2020 Proton prefix and provisions a TCP SimConnect.xml listener, letting native Linux programs communicate with MSFS with zero latency.");
    ui.add_space(8.0);

    let detected_path = ProtonConfigurator::find_msfs_roaming_path(None);

    ui.group(|ui| {
        ui.label(RichText::new("Auto-Detection Status:").strong());

        if let Some(ref path) = detected_path {
            ui.label(RichText::new(format!("✔ Found MSFS Proton Roaming Directory:")).color(Color32::from_rgb(0, 255, 120)));
            ui.label(RichText::new(path.to_string_lossy()).monospace());
        } else {
            ui.label(RichText::new("✖ MSFS 2020 Proton prefix not detected in standard Steam library locations.").color(Color32::LIGHT_RED));
        }

        ui.add_space(6.0);

        ui.horizontal(|ui| {
            ui.label("Custom Prefix Path (optional):");
            ui.text_edit_singleline(&mut state.custom_prefix);
        });

        ui.horizontal(|ui| {
            ui.label("SimConnect TCP Port:");
            let mut port_str = state.port.to_string();
            if ui.text_edit_singleline(&mut port_str).changed() {
                if let Ok(p) = port_str.parse::<u16>() {
                    state.port = p;
                }
            }
        });

        ui.add_space(8.0);

        if ui.button(RichText::new("🚀 One-Click: Configure SimConnect.xml in MSFS Proton Prefix").color(Color32::from_rgb(0, 255, 120)).strong()).clicked() {
            let custom_opt = if state.custom_prefix.trim().is_empty() {
                None
            } else {
                Some(PathBuf::from(state.custom_prefix.trim()))
            };

            match ProtonConfigurator::configure_simconnect(custom_opt, state.port) {
                Ok(path) => {
                    state.status_message = Some(format!("Success! Configured SimConnect.xml at {:?}", path));
                }
                Err(e) => {
                    state.status_message = Some(format!("Error: {}", e));
                }
            }
        }

        if let Some(ref msg) = state.status_message {
            ui.add_space(6.0);
            let color = if msg.starts_with("Success") { Color32::from_rgb(0, 255, 120) } else { Color32::LIGHT_RED };
            ui.label(RichText::new(msg).color(color).strong());
        }
    });
}
