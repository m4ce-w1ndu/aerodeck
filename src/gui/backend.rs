use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{mpsc, Mutex};
use tokio::time::{sleep, Duration};
use tracing::trace;

use crate::bridge::{SpadLvarBridge, CALC_RET_DOUBLE};
use crate::hardware::device::DeviceEvent;
use crate::hardware::enumerator::{ConnectedDevice, DeviceEnumerator};
use crate::profile::{
    AircraftAssignmentManager, AircraftScanner, DiscoveredAircraft, OnlineSpadProfileEntry,
    ProfileExecutor, ProfileParser, ProfileRepository, SpadAccountConfig, SpadProfile,
    WindowsSpadProfileEntry,
};
use crate::simconnect::SimConnectClient;

#[derive(Debug, Clone)]
pub enum GuiEvent {
    SimConnectStatus { connected: bool, message: String },
    BridgeStatus { active: bool, lvar_count: usize },
    AircraftChanged(String),
    AircraftCatalogUpdated {
        aircrafts: Vec<DiscoveredAircraft>,
        assignments: AircraftAssignmentManager,
    },
    RepositoryProfileInstalled {
        id: String,
        name: String,
        path: String,
    },
    OnlineProfilesQueried(String),
    OnlineSpadCatalogLoaded(Vec<OnlineSpadProfileEntry>),
    WindowsSpadProfilesUpdated(Vec<WindowsSpadProfileEntry>),
    DevicesUpdated(Vec<ConnectedDevice>),
    InputTriggered {
        device: String,
        bound_to: String,
        action: String,
        time: Instant,
    },
    AxisMoved {
        device: String,
        bound_to: String,
        raw_value: i32,
        normalized: f64,
        min: i32,
        max: i32,
        time: Instant,
    },
    LvarsDiscovered(Vec<String>),
    LvarUpdated { name: String, value: f64 },
    Log { timestamp: String, level: String, text: String },
    ProfileLoaded { name: String, path: String },
    ProfileSaved { path: String },
    Error(String),
}

#[derive(Debug)]
pub enum GuiCommand {
    ConnectSimConnect { host: String, port: u16 },
    DisconnectSimConnect,
    FireSimConnectEvent { event_name: String, value: u32 },
    FireAxisEvent { event_name: String, value: i32 },
    SetLvar { name: String, value: f64 },
    ExecCalculatorCode { code: String },
    RequestLvarScan,
    RescanDevices,
    RescanAircrafts { custom_prefix: Option<PathBuf> },
    BindAircraftToProfile { aircraft_title: String, profile_path: String },
    UnbindAircraft { aircraft_title: String },
    SetDefaultProfile(Option<String>),
    SetAutoSwitchProfile(bool),
    SimulateAircraftSwitch(String),
    InstallRepositoryProfile {
        repo_id: String,
        activate_now: bool,
        bind_to_aircraft: Option<String>,
    },
    ImportProfileFromUrl {
        url: String,
        save_name: String,
        activate_now: bool,
    },
    QuerySpadOnlineSoap {
        search_query: String,
        device_vid: String,
        device_pid: String,
        license_token: Option<String>,
    },
    FetchOnlineSpadCatalog {
        account: SpadAccountConfig,
    },
    DownloadOnlineSpadSnippet {
        snippet_id: i32,
        profile_name: String,
        account: SpadAccountConfig,
        activate_now: bool,
        bind_to_aircraft: Option<String>,
    },
    ImportWindowsSpadProfile {
        source_path: PathBuf,
        activate_now: bool,
        bind_to_aircraft: Option<String>,
    },
    ImportAllWindowsSpadProfiles {
        custom_dir: Option<String>,
    },
    SimulateInput { device: String, bound_to: String, action: String },
    LoadProfile(PathBuf),
    SaveProfile(PathBuf),
}

pub fn start_backend(
    initial_profile: Option<PathBuf>,
    default_host: String,
    default_port: u16,
) -> (mpsc::UnboundedSender<GuiCommand>, mpsc::UnboundedReceiver<GuiEvent>) {
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<GuiCommand>();
    let (event_tx, event_rx) = mpsc::unbounded_channel::<GuiEvent>();

    let event_tx_clone = event_tx.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to create Tokio runtime for GUI backend");

        rt.block_on(async move {
            run_backend_loop(cmd_rx, event_tx_clone, initial_profile, default_host, default_port).await;
        });
    });

    (cmd_tx, event_rx)
}

