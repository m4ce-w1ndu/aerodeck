use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::info;

pub const MSFS_2020_STEAM_APP_ID: &str = "1250410";
pub const DEFAULT_SIMCONNECT_PORT: u16 = 45454;

pub struct ProtonConfigurator;

impl ProtonConfigurator {
    /// Discovers possible MSFS Steam Proton prefixes on Linux
    pub fn find_msfs_proton_prefix() -> Option<PathBuf> {
        let home = std::env::var("HOME").ok()?;
        let candidates = [
            format!("{}/.local/share/Steam/steamapps/compatdata/{}/pfx", home, MSFS_2020_STEAM_APP_ID),
            format!("{}/.steam/steam/steamapps/compatdata/{}/pfx", home, MSFS_2020_STEAM_APP_ID),
            format!("{}/.var/app/com.valvesoftware.Steam/.local/share/Steam/steamapps/compatdata/{}/pfx", home, MSFS_2020_STEAM_APP_ID),
        ];

        for c in candidates {
            let p = PathBuf::from(c);
            if p.exists() {
                return Some(p);
            }
        }
        None
    }

    /// Finds the MSFS AppData Roaming directory inside the Proton prefix
    pub fn find_appdata_dir(prefix: &Path) -> Option<PathBuf> {
        let path = prefix
            .join("drive_c")
            .join("users")
            .join("steamuser")
            .join("AppData")
            .join("Roaming")
            .join("Microsoft Flight Simulator");
        if path.exists() {
            Some(path)
        } else {
            None
        }
    }

    /// Generates the standard SimConnect.xml enabling IPv4 network TCP connections from native Linux
    pub fn generate_simconnect_xml(port: u16) -> String {
        format!(
            r#"<?xml version="1.0" encoding="Windows-1252"?>
<SimBase.Document Type="SimConnect" version="1,0">
  <Descr>SimConnect Server Configuration for Linux Proton Interop</Descr>
  <Filename>SimConnect.xml</Filename>
  <Disabled>False</Disabled>

  <!-- Local Named Pipe (Used by in-proc WASM Bridge and Proton internal tools) -->
  <SimConnect.Comm>
    <Disabled>False</Disabled>
    <Protocol>Pipe</Protocol>
    <Scope>local</Scope>
  </SimConnect.Comm>

  <!-- IPv4 Network TCP Port (Allows native Linux AeroDeck to communicate with zero latency) -->
  <SimConnect.Comm>
    <Disabled>False</Disabled>
    <Protocol>IPv4</Protocol>
    <Scope>local</Scope>
    <Address>127.0.0.1</Address>
    <Port>{}</Port>
    <MaxClients>64</MaxClients>
    <MaxRecvSize>65536</MaxRecvSize>
    <DisableNagle>True</DisableNagle>
  </SimConnect.Comm>
</SimBase.Document>
"#,
            port
        )
    }

    /// Installs or verifies SimConnect.xml in MSFS AppData directory
    pub fn install_simconnect_xml<P: AsRef<Path>>(target_dir: P, port: u16) -> Result<PathBuf> {
        let dest = target_dir.as_ref().join("SimConnect.xml");
        let xml = Self::generate_simconnect_xml(port);
        fs::write(&dest, xml)
            .with_context(|| format!("Failed to write SimConnect.xml to {:?}", dest))?;
        info!("Installed network SimConnect.xml configured on port {} to {:?}", port, dest);
        Ok(dest)
    }

    /// Finds MSFS AppData Roaming path, checking custom prefix or auto-detected Proton prefixes
    pub fn find_msfs_roaming_path(custom_prefix: Option<PathBuf>) -> Option<PathBuf> {
        if let Some(pfx) = custom_prefix {
            Self::find_appdata_dir(&pfx)
        } else {
            let pfx = Self::find_msfs_proton_prefix()?;
            Self::find_appdata_dir(&pfx)
        }
    }

    /// Configures SimConnect.xml directly
    pub fn configure_simconnect(custom_prefix: Option<PathBuf>, port: u16) -> Result<PathBuf> {
        let roaming = Self::find_msfs_roaming_path(custom_prefix)
            .ok_or_else(|| anyhow::anyhow!("MSFS AppData Roaming directory not found in Proton prefix"))?;
        Self::install_simconnect_xml(&roaming, port)
    }
}
