pub mod presets;
pub mod sim_events;
pub mod backend;
pub mod views;
pub mod app;
pub mod theme;
pub mod widgets;

use std::path::PathBuf;
use anyhow::Result;
use app::SpadLinxApp;
use backend::start_backend;
use theme::AeroTheme;

const APP_ICON_PNG: &[u8] = include_bytes!("../../assets/icons/icon_256x256.png");

fn load_app_icon() -> Option<eframe::egui::IconData> {
    eframe::icon_data::from_png_bytes(APP_ICON_PNG).ok()
}

pub fn run_gui(profile_path: Option<PathBuf>, simconnect_host: String, simconnect_port: u16) -> Result<()> {
    let (cmd_tx, event_rx) = start_backend(profile_path.clone(), simconnect_host.clone(), simconnect_port);

    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_inner_size([1480.0, 940.0])
        .with_min_inner_size([1024.0, 680.0])
        .with_title("AeroDeck - Flight Controller & Telemetry Suite")
        .with_app_id("aerodeck");

    if let Some(icon) = load_app_icon() {
        viewport = viewport.with_icon(icon);
    }

    let native_options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "AeroDeck",
        native_options,
        Box::new(move |cc| {
            // Apply AeroDeck glass-cockpit theme
            let theme = AeroTheme::dark();
            crate::gui::theme::install_fonts(&cc.egui_ctx);
            theme.apply(&cc.egui_ctx);

            Ok(Box::new(SpadLinxApp::new(
                cc,
                cmd_tx,
                event_rx,
                profile_path,
                simconnect_host,
                simconnect_port,
            )))
        }),
    )
    .map_err(|e| anyhow::anyhow!("GUI error: {}", e))
}
