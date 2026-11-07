pub mod presets;
pub mod backend;
pub mod views;
pub mod app;

use std::path::PathBuf;
use anyhow::Result;
use app::SpadLinxApp;
use backend::start_backend;

pub fn run_gui(profile_path: Option<PathBuf>, simconnect_host: String, simconnect_port: u16) -> Result<()> {
    let (cmd_tx, event_rx) = start_backend(profile_path.clone(), simconnect_host.clone(), simconnect_port);

    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 840.0])
            .with_min_inner_size([960.0, 600.0])
            .with_title("SPAD-LINX - Flight Controller & Profile Mapper"),
        ..Default::default()
    };

    eframe::run_native(
        "SPAD-LINX",
        native_options,
        Box::new(move |cc| {
            // Configure aviation dark cockpit theme
            let mut visuals = eframe::egui::Visuals::dark();
            visuals.window_rounding = 6.0.into();
            visuals.panel_fill = eframe::egui::Color32::from_rgb(20, 24, 30);
            visuals.extreme_bg_color = eframe::egui::Color32::from_rgb(14, 16, 20);
            cc.egui_ctx.set_visuals(visuals);

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
