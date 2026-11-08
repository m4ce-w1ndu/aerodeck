pub mod presets;
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

pub fn run_gui(profile_path: Option<PathBuf>, simconnect_host: String, simconnect_port: u16) -> Result<()> {
    let (cmd_tx, event_rx) = start_backend(profile_path.clone(), simconnect_host.clone(), simconnect_port);

    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 840.0])
            .with_min_inner_size([960.0, 600.0])
            .with_title("AeroDeck - Flight Controller & Telemetry Suite"),
        ..Default::default()
    };

    eframe::run_native(
        "AeroDeck",
        native_options,
        Box::new(move |cc| {
            // Apply AeroDeck glass-cockpit theme
            let theme = AeroTheme::dark();
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
