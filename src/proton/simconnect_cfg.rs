use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::info;

pub const MSFS_2020_STEAM_APP_ID: &str = "1250410";
pub const MSFS_2024_STEAM_APP_ID: &str = "2537590";
pub const DEFAULT_SIMCONNECT_PORT: u16 = 45454;

pub struct ProtonConfigurator;

impl ProtonConfigurator {
    /// Discovers all Steam library folders from Steam metadata and libraryfolders.vdf
    pub fn discover_steam_libraries() -> Vec<PathBuf> {
        let mut libraries = Vec::new();
        let home = match std::env::var("HOME") {
            Ok(h) => h,
            Err(_) => return libraries,
        };

        // Standard Steam base directories
        let steam_roots = [
            format!("{}/.local/share/Steam", home),
            format!("{}/.steam/steam", home),
            format!("{}/.steam/root", home),
            format!("{}/.var/app/com.valvesoftware.Steam/.local/share/Steam", home),
        ];

        for root in &steam_roots {
            let root_path = PathBuf::from(root);
            if root_path.exists() && !libraries.contains(&root_path) {
                libraries.push(root_path.clone());
            }

            // Check libraryfolders.vdf in steamapps and config
            let vdf_candidates = [
                root_path.join("steamapps").join("libraryfolders.vdf"),
                root_path.join("config").join("libraryfolders.vdf"),
            ];

            for vdf_path in &vdf_candidates {
                if let Ok(content) = fs::read_to_string(vdf_path) {
                    Self::parse_libraryfolders_vdf(&content, &mut libraries);
                }
            }
        }

        libraries
    }

