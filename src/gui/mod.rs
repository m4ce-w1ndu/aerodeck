//! Desktop interface: a Slint UI (`ui/*.slint`) driven by `ui::App`, talking to the async
//! backend over channels.

pub mod backend;
mod desktop;
pub mod editing;
pub mod presets;
pub mod sim_events;
mod ui;

use anyhow::Result;
use std::path::PathBuf;

pub fn run_gui(profile_path: Option<PathBuf>, simconnect_host: String, simconnect_port: u16) -> Result<()> {
    desktop::ensure_installed();
    ui::run(profile_path, simconnect_host, simconnect_port)
}
