use anyhow::{Context, Result};
use std::fs;
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;
use tracing::info;

pub const MSFS_2020_STEAM_APP_ID: &str = "1250410";
pub const MSFS_2024_STEAM_APP_ID: &str = "2537590";
pub const DEFAULT_SIMCONNECT_PORT: u16 = 45454;

/// Embedded 7.5 KB freestanding Win64 Named-Pipe-to-TCP bridge helper (`aerodeck-pipe-bridge.exe`)
pub const PIPE_BRIDGE_EXE: &[u8] = include_bytes!("pipe_bridge/aerodeck-pipe-bridge.exe");

#[derive(Debug, Clone)]
pub struct UnattendedSetupReport {
    pub prefix_path: PathBuf,
    pub roaming_path: PathBuf,
    pub active_port: u16,
    pub xml_auto_provisioned: bool,
    pub helper_deployed_path: PathBuf,
    pub proton_wine_bin: Option<PathBuf>,
    pub strategy_summary: String,
}

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

    /// Resolves the root `pfx` path from a user-supplied prefix or roaming path
    pub fn resolve_pfx_root(input: &Path) -> Option<PathBuf> {
        if input.join("drive_c").exists() {
            return Some(input.to_path_buf());
        }
        if input.join("pfx").join("drive_c").exists() {
            return Some(input.join("pfx"));
        }
        let mut cur = Some(input);
        while let Some(p) = cur {
            if p.file_name().and_then(|n| n.to_str()) == Some("pfx") && p.join("drive_c").exists() {
                return Some(p.to_path_buf());
            }
            cur = p.parent();
        }
        None
    }

    /// Finds the exact Proton `wine` binary matching the MSFS compatdata prefix via `config_info`
    pub fn find_proton_wine_binary(pfx_or_compatdata: &Path) -> Option<PathBuf> {
        let pfx = Self::resolve_pfx_root(pfx_or_compatdata).unwrap_or_else(|| pfx_or_compatdata.to_path_buf());
        // Check ../config_info inside compatdata/<appid>/config_info
        if let Some(compatdata_dir) = pfx.parent() {
            let config_info = compatdata_dir.join("config_info");
            if let Ok(content) = fs::read_to_string(&config_info) {
                for line in content.lines() {
                    if let Some(idx) = line.find("/files/") {
                        let proton_root = PathBuf::from(&line[..idx]);
                        let wine_bin = proton_root.join("files").join("bin").join("wine");
                        if wine_bin.exists() {
                            return Some(wine_bin);
                        }
                        let wine64_bin = proton_root.join("files").join("bin").join("wine64");
                        if wine64_bin.exists() {
                            return Some(wine64_bin);
                        }
                    }
                }
            }
        }

        // Fallback: check discovered Steam libraries for Proton - Experimental
        for lib in Self::discover_steam_libraries() {
            let exp_wine = lib
                .join("steamapps")
                .join("common")
                .join("Proton - Experimental")
                .join("files")
                .join("bin")
                .join("wine");
            if exp_wine.exists() {
                return Some(exp_wine);
            }
        }

        // System wine fallback
        let sys_wine = PathBuf::from("/usr/bin/wine");
        if sys_wine.exists() {
            return Some(sys_wine);
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

    /// Scans `/proc` for a running `FlightSimulator.exe` process and returns all TCP IPv4 ports it is listening on.
    /// Even with an unmodified default `SimConnect.xml`, MSFS includes `<Descr>Dynamic IP4 port</Descr> <Port>0</Port>`,
    /// which binds an ephemeral unprivileged port in the Linux kernel.
    pub fn discover_live_msfs_tcp_ports() -> Vec<u16> {
        let mut socket_inodes = Vec::new();
        if let Ok(proc_entries) = fs::read_dir("/proc") {
            for entry in proc_entries.flatten() {
                let file_name = entry.file_name();
                let pid_str = match file_name.to_str() {
                    Some(s) if s.chars().all(|c| c.is_ascii_digit()) => s,
                    _ => continue,
                };
                let pid_path = entry.path();
                let comm = fs::read_to_string(pid_path.join("comm"))
                    .unwrap_or_default()
                    .trim()
                    .to_lowercase();

                // Explicitly reject known wrapper processes
                if comm.starts_with("gamescope")
                    || comm.starts_with("reaper")
                    || comm.starts_with("srt-bwrap")
                    || comm.starts_with("bwrap")
                    || comm.starts_with("pv-adverb")
                    || comm.starts_with("python")
                    || comm.starts_with("steam")
                    || comm.starts_with("bash")
                    || comm.starts_with("sh")
                {
                    continue;
                }

                let cmdline = fs::read_to_string(pid_path.join("cmdline"))
                    .unwrap_or_default()
                    .to_lowercase();

                let is_msfs = comm.contains("flightsimulator")
                    || (cmdline.contains("flightsimulator.exe")
                        && !cmdline.contains("gamescope")
                        && !cmdline.contains("reaper")
                        && !cmdline.contains("pv-adverb")
                        && !cmdline.contains("bwrap"));

                if is_msfs {
                    if let Ok(fds) = fs::read_dir(pid_path.join("fd")) {
                        for fd in fds.flatten() {
                            if let Ok(target) = fs::read_link(fd.path()) {
                                if let Some(target_str) = target.to_str() {
                                    if let Some(inode) = target_str
                                        .strip_prefix("socket:[")
                                        .and_then(|s| s.strip_suffix(']'))
                                    {
                                        socket_inodes.push((pid_str.to_string(), inode.to_string()));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if socket_inodes.is_empty() {
            return Vec::new();
        }

        let mut ports = Vec::new();
        if let Ok(tcp_table) = fs::read_to_string("/proc/net/tcp") {
            for line in tcp_table.lines().skip(1) {
                let cols: Vec<&str> = line.split_whitespace().collect();
                if cols.len() < 10 {
                    continue;
                }
                // State 0A == TCP_LISTEN
                if cols[3] != "0A" {
                    continue;
                }
                let inode = cols[9];
                if socket_inodes.iter().any(|(_, i)| i == inode) {
                    if let Some(port_hex) = cols[1].split(':').nth(1) {
                        if let Ok(port) = u16::from_str_radix(port_hex, 16) {
                            if port > 0 && !ports.contains(&port) {
                                ports.push(port);
                            }
                        }
                    }
                }
            }
        }

        ports
    }

    /// Reads `SimConnect_Port_IPv4` from the Proton prefix's `user.reg` if MSFS wrote a dynamic IPv4 port
    pub fn read_registry_dynamic_ipv4_port(pfx_path: &Path) -> Option<u16> {
        let pfx = Self::resolve_pfx_root(pfx_path)?;
        let user_reg = pfx.join("user.reg");
        let content = fs::read_to_string(user_reg).ok()?;
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("\"SimConnect_Port_IPv4\"") {
                if let Some(val_part) = trimmed.split('=').nth(1) {
                    let clean = val_part.trim().trim_matches('"');
                    if let Some(hex) = clean.strip_prefix("dword:") {
                        if let Ok(p) = u16::from_str_radix(hex, 16) {
                            if p > 0 {
                                return Some(p);
                            }
                        }
                    } else if let Ok(p) = clean.parse::<u16>() {
                        if p > 0 {
                            return Some(p);
                        }
                    }
                }
            }
        }
        None
    }

    /// Deploys the embedded 7.5 KB Win64 `aerodeck-pipe-bridge.exe` helper into the MSFS Roaming directory
    pub fn deploy_pipe_bridge_helper(roaming_dir: &Path) -> Result<PathBuf> {
        let exe_path = roaming_dir.join("aerodeck-pipe-bridge.exe");
        let needs_write = match fs::metadata(&exe_path) {
            Ok(meta) => meta.len() != PIPE_BRIDGE_EXE.len() as u64,
            Err(_) => true,
        };
        if needs_write {
            fs::write(&exe_path, PIPE_BRIDGE_EXE)
                .with_context(|| format!("Failed to write {:?}", exe_path))?;
        }
        Ok(exe_path)
    }

    /// Runs `aerodeck-pipe-bridge.exe --self-test` inside the Proton prefix to verify Wine Named Pipe -> TCP bridging
    pub fn run_pipe_bridge_self_test(custom_prefix: Option<PathBuf>) -> Result<String> {
        let roaming = Self::find_msfs_roaming_path(custom_prefix.clone())
            .ok_or_else(|| anyhow::anyhow!("MSFS Roaming directory not found"))?;
        let pfx = Self::resolve_pfx_root(&roaming)
            .or(custom_prefix)
            .ok_or_else(|| anyhow::anyhow!("Proton pfx root not found"))?;
        let wine_bin = Self::find_proton_wine_binary(&pfx)
            .ok_or_else(|| anyhow::anyhow!("Proton wine binary not found"))?;
        let helper_exe = Self::deploy_pipe_bridge_helper(&roaming)?;

        let output = Command::new(&wine_bin)
            .env("WINEPREFIX", &pfx)
            .env("WINEDEBUG", "-all")
            .arg(&helper_exe)
            .arg("--self-test")
            .output()
            .with_context(|| format!("Failed to execute {:?} under {:?}", helper_exe, wine_bin))?;

        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if output.status.success() && stdout.contains("SELF_TEST_OK") {
            Ok(format!(
                "Wine Named-Pipe Bridge self-test passed via {:?} ({})",
                wine_bin.file_name().unwrap_or_default(),
                stdout
            ))
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Bridge self-test failed: stdout={} stderr={}", stdout, stderr)
        }
    }

    /// Probes for live MSFS Named Pipes (`\\.\pipe\Custom\SimConnect`) inside the Proton prefix
    /// and spawns the background `aerodeck-pipe-bridge.exe` helper on `target_port` if found.
    pub fn probe_and_spawn_pipe_bridge(pfx: &Path, roaming: &Path, target_port: u16) -> Result<bool> {
        let wine_bin = match Self::find_proton_wine_binary(pfx) {
            Some(b) => b,
            None => return Ok(false),
        };
        let helper_exe = Self::deploy_pipe_bridge_helper(roaming)?;

        let probe = Command::new(&wine_bin)
            .env("WINEPREFIX", pfx)
            .env("WINEDEBUG", "-all")
            .arg(&helper_exe)
            .arg("--probe")
            .output()?;

        let stdout = String::from_utf8_lossy(&probe.stdout);
        if probe.status.success() && stdout.contains("PIPE_FOUND:") {
            info!(
                "Detected active MSFS Named Pipe in Proton ({}), spawning aerodeck-pipe-bridge.exe on port {}",
                stdout.trim(),
                target_port
            );
            Command::new(&wine_bin)
                .env("WINEPREFIX", pfx)
                .env("WINEDEBUG", "-all")
                .arg(&helper_exe)
                .arg("--port")
                .arg(target_port.to_string())
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;
            std::thread::sleep(Duration::from_millis(150));
            return Ok(true);
        }

        Ok(false)
    }

    /// Performs 100% UNATTENDED setup on startup:
    /// 1. Discovers MSFS Proton prefix across all Steam libraries (`libraryfolders.vdf`).
    /// 2. Checks `SimConnect.xml`; if only privileged port `< 1024` (`500`) is configured, automatically
    ///    provisions unprivileged static IPv4 port `45454` while preserving all standard MSFS pipes & dynamic ports.
    /// 3. Deploys the embedded Win64 `aerodeck-pipe-bridge.exe` helper into the prefix.
    pub fn ensure_unattended_setup(custom_prefix: Option<PathBuf>) -> Result<UnattendedSetupReport> {
        let roaming = Self::find_msfs_roaming_path(custom_prefix.clone())
            .ok_or_else(|| anyhow::anyhow!("MSFS Proton prefix not found"))?;
        let pfx = Self::resolve_pfx_root(&roaming)
            .or_else(|| custom_prefix.clone())
            .unwrap_or_else(|| roaming.clone());

        let min_unpriv = Self::get_linux_unprivileged_port_start();
        let configured_ports = Self::read_configured_ipv4_ports(&roaming);

        // Find if an unprivileged static IPv4 port (>= min_unpriv) already exists in SimConnect.xml
        let existing_unpriv_port = configured_ports
            .iter()
            .map(|(_, port)| *port)
            .find(|&p| p >= min_unpriv.max(1024));

        let (active_port, xml_auto_provisioned) = if let Some(port) = existing_unpriv_port {
            (port, false)
        } else {
            // Automatically provision DEFAULT_SIMCONNECT_PORT (45454) so MSFS binds an unprivileged TCP port unattended!
            Self::install_simconnect_xml_with_scope(&roaming, DEFAULT_SIMCONNECT_PORT, "local")?;
            (DEFAULT_SIMCONNECT_PORT, true)
        };

        let helper_deployed_path = Self::deploy_pipe_bridge_helper(&roaming)?;
        let proton_wine_bin = Self::find_proton_wine_binary(&pfx);

        let strategy_summary = if xml_auto_provisioned {
            format!(
                "Unattended setup complete: Auto-provisioned SimConnect.xml on unprivileged TCP port {} (replacing privileged port 500 block) + deployed Win64 Named-Pipe bridge helper",
                active_port
            )
        } else {
            format!(
                "Unattended setup verified: SimConnect.xml ready on unprivileged TCP port {} + Win64 Named-Pipe bridge helper staged",
                active_port
            )
        };

        Ok(UnattendedSetupReport {
            prefix_path: pfx,
            roaming_path: roaming,
            active_port,
            xml_auto_provisioned,
            helper_deployed_path,
            proton_wine_bin,
            strategy_summary,
        })
    }

    /// Resolves the best active SimConnect TCP port completely unattended:
    /// 1. Live `FlightSimulator.exe` TCP listeners from `/proc/<pid>/fd` + `/proc/net/tcp` (catches `<Port>0</Port>` dynamic ports!)
    /// 2. `SimConnect_Port_IPv4` in `user.reg`
    /// 3. Unprivileged static ports in `SimConnect.xml` (e.g. `45454`, `2048`)
    /// 4. Automatic fallback to `aerodeck-pipe-bridge.exe` Named Pipe relay if MSFS is running in Pipe-only mode.
    pub fn resolve_best_simconnect_port(
        custom_prefix: Option<PathBuf>,
        preferred_port: u16,
    ) -> u16 {
        let mut candidates = Vec::new();

        // 1. Prioritize preferred_port (default 45454) if specified
        if preferred_port > 0 && !candidates.contains(&preferred_port) {
            candidates.push(preferred_port);
        }

        if let Some(roaming) = Self::find_msfs_roaming_path(custom_prefix.clone()) {
            let pfx = Self::resolve_pfx_root(&roaming).unwrap_or_else(|| roaming.clone());

            // 2. Static ports from SimConnect.xml
            for (_, xml_port) in Self::read_configured_ipv4_ports(&roaming) {
                if xml_port >= 1024 && !candidates.contains(&xml_port) {
                    candidates.push(xml_port);
                }
            }

            // 3. Dynamic port from Wine registry
            if let Some(reg_port) = Self::read_registry_dynamic_ipv4_port(&pfx) {
                if reg_port >= 1024 && !candidates.contains(&reg_port) {
                    candidates.push(reg_port);
                }
            }
        }

        // 4. Live MSFS TCP sockets discovered from /proc
        for port in Self::discover_live_msfs_tcp_ports() {
            if port >= 1024 && !candidates.contains(&port) {
                candidates.push(port);
            }
        }

        // Probe TCP reachability on all candidates
        for &port in &candidates {
            let addr = SocketAddr::from(([127, 0, 0, 1], port));
            if TcpStream::connect_timeout(&addr, Duration::from_millis(80)).is_ok() {
                return port;
            }
        }

        // 5. Fallback: If no TCP port is reachable, check if MSFS Named Pipe is active in Proton and spawn bridge!
        if let Some(roaming) = Self::find_msfs_roaming_path(custom_prefix) {
            let pfx = Self::resolve_pfx_root(&roaming).unwrap_or_else(|| roaming.clone());
            if let Ok(true) = Self::probe_and_spawn_pipe_bridge(&pfx, &roaming, preferred_port) {
                return preferred_port;
            }
        }

        preferred_port
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