    /// Extracts "path" directory values from Valve libraryfolders.vdf format
    fn parse_libraryfolders_vdf(content: &str, libraries: &mut Vec<PathBuf>) {
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("\"path\"") {
                let parts: Vec<&str> = trimmed.split('"').collect();
                if parts.len() >= 4 {
                    let dir = PathBuf::from(parts[3]);
                    if dir.exists() && !libraries.contains(&dir) {
                        libraries.push(dir);
                    }
                }
            }
        }
    }

    /// Discovers MSFS Steam Proton prefixes on Linux across all internal and external library drives
    pub fn find_msfs_proton_prefix() -> Option<PathBuf> {
        let libraries = Self::discover_steam_libraries();
        let app_ids = [MSFS_2020_STEAM_APP_ID, MSFS_2024_STEAM_APP_ID];

        for lib in &libraries {
            for app_id in &app_ids {
                // Check <library>/steamapps/compatdata/<app_id>/pfx
                let candidate = lib.join("steamapps").join("compatdata").join(app_id).join("pfx");
                if candidate.exists() {
                    info!("Discovered MSFS Proton prefix via Steam metadata: {:?}", candidate);
                    return Some(candidate);
                }
                // Check <library>/compatdata/<app_id>/pfx
                let candidate2 = lib.join("compatdata").join(app_id).join("pfx");
                if candidate2.exists() {
                    info!("Discovered MSFS Proton prefix: {:?}", candidate2);
                    return Some(candidate2);
                }
            }
        }

        None
    }

    /// Finds the MSFS AppData Roaming directory inside the Proton prefix (resilient to any user name)
    pub fn find_appdata_dir(prefix: &Path) -> Option<PathBuf> {
        if !prefix.exists() {
            return None;
        }

        // Case 1: Directly points to 'Microsoft Flight Simulator' roaming folder
        if prefix.file_name().and_then(|f| f.to_str()) == Some("Microsoft Flight Simulator") {
            return Some(prefix.to_path_buf());
        }

        // Case 2: Directly points to Roaming folder
        let direct_sub = prefix.join("Microsoft Flight Simulator");
        if direct_sub.exists() {
            return Some(direct_sub);
        }

        // Case 3: Points to compatdata/<app_id> (without /pfx)
        let via_pfx = prefix.join("pfx");
        if via_pfx.exists() {
            if let Some(res) = Self::find_appdata_dir(&via_pfx) {
                return Some(res);
            }
        }

        // Case 4: Standard prefix root or drive_c root: look for drive_c/users/*/AppData/Roaming/Microsoft Flight Simulator
        let drive_c = if prefix.file_name().and_then(|f| f.to_str()) == Some("drive_c") {
            prefix.to_path_buf()
        } else {
            prefix.join("drive_c")
        };

        if drive_c.exists() {
            let users_dir = drive_c.join("users");
            if let Ok(entries) = fs::read_dir(&users_dir) {
                for entry in entries.flatten() {
                    let user_path = entry.path();
                    if user_path.is_dir() {
                        let msfs_roaming = user_path
                            .join("AppData")
                            .join("Roaming")
                            .join("Microsoft Flight Simulator");
                        if msfs_roaming.exists() {
                            return Some(msfs_roaming);
                        }
                    }
                }
            }
        }

        None
    }

    /// Reads all configured static IPv4 ports (Port > 0) from an existing SimConnect.xml
    pub fn read_configured_ipv4_ports(roaming_dir: &Path) -> Vec<(String, u16)> {
        let xml_path = roaming_dir.join("SimConnect.xml");
        let content = match fs::read_to_string(&xml_path) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        let mut results = Vec::new();
        for block in content.split("<SimConnect.Comm>") {
            let block_end = block.split("</SimConnect.Comm>").next().unwrap_or("");
            let is_ipv4 = block_end.to_lowercase().contains("<protocol>ipv4</protocol>");
            if !is_ipv4 {
                continue;
            }

            let scope = Self::extract_xml_tag(block_end, "Scope").unwrap_or_else(|| "local".to_string());
            if let Some(port_str) = Self::extract_xml_tag(block_end, "Port") {
                if let Ok(port) = port_str.trim().parse::<u16>() {
                    if port > 0 && !results.iter().any(|(_, p)| *p == port) {
                        results.push((scope, port));
                    }
                }
            }
        }

        results
    }

    fn extract_xml_tag(text: &str, tag: &str) -> Option<String> {
        let open = format!("<{}>", tag);
        let close = format!("</{}>", tag);
        let lower = text.to_lowercase();
        let start = lower.find(&open.to_lowercase())? + open.len();
        let end = lower[start..].find(&close.to_lowercase())? + start;
        Some(text[start..end].trim().to_string())
    }

    /// Returns the minimum unprivileged TCP port allowed by the Linux kernel (typically 1024)
    pub fn get_linux_unprivileged_port_start() -> u16 {
        if let Ok(s) = fs::read_to_string("/proc/sys/net/ipv4/ip_unprivileged_port_start") {
            if let Ok(val) = s.trim().parse::<u16>() {
                return val;
            }
        }
        0
    }

    /// Generates a complete MSFS-compatible SimConnect.xml preserving standard Pipes, IPv6, and Dynamic ports
    pub fn generate_simconnect_xml(port: u16) -> String {
        Self::generate_simconnect_xml_with_scope(port, "local")
    }

    /// Generates a complete MSFS-compatible SimConnect.xml with configurable Scope ("local" or "global")
    pub fn generate_simconnect_xml_with_scope(port: u16, scope: &str) -> String {
        let address = if scope.eq_ignore_ascii_case("global") {
            "0.0.0.0"
        } else {
            "127.0.0.1"
        };

        let extra_port_500 = if port != 500 {
            r#"
    <SimConnect.Comm>
        <Descr>Static IP4 port (Standard 500)</Descr>
        <Protocol>IPv4</Protocol>
        <Scope>local</Scope>
        <Port>500</Port>
        <MaxClients>64</MaxClients>
        <MaxRecvSize>41088</MaxRecvSize>
    </SimConnect.Comm>"#
        } else {
            ""
        };

        format!(
            r#"<?xml version="1.0" encoding="Windows-1252"?>

<SimBase.Document Type="SimConnect" version="1,0">
    <Descr>SimConnect Server Configuration</Descr>
    <Filename>SimConnect.xml</Filename>
    <!-- Primary IPv4 TCP Listener (AeroDeck / External Clients) -->
    <SimConnect.Comm>
        <Descr>Static IP4 port</Descr>
        <Disabled>False</Disabled>
        <Protocol>IPv4</Protocol>
        <Scope>{scope}</Scope>
        <Address>{address}</Address>
        <Port>{port}</Port>
        <MaxClients>64</MaxClients>
        <MaxRecvSize>65536</MaxRecvSize>
        <DisableNagle>True</DisableNagle>
    </SimConnect.Comm>{extra_port_500}
    <SimConnect.Comm>
        <Descr>Static IP6 port</Descr>
        <Protocol>IPv6</Protocol>
        <Scope>local</Scope>
        <Port>501</Port>
        <MaxClients>64</MaxClients>
        <MaxRecvSize>41088</MaxRecvSize>
    </SimConnect.Comm>
    <SimConnect.Comm>
        <Descr>Custom Pipe</Descr>
        <Protocol>Pipe</Protocol>
        <Scope>local</Scope>
        <Port>Custom\SimConnect</Port>
        <MaxClients>64</MaxClients>
        <MaxRecvSize>41088</MaxRecvSize>
    </SimConnect.Comm>
    <SimConnect.Comm>
        <Descr>Dynamic IP4 port</Descr>
        <Protocol>IPv4</Protocol>
        <Scope>local</Scope>
        <Port>0</Port>
        <MaxClients>64</MaxClients>
        <MaxRecvSize>41088</MaxRecvSize>
    </SimConnect.Comm>
    <SimConnect.Comm>
        <Descr>Dynamic IP6 port</Descr>
        <Protocol>IPv6</Protocol>
        <Scope>local</Scope>
        <Port>0</Port>
        <MaxClients>64</MaxClients>
        <MaxRecvSize>41088</MaxRecvSize>
    </SimConnect.Comm>
    <SimConnect.Comm>
        <Descr>Default FSX Pipe</Descr>
        <Protocol>Pipe</Protocol>
        <Scope>local</Scope>
        <Port>0</Port>
        <MaxClients>64</MaxClients>
        <MaxRecvSize>41088</MaxRecvSize>
    </SimConnect.Comm>
</SimBase.Document>
"#
        )
    }

    /// Generates a client SimConnect.cfg snippet matching the server configuration
    pub fn generate_simconnect_cfg(host: &str, port: u16) -> String {
        format!(
            r#"[SimConnect]
Protocol=IPv4
Address={}
Port={}
MaxReceiveSize=65536
DisableNagle=1
"#,
            host, port
        )
    }

    /// Installs or verifies SimConnect.xml in MSFS AppData directory
    pub fn install_simconnect_xml<P: AsRef<Path>>(target_dir: P, port: u16) -> Result<PathBuf> {
        Self::install_simconnect_xml_with_scope(target_dir, port, "local")
    }

    /// Installs SimConnect.xml with explicit Scope ("local" or "global")
    pub fn install_simconnect_xml_with_scope<P: AsRef<Path>>(
        target_dir: P,
        port: u16,
        scope: &str,
    ) -> Result<PathBuf> {
        let dest = target_dir.as_ref().join("SimConnect.xml");
        let xml = Self::generate_simconnect_xml_with_scope(port, scope);
        fs::write(&dest, xml)
            .with_context(|| format!("Failed to write SimConnect.xml to {:?}", dest))?;
        info!(
            "Installed network SimConnect.xml configured on port {} (scope: {}) to {:?}",
            port, scope, dest
        );
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
        Self::configure_simconnect_with_scope(custom_prefix, port, "local")
    }

    /// Configures SimConnect.xml with explicit Scope ("local" or "global")
    pub fn configure_simconnect_with_scope(
        custom_prefix: Option<PathBuf>,
        port: u16,
        scope: &str,
    ) -> Result<PathBuf> {
        let roaming = Self::find_msfs_roaming_path(custom_prefix)
            .ok_or_else(|| anyhow::anyhow!("MSFS AppData Roaming directory not found in Proton prefix"))?;
        Self::install_simconnect_xml_with_scope(&roaming, port, scope)
    }
}