async fn run_backend_loop(
    mut cmd_rx: mpsc::UnboundedReceiver<GuiCommand>,
    event_tx: mpsc::UnboundedSender<GuiEvent>,
    active_profile_path: Option<PathBuf>,
    _default_host: String,
    _default_port: u16,
) {
    let mut simconnect: Option<Arc<SimConnectClient>> = None;
    let mut bridge: Option<Arc<SpadLvarBridge>> = None;
    let mut active_profile: Option<Arc<Mutex<SpadProfile>>> = None;
    let mut current_profile_path: Option<String> = None;
    let mut executor: Option<Arc<ProfileExecutor>> = None;
    let mut user_disconnected = false;
    let mut tick_counter: u64 = 0;
    let mut last_active_aircraft = String::new();

    // 1. Unattended Proton & SimConnect Setup on Startup
    match crate::proton::ProtonConfigurator::ensure_unattended_setup(None) {
        Ok(report) => {
            let _ = event_tx.send(GuiEvent::Log {
                timestamp: chrono_now(),
                level: "INFO".to_string(),
                text: format!("[UNATTENDED] {}", report.strategy_summary),
            });
        }
        Err(e) => {
            let _ = event_tx.send(GuiEvent::Log {
                timestamp: chrono_now(),
                level: "WARN".to_string(),
                text: format!("[UNATTENDED] Proton prefix auto-setup skipped: {}", e),
            });
        }
    }

    // 2. Ensure Built-in Categorized Device & Aircraft Profiles are installed in ./profiles/
    ProfileRepository::ensure_builtin_profiles_on_disk();
    let imported_win = ProfileRepository::auto_import_windows_profiles_on_startup(None);
    if imported_win > 0 {
        let _ = event_tx.send(GuiEvent::Log {
            timestamp: chrono_now(),
            level: "INFO".to_string(),
            text: format!(
                "[AUTO-IMPORT] Imported {} Windows SPAD.neXt profiles into ./profiles/",
                imported_win
            ),
        });
    }
    let win_profiles = ProfileRepository::discover_windows_spad_profiles(None);
    let _ = event_tx.send(GuiEvent::WindowsSpadProfilesUpdated(win_profiles));

    // Spawn non-blocking background fetch of the live SPAD.neXt Online Repository
    let bg_tx = event_tx.clone();
    std::thread::spawn(move || {
        let acct = SpadAccountConfig::load_or_autodiscover();
        if let Ok(online) = ProfileRepository::fetch_online_spad_profiles(&acct) {
            let _ = bg_tx.send(GuiEvent::Log {
                timestamp: chrono_now(),
                level: "INFO".to_string(),
                text: format!(
                    "[SPAD.neXt ONLINE] Loaded {} live profiles from update.spadnext.net using license {} (Order #{})",
                    online.len(),
                    acct.email,
                    acct.purchase_id
                ),
            });
            let _ = bg_tx.send(GuiEvent::OnlineSpadCatalogLoaded(online));
        }
    });

    // 3. Discover Installed MSFS Aircraft and Load Profile Assignments
    let mut discovered_aircrafts = AircraftScanner::discover_installed_aircraft(None);
    let mut assignments = AircraftAssignmentManager::load_or_default();
    if assignments.assignments.is_empty() {
        assignments.import_from_xml_profiles();
        if assignments.default_profile.is_none() {
            assignments.default_profile = Some("profiles/cessna_ga_winwing_tca.xml".to_string());
        }
        let _ = assignments.save();
    }
    let _ = event_tx.send(GuiEvent::AircraftCatalogUpdated {
        aircrafts: discovered_aircrafts.clone(),
        assignments: assignments.clone(),
    });

    // Load initial profile if given
    if let Some(ref p) = active_profile_path {
        match ProfileParser::parse_file(p) {
            Ok(prof) => {
                let name = prof.name.clone().unwrap_or_else(|| "Default Profile".to_string());
                let path_str = p.to_string_lossy().to_string();
                current_profile_path = Some(path_str.clone());
                active_profile = Some(Arc::new(Mutex::new(prof)));
                let _ = event_tx.send(GuiEvent::ProfileLoaded {
                    name,
                    path: path_str,
                });
            }
            Err(e) => {
                let _ = event_tx.send(GuiEvent::Log {
                    timestamp: chrono_now(),
                    level: "WARN".to_string(),
                    text: format!("Could not load initial profile {:?}: {}", p, e),
                });
            }
        }
    }

    // Initial device enumeration and listener startup
    let devices = DeviceEnumerator::enumerate_all();
    let _ = event_tx.send(GuiEvent::DevicesUpdated(devices.clone()));
    let shutdown = Arc::new(AtomicBool::new(false));
    DeviceEnumerator::start_hardware_listeners(&devices, event_tx.clone(), shutdown.clone());

    loop {
        tokio::select! {
            cmd = cmd_rx.recv() => {
                match cmd {
                    Some(GuiCommand::ConnectSimConnect { host, port }) => {
                        user_disconnected = false;
                        let resolved_port = if host == "127.0.0.1" || host.eq_ignore_ascii_case("localhost") {
                            crate::proton::ProtonConfigurator::resolve_best_simconnect_port(None, port)
                        } else {
                            port
                        };
                        let addr = format!("{}:{}", host, resolved_port);
                        let _ = event_tx.send(GuiEvent::Log {
                            timestamp: chrono_now(),
                            level: "INFO".to_string(),
                            text: format!("Connecting to SimConnect at {} (resolved unattended)...", addr),
                        });
                        match SimConnectClient::connect(&addr).await {
                            Ok(sc) => {
                                let sc = Arc::new(sc);
                                if let Err(e) = sc.open("aerodeck-gui").await {
                                    let _ = event_tx.send(GuiEvent::SimConnectStatus {
                                        connected: false,
                                        message: format!("Failed to open SimConnect session: {}", e),
                                    });
                                    continue;
                                }

                                simconnect = Some(sc.clone());
                                let _ = event_tx.send(GuiEvent::SimConnectStatus {
                                    connected: true,
                                    message: format!("Connected to MSFS SimConnect ({})", addr),
                                });

                                // Initialize WASM LVAR Bridge
                                let br = Arc::new(SpadLvarBridge::new(sc.clone()));
                                match br.start().await {
                                    Ok(_) => {
                                        let _ = br.start_lvar_discovery().await;
                                        let _ = br.request_aircraft_name().await;
                                        bridge = Some(br.clone());
                                        let _ = event_tx.send(GuiEvent::BridgeStatus {
                                            active: true,
                                            lvar_count: 0,
                                        });
                                    }
                                    Err(e) => {
                                        let _ = event_tx.send(GuiEvent::Log {
                                            timestamp: chrono_now(),
                                            level: "WARN".to_string(),
                                            text: format!("WASM Bridge init warning: {}", e),
                                        });
                                    }
                                }

                                // Update ProfileExecutor with SimConnect and Bridge
                                if let (Some(ref prof), Some(ref br)) = (&active_profile, &bridge) {
                                    let prof_guard = prof.lock().await;
                                    executor = Some(Arc::new(ProfileExecutor::new(
                                        prof_guard.clone(),
                                        sc.clone(),
                                        br.clone(),
                                    )));
                                }
                            }
                            Err(e) => {
                                let _ = event_tx.send(GuiEvent::SimConnectStatus {
                                    connected: false,
                                    message: format!("Connection failed: {}", e),
                                });
                            }
                        }
                    }
                    Some(GuiCommand::DisconnectSimConnect) => {
                        user_disconnected = true;
                        simconnect = None;
                        bridge = None;
                        executor = None;
                        let _ = event_tx.send(GuiEvent::SimConnectStatus {
                            connected: false,
                            message: "Disconnected by user".to_string(),
                        });
                        let _ = event_tx.send(GuiEvent::BridgeStatus {
                            active: false,
                            lvar_count: 0,
                        });
                    }
                    Some(GuiCommand::FireSimConnectEvent { event_name, value }) => {
                        if let Some(ref sc) = simconnect {
                            let _ = sc.map_client_event_to_sim_event(1, &event_name).await;
                            let _ = sc.transmit_client_event(0, 1, value).await;
                            let _ = event_tx.send(GuiEvent::Log {
                                timestamp: chrono_now(),
                                level: "ACTION".to_string(),
                                text: format!("Dispatched SimConnect Event: {} (val: {})", event_name, value),
                            });
                        } else {
                            let _ = event_tx.send(GuiEvent::Log {
                                timestamp: chrono_now(),
                                level: "WARN".to_string(),
                                text: format!("Cannot fire '{}': SimConnect not connected", event_name),
                            });
                        }
                    }
                    Some(GuiCommand::FireAxisEvent { event_name, value }) => {
                        if let Some(ref sc) = simconnect {
                            let _ = sc.map_client_event_to_sim_event(2, &event_name).await;
                            let _ = sc.transmit_client_event(0, 2, value as u32).await;
                            trace!("Dispatched SimConnect Axis: {} = {}", event_name, value);
                        }
                    }
                    Some(GuiCommand::SetLvar { name, value }) => {
                        if let Some(ref br) = bridge {
                            let _ = br.set_lvar(&name, value).await;
                            let _ = event_tx.send(GuiEvent::Log {
                                timestamp: chrono_now(),
                                level: "ACTION".to_string(),
                                text: format!("Set LVAR '{}' = {}", name, value),
                            });
                        } else {
                            let _ = event_tx.send(GuiEvent::Log {
                                timestamp: chrono_now(),
                                level: "WARN".to_string(),
                                text: format!("Cannot set LVAR '{}': Bridge not active", name),
                            });
                        }
                    }
                    Some(GuiCommand::ExecCalculatorCode { code }) => {
                        if let Some(ref br) = bridge {
                            match br.execute_calculator_code(&code, CALC_RET_DOUBLE).await {
                                Ok(res) => {
                                    let _ = event_tx.send(GuiEvent::Log {
                                        timestamp: chrono_now(),
                                        level: "ACTION".to_string(),
                                        text: format!("Calc Code '{}' -> Result: {:?}", code, res),
                                    });
                                }
                                Err(e) => {
                                    let _ = event_tx.send(GuiEvent::Log {
                                        timestamp: chrono_now(),
                                        level: "ERROR".to_string(),
                                        text: format!("Calc Code error: {}", e),
                                    });
                                }
                            }
                        }
                    }
                    Some(GuiCommand::RequestLvarScan) => {
                        if let Some(ref br) = bridge {
                            let _ = br.start_lvar_discovery().await;
                            let count = br.lvar_count().await;
                            let names = br.get_known_lvars().await;
                            let _ = event_tx.send(GuiEvent::BridgeStatus {
                                active: true,
                                lvar_count: count,
                            });
                            let _ = event_tx.send(GuiEvent::LvarsDiscovered(names));
                        }
                    }
                    Some(GuiCommand::RescanDevices) => {
                        let devs = DeviceEnumerator::enumerate_all();
                        let _ = event_tx.send(GuiEvent::DevicesUpdated(devs.clone()));
                        DeviceEnumerator::start_hardware_listeners(&devs, event_tx.clone(), shutdown.clone());
                    }
                    Some(GuiCommand::RescanAircrafts { custom_prefix }) => {
                        discovered_aircrafts = AircraftScanner::discover_installed_aircraft(custom_prefix);
                        let _ = event_tx.send(GuiEvent::AircraftCatalogUpdated {
                            aircrafts: discovered_aircrafts.clone(),
                            assignments: assignments.clone(),
                        });
                        let _ = event_tx.send(GuiEvent::Log {
                            timestamp: chrono_now(),
                            level: "INFO".to_string(),
                            text: format!("Discovered {} installed MSFS aircraft packages.", discovered_aircrafts.len()),
                        });
                    }
                    Some(GuiCommand::BindAircraftToProfile { aircraft_title, profile_path }) => {
                        let _ = assignments.assign_aircraft(&aircraft_title, &profile_path);
                        let _ = event_tx.send(GuiEvent::AircraftCatalogUpdated {
                            aircrafts: discovered_aircrafts.clone(),
                            assignments: assignments.clone(),
                        });
                        let _ = event_tx.send(GuiEvent::Log {
                            timestamp: chrono_now(),
                            level: "INFO".to_string(),
                            text: format!("Bound aircraft '{}' -> profile '{}'", aircraft_title, profile_path),
                        });
                    }
                    Some(GuiCommand::UnbindAircraft { aircraft_title }) => {
                        let _ = assignments.unassign_aircraft(&aircraft_title);
                        let _ = event_tx.send(GuiEvent::AircraftCatalogUpdated {
                            aircrafts: discovered_aircrafts.clone(),
                            assignments: assignments.clone(),
                        });
                        let _ = event_tx.send(GuiEvent::Log {
                            timestamp: chrono_now(),
                            level: "INFO".to_string(),
                            text: format!("Unbound aircraft '{}'", aircraft_title),
                        });
                    }
                    Some(GuiCommand::SetDefaultProfile(def_prof)) => {
                        assignments.default_profile = def_prof.clone();
                        let _ = assignments.save();
                        let _ = event_tx.send(GuiEvent::AircraftCatalogUpdated {
                            aircrafts: discovered_aircrafts.clone(),
                            assignments: assignments.clone(),
                        });
                        let _ = event_tx.send(GuiEvent::Log {
                            timestamp: chrono_now(),
                            level: "INFO".to_string(),
                            text: format!("Updated default profile for unassigned aircraft: {:?}", def_prof),
                        });
                    }
                    Some(GuiCommand::SetAutoSwitchProfile(enabled)) => {
                        assignments.auto_switch_enabled = enabled;
                        let _ = assignments.save();
                        let _ = event_tx.send(GuiEvent::AircraftCatalogUpdated {
                            aircrafts: discovered_aircrafts.clone(),
                            assignments: assignments.clone(),
                        });
                        let _ = event_tx.send(GuiEvent::Log {
                            timestamp: chrono_now(),
                            level: "INFO".to_string(),
                            text: format!("Automatic profile switching on aircraft change: {}", if enabled { "ENABLED" } else { "DISABLED" }),
                        });
                    }
                    Some(GuiCommand::SimulateAircraftSwitch(raw_ac)) => {
                        handle_aircraft_change(
                            &raw_ac,
                            &mut last_active_aircraft,
                            &discovered_aircrafts,
                            &assignments,
                            &mut active_profile,
                            &mut current_profile_path,
                            &mut executor,
                            &simconnect,
                            &bridge,
                            &event_tx,
                        ).await;
                    }
                    Some(GuiCommand::InstallRepositoryProfile { repo_id, activate_now, bind_to_aircraft }) => {
                        let catalog = ProfileRepository::builtin_catalog();
                        if let Some(entry) = catalog.iter().find(|e| e.id == repo_id) {
                            match ProfileRepository::install_repository_profile(entry) {
                                Ok(path) => {
                                    let rel_path = path
                                        .file_name()
                                        .map(|f| format!("profiles/{}", f.to_string_lossy()))
                                        .unwrap_or_else(|| path.to_string_lossy().to_string());
                                    if let Ok(prof) = ProfileParser::parse_file(&path) {
                                        let prof_name = prof.name.clone().unwrap_or_else(|| entry.name.clone());
                                        assignments.import_from_xml_profiles();
                                        if let Some(ref ac_title) = bind_to_aircraft {
                                            let _ = assignments.assign_aircraft(ac_title, &rel_path);
                                        }
                                        let _ = event_tx.send(GuiEvent::RepositoryProfileInstalled {
                                            id: repo_id.clone(),
                                            name: prof_name.clone(),
                                            path: rel_path.clone(),
                                        });
                                        let _ = event_tx.send(GuiEvent::AircraftCatalogUpdated {
                                            aircrafts: discovered_aircrafts.clone(),
                                            assignments: assignments.clone(),
                                        });
                                        let _ = event_tx.send(GuiEvent::Log {
                                            timestamp: chrono_now(),
                                            level: "INFO".to_string(),
                                            text: format!("Installed repository profile '{}' to {}", prof_name, rel_path),
                                        });
                                        if activate_now {
                                            current_profile_path = Some(rel_path.clone());
                                            active_profile = Some(Arc::new(Mutex::new(prof.clone())));
                                            if let (Some(ref sc), Some(ref br)) = (&simconnect, &bridge) {
                                                executor = Some(Arc::new(ProfileExecutor::new(
                                                    prof,
                                                    sc.clone(),
                                                    br.clone(),
                                                )));
                                            }
                                            let _ = event_tx.send(GuiEvent::ProfileLoaded {
                                                name: prof_name,
                                                path: rel_path,
                                            });
                                        }
                                    }
                                }
                                Err(e) => {
                                    let _ = event_tx.send(GuiEvent::Error(format!("Failed to install profile '{}': {}", repo_id, e)));
                                }
                            }
                        }
                    }
                    Some(GuiCommand::ImportProfileFromUrl { url, save_name, activate_now }) => {
                        match ProfileRepository::download_profile_from_url(&url, &save_name) {
                            Ok(path) => {
                                let rel_path = path
                                    .file_name()
                                    .map(|f| format!("profiles/{}", f.to_string_lossy()))
                                    .unwrap_or_else(|| path.to_string_lossy().to_string());
                                if let Ok(prof) = ProfileParser::parse_file(&path) {
                                    let prof_name = prof.name.clone().unwrap_or_else(|| save_name.clone());
                                    assignments.import_from_xml_profiles();
                                    let _ = event_tx.send(GuiEvent::AircraftCatalogUpdated {
                                        aircrafts: discovered_aircrafts.clone(),
                                        assignments: assignments.clone(),
                                    });
                                    let _ = event_tx.send(GuiEvent::Log {
                                        timestamp: chrono_now(),
                                        level: "INFO".to_string(),
                                        text: format!("Downloaded profile '{}' from {} -> {}", prof_name, url, rel_path),
                                    });
                                    if activate_now {
                                        current_profile_path = Some(rel_path.clone());
                                        active_profile = Some(Arc::new(Mutex::new(prof.clone())));
                                        if let (Some(ref sc), Some(ref br)) = (&simconnect, &bridge) {
                                            executor = Some(Arc::new(ProfileExecutor::new(
                                                prof,
                                                sc.clone(),
                                                br.clone(),
                                            )));
                                        }
                                        let _ = event_tx.send(GuiEvent::ProfileLoaded {
                                            name: prof_name,
                                            path: rel_path,
                                        });
                                    }
                                }
                            }
                            Err(e) => {
                                let _ = event_tx.send(GuiEvent::Error(format!("URL download failed: {}", e)));
                            }
                        }
                    }
                    Some(GuiCommand::QuerySpadOnlineSoap { search_query: _, device_vid, device_pid, license_token }) => {
                        let provider = format!("{}:{}", device_vid.trim_start_matches("0x"), device_pid.trim_start_matches("0x"));
                        match ProfileRepository::query_spad_online_soap(
                            &provider,
                            0,
                            license_token.as_deref(),
                        ) {
                            Ok(resp) => {
                                let _ = event_tx.send(GuiEvent::Log {
                                    timestamp: chrono_now(),
                                    level: "INFO".to_string(),
                                    text: format!("SPAD.neXt SOAP repository response received ({} bytes).", resp.len()),
                                });
                                let _ = event_tx.send(GuiEvent::OnlineProfilesQueried(resp));
                            }
                            Err(e) => {
                                let _ = event_tx.send(GuiEvent::Log {
                                    timestamp: chrono_now(),
                                    level: "WARN".to_string(),
                                    text: format!("SPAD.neXt SOAP query: {}", e),
                                });
                            }
                        }
                    }
                    Some(GuiCommand::FetchOnlineSpadCatalog { account }) => {
                        let _ = account.save();
                        let bg_tx = event_tx.clone();
                        std::thread::spawn(move || {
                            match ProfileRepository::fetch_online_spad_profiles(&account) {
                                Ok(entries) => {
                                    let _ = bg_tx.send(GuiEvent::Log {
                                        timestamp: chrono_now(),
                                        level: "INFO".to_string(),
                                        text: format!(
                                            "Fetched {} live profiles from update.spadnext.net (X-Token auto-generated for {})",
                                            entries.len(),
                                            account.email
                                        ),
                                    });
                                    let _ = bg_tx.send(GuiEvent::OnlineSpadCatalogLoaded(entries));
                                }
                                Err(e) => {
                                    let _ = bg_tx.send(GuiEvent::Error(format!(
                                        "Failed to query SPAD.neXt Online Repository: {}",
                                        e
                                    )));
                                }
                            }
                        });
                    }
                    Some(GuiCommand::DownloadOnlineSpadSnippet {
                        snippet_id,
                        profile_name,
                        account,
                        activate_now,
                        bind_to_aircraft,
                    }) => {
                        match ProfileRepository::download_online_spad_snippet(
                            snippet_id,
                            &profile_name,
                            &account,
                        ) {
                            Ok(path) => {
                                let rel_path = path
                                    .file_name()
                                    .map(|f| format!("profiles/{}", f.to_string_lossy()))
                                    .unwrap_or_else(|| path.to_string_lossy().to_string());
                                if let Some(ref ac_title) = bind_to_aircraft {
                                    let _ = assignments.assign_aircraft(ac_title, &rel_path);
                                }
                                assignments.import_from_xml_profiles();
                                let _ = event_tx.send(GuiEvent::AircraftCatalogUpdated {
                                    aircrafts: discovered_aircrafts.clone(),
                                    assignments: assignments.clone(),
                                });
                                let _ = event_tx.send(GuiEvent::Log {
                                    timestamp: chrono_now(),
                                    level: "INFO".to_string(),
                                    text: format!(
                                        "Downloaded SPAD.neXt Online Profile #{} '{}' -> {}",
                                        snippet_id, profile_name, rel_path
                                    ),
                                });
                                if activate_now {
                                    if let Ok(mut prof) = ProfileParser::parse_file(&path) {
                                        if prof.name.is_none() {
                                            prof.name = Some(profile_name.clone());
                                        }
                                        let prof_name = prof
                                            .name
                                            .clone()
                                            .unwrap_or_else(|| profile_name.clone());
                                        current_profile_path = Some(rel_path.clone());
                                        active_profile = Some(Arc::new(Mutex::new(prof.clone())));
                                        if let (Some(ref sc), Some(ref br)) = (&simconnect, &bridge)
                                        {
                                            executor = Some(Arc::new(ProfileExecutor::new(
                                                prof,
                                                sc.clone(),
                                                br.clone(),
                                            )));
                                        }
                                        let _ = event_tx.send(GuiEvent::ProfileLoaded {
                                            name: prof_name,
                                            path: rel_path,
                                        });
                                    }
                                }
                            }
                            Err(e) => {
                                let _ = event_tx.send(GuiEvent::Error(format!(
                                    "Failed to download online profile #{}: {}",
                                    snippet_id, e
                                )));
                            }
                        }
                    }
                    Some(GuiCommand::ImportWindowsSpadProfile {
                        source_path,
                        activate_now,
                        bind_to_aircraft,
                    }) => {
                        match ProfileRepository::import_windows_spad_profile(&source_path) {
                            Ok(path) => {
                                let rel_path = path
                                    .file_name()
                                    .map(|f| format!("profiles/{}", f.to_string_lossy()))
                                    .unwrap_or_else(|| path.to_string_lossy().to_string());
                                if let Some(ref ac_title) = bind_to_aircraft {
                                    let _ = assignments.assign_aircraft(ac_title, &rel_path);
                                }
                                let win_profiles = ProfileRepository::discover_windows_spad_profiles(None);
                                let _ = event_tx.send(GuiEvent::WindowsSpadProfilesUpdated(win_profiles));
                                let _ = event_tx.send(GuiEvent::AircraftCatalogUpdated {
                                    aircrafts: discovered_aircrafts.clone(),
                                    assignments: assignments.clone(),
                                });
                                if activate_now {
                                    if let Ok(prof) = ProfileParser::parse_file(&path) {
                                        let prof_name = prof
                                            .name
                                            .clone()
                                            .unwrap_or_else(|| rel_path.clone());
                                        current_profile_path = Some(rel_path.clone());
                                        active_profile = Some(Arc::new(Mutex::new(prof.clone())));
                                        if let (Some(ref sc), Some(ref br)) = (&simconnect, &bridge)
                                        {
                                            executor = Some(Arc::new(ProfileExecutor::new(
                                                prof,
                                                sc.clone(),
                                                br.clone(),
                                            )));
                                        }
                                        let _ = event_tx.send(GuiEvent::ProfileLoaded {
                                            name: prof_name,
                                            path: rel_path,
                                        });
                                    }
                                }
                            }
                            Err(e) => {
                                let _ = event_tx.send(GuiEvent::Error(format!(
                                    "Failed to import Windows SPAD profile: {}",
                                    e
                                )));
                            }
                        }
                    }
                    Some(GuiCommand::ImportAllWindowsSpadProfiles { custom_dir }) => {
                        let count = ProfileRepository::auto_import_windows_profiles_on_startup(
                            custom_dir.as_deref(),
                        );
                        let win_profiles =
                            ProfileRepository::discover_windows_spad_profiles(custom_dir.as_deref());
                        let _ = event_tx.send(GuiEvent::WindowsSpadProfilesUpdated(win_profiles));
                        let _ = event_tx.send(GuiEvent::AircraftCatalogUpdated {
                            aircrafts: discovered_aircrafts.clone(),
                            assignments: assignments.clone(),
                        });
                        let _ = event_tx.send(GuiEvent::Log {
                            timestamp: chrono_now(),
                            level: "INFO".to_string(),
                            text: format!("Imported {} Windows SPAD.neXt profiles into ./profiles/", count),
                        });
                    }
                    Some(GuiCommand::SimulateInput { device, bound_to, action }) => {
                        let _ = event_tx.send(GuiEvent::InputTriggered {
                            device: device.clone(),
                            bound_to: bound_to.clone(),
                            action: action.clone(),
                            time: Instant::now(),
                        });
                        let _ = event_tx.send(GuiEvent::Log {
                            timestamp: chrono_now(),
                            level: "INPUT".to_string(),
                            text: format!("[{}] Input Triggered: {} -> {}", device, bound_to, action),
                        });

                        // If executor is running, execute profile mapping
                        if let Some(ref exec) = executor {
                            let dev_ev = match action.to_lowercase().as_str() {
                                "press" => DeviceEvent::ButtonPress { bound_to: bound_to.clone() },
                                "release" => DeviceEvent::ButtonRelease { bound_to: bound_to.clone() },
                                "cw" => DeviceEvent::RotaryTurn { bound_to: bound_to.clone(), clockwise: true },
                                "ccw" => DeviceEvent::RotaryTurn { bound_to: bound_to.clone(), clockwise: false },
                                _ => DeviceEvent::SwitchChange { bound_to: bound_to.clone(), state: true },
                            };
                            let _ = exec.handle_device_event(&device, &dev_ev).await;
                        }
                    }
                    Some(GuiCommand::LoadProfile(path)) => {
                        match ProfileParser::parse_file(&path) {
                            Ok(prof) => {
                                let name = prof.name.clone().unwrap_or_else(|| "Loaded Profile".to_string());
                                let path_str = path.to_string_lossy().to_string();
                                current_profile_path = Some(path_str.clone());
                                active_profile = Some(Arc::new(Mutex::new(prof.clone())));
                                if let (Some(ref sc), Some(ref br)) = (&simconnect, &bridge) {
                                    executor = Some(Arc::new(ProfileExecutor::new(
                                        prof,
                                        sc.clone(),
                                        br.clone(),
                                    )));
                                }
                                let _ = event_tx.send(GuiEvent::ProfileLoaded {
                                    name,
                                    path: path_str,
                                });
                            }
                            Err(e) => {
                                let _ = event_tx.send(GuiEvent::Error(format!("Failed to load profile: {}", e)));
                            }
                        }
                    }
                    Some(GuiCommand::SaveProfile(path)) => {
                        if let Some(ref prof) = active_profile {
                            let prof_guard = prof.lock().await;
                            match quick_xml::se::to_string_with_root("Profile", &*prof_guard) {
                                Ok(xml) => {
                                    let full_xml = format!(r#"<?xml version="1.0" encoding="utf-8"?>
{}"#, xml);
                                    if let Err(e) = std::fs::write(&path, full_xml) {
                                        let _ = event_tx.send(GuiEvent::Error(format!("Failed to save profile: {}", e)));
                                    } else {
                                        let _ = event_tx.send(GuiEvent::ProfileSaved {
                                            path: path.to_string_lossy().to_string(),
                                        });
                                    }
                                }
                                Err(e) => {
                                    let _ = event_tx.send(GuiEvent::Error(format!("Failed to serialize profile: {}", e)));
                                }
                            }
                        }
                    }
                    None => break,
                }
            }
            _ = sleep(Duration::from_millis(500)) => {
                tick_counter = tick_counter.wrapping_add(1);

                // Unattended background auto-connect when FlightSimulator.exe is detected
                if simconnect.is_none() && !user_disconnected && tick_counter % 6 == 0 {
                    let live_ports = crate::proton::ProtonConfigurator::discover_live_msfs_tcp_ports();
                    if let Some(&port) = live_ports.first() {
                        let addr = format!("127.0.0.1:{}", port);
                        if let Ok(sc) = SimConnectClient::connect(&addr).await {
                            let sc = Arc::new(sc);
                            if sc.open("aerodeck-gui").await.is_ok() {
                                simconnect = Some(sc.clone());
                                let _ = event_tx.send(GuiEvent::SimConnectStatus {
                                    connected: true,
                                    message: format!("Auto-connected to MSFS SimConnect ({})", addr),
                                });
                                let br = Arc::new(SpadLvarBridge::new(sc.clone()));
                                if br.start().await.is_ok() {
                                    let _ = br.start_lvar_discovery().await;
                                    let _ = br.request_aircraft_name().await;
                                    bridge = Some(br.clone());
                                    let _ = event_tx.send(GuiEvent::BridgeStatus {
                                        active: true,
                                        lvar_count: 0,
                                    });
                                }
                                if let (Some(ref prof), Some(ref br)) = (&active_profile, &bridge) {
                                    let prof_guard = prof.lock().await;
                                    executor = Some(Arc::new(ProfileExecutor::new(
                                        prof_guard.clone(),
                                        sc.clone(),
                                        br.clone(),
                                    )));
                                }
                            }
                        }
                    }
                }

                // Periodic bridge check, live values, and aircraft name update
                if let Some(ref br) = bridge {
                    let count = br.lvar_count().await;
                    if count > 0 {
                        let names = br.get_known_lvars().await;
                        let _ = event_tx.send(GuiEvent::BridgeStatus {
                            active: true,
                            lvar_count: count,
                        });
                        let _ = event_tx.send(GuiEvent::LvarsDiscovered(names));
                    }
                    let values = br.get_lvar_values().await;
                    for (k, v) in values {
                        let _ = event_tx.send(GuiEvent::LvarUpdated { name: k, value: v });
                    }
                    let ac_name = br.get_active_aircraft().await;
                    if !ac_name.is_empty() && ac_name != last_active_aircraft {
                        handle_aircraft_change(
                            &ac_name,
                            &mut last_active_aircraft,
                            &discovered_aircrafts,
                            &assignments,
                            &mut active_profile,
                            &mut current_profile_path,
                            &mut executor,
                            &simconnect,
                            &bridge,
                            &event_tx,
                        ).await;
                    }
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn handle_aircraft_change(
    raw_ac: &str,
    last_active_aircraft: &mut String,
    discovered_aircrafts: &[DiscoveredAircraft],
    assignments: &AircraftAssignmentManager,
    active_profile: &mut Option<Arc<Mutex<SpadProfile>>>,
    current_profile_path: &mut Option<String>,
    executor: &mut Option<Arc<ProfileExecutor>>,
    simconnect: &Option<Arc<SimConnectClient>>,
    bridge: &Option<Arc<SpadLvarBridge>>,
    event_tx: &mpsc::UnboundedSender<GuiEvent>,
) {
    *last_active_aircraft = raw_ac.to_string();
    let canonical = AircraftAssignmentManager::normalize_aircraft_identifier(raw_ac, discovered_aircrafts);
    let _ = event_tx.send(GuiEvent::AircraftChanged(canonical.clone()));

    if !assignments.auto_switch_enabled {
        return;
    }

    if let Some(target_profile_buf) = assignments.resolve_profile_for_aircraft(&canonical, discovered_aircrafts)
        .or_else(|| assignments.resolve_profile_for_aircraft(raw_ac, discovered_aircrafts))
    {
        let target_profile_path = target_profile_buf.to_string_lossy().to_string();
        let already_loaded = current_profile_path
            .as_deref()
            .map(|cur| cur == target_profile_path || cur.ends_with(&target_profile_path))
            .unwrap_or(false);

        if !already_loaded && target_profile_buf.exists() {
            if let Ok(prof) = ProfileParser::parse_file(&target_profile_buf) {
                let prof_name = prof.name.clone().unwrap_or_else(|| target_profile_path.clone());
                *current_profile_path = Some(target_profile_path.clone());
                *active_profile = Some(Arc::new(Mutex::new(prof.clone())));
                if let (Some(ref sc), Some(ref br)) = (simconnect, bridge) {
                    *executor = Some(Arc::new(ProfileExecutor::new(
                        prof,
                        sc.clone(),
                        br.clone(),
                    )));
                }
                let _ = event_tx.send(GuiEvent::ProfileLoaded {
                    name: prof_name.clone(),
                    path: target_profile_path.clone(),
                });
                let _ = event_tx.send(GuiEvent::Log {
                    timestamp: chrono_now(),
                    level: "INFO".to_string(),
                    text: format!(
                        "[AUTO-SWITCH] Aircraft switched to '{}' -> Automatically loaded bound profile '{}' ({})",
                        canonical, prof_name, target_profile_path
                    ),
                });
            }
        }
    }
}

fn chrono_now() -> String {
    use std::time::SystemTime;
    let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs();
    let hours = (now / 3600) % 24;
    let mins = (now / 60) % 60;
    let secs = now % 60;
    format!("{:02}:{:02}:{:02}", hours, mins, secs)
}
