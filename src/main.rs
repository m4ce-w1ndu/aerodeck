#![allow(dead_code, unused_imports, unused_variables)]

mod simconnect;
mod bridge;
mod profile;
mod hardware;
mod proton;
mod gui;

use anyhow::Result;
use clap::{Parser, Subcommand};
use colored::*;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::time::{sleep, Duration};
use tracing::{error, info, Level};
use tracing_subscriber::FmtSubscriber;

use bridge::{SpadLvarBridge, CALC_RET_DOUBLE, CALC_RET_STRING};
use hardware::virtual_panel::VirtualPanelHost;
use hardware::UsbPanelScanner;
use profile::{ProfileExecutor, ProfileParser, SpadProfile};
use proton::ProtonConfigurator;
use simconnect::SimConnectClient;

#[derive(Parser)]
#[command(name = "spad-linx")]
#[command(author = "spad-linx developers")]
#[command(version = "0.1.0")]
#[command(about = "Clean-room cross-platform SPAD.neXt profile engine & MSFS LVAR Bridge for Linux / Steam Proton", long_about = None)]
struct Cli {
    #[arg(short, long, default_value = "info")]
    log_level: String,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Launch interactive graphical user interface (Default when no command specified)
    Gui {
        #[arg(short, long)]
        profile: Option<PathBuf>,

        #[arg(short, long, default_value = "127.0.0.1:45454")]
        simconnect: String,
    },

    /// Run SPAD.neXt profile with SimConnect and hardware controllers
    Run {
        #[arg(short, long)]
        profile: PathBuf,

        #[arg(short, long, default_value = "127.0.0.1:45454")]
        simconnect: String,
    },

    /// Profile management commands
    Profile {
        #[command(subcommand)]
        action: ProfileCommands,
    },

    /// MSFS WASM LVAR Bridge direct commands
    Bridge {
        #[command(subcommand)]
        action: BridgeCommands,
    },

    /// Scan and test USB hardware flight panels
    Devices {
        #[command(subcommand)]
        action: DeviceCommands,
    },

    /// Steam Proton configuration helper
    Proton {
        #[command(subcommand)]
        action: ProtonCommands,
    },

    /// Interactive virtual test session (simulate events without hardware)
    TestVirtual {
        #[arg(short, long)]
        profile: PathBuf,
    },
}

#[derive(Subcommand)]
enum ProfileCommands {
    /// Inspect a SPAD.neXt profile file
    Inspect {
        #[arg(short, long)]
        path: PathBuf,
    },
    /// Generate a complete sample SPAD.neXt XML profile
    CreateSample {
        #[arg(short, long, default_value = "default_c172.xml")]
        output: PathBuf,
    },
}

#[derive(Subcommand)]
enum BridgeCommands {
    /// Connect to WASM Bridge and list discovered LVARs
    ListLvars {
        #[arg(short, long, default_value = "127.0.0.1:45454")]
        simconnect: String,
    },
    /// Execute MSFS RPN calculator code via WASM Bridge
    Exec {
        code: String,

        #[arg(short, long, default_value = "127.0.0.1:45454")]
        simconnect: String,

        #[arg(short, long, default_value = "double")]
        return_type: String,
    },
}

#[derive(Subcommand)]
enum DeviceCommands {
    /// Scan USB bus for Saitek / Logitech flight panels
    Scan,
}

#[derive(Subcommand)]
enum ProtonCommands {
    /// Configure SimConnect.xml in MSFS Steam Proton prefix for Linux network access
    Setup {
        #[arg(short, long)]
        prefix: Option<PathBuf>,

        #[arg(short, long, default_value_t = 45454)]
        port: u16,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let level = match cli.log_level.to_lowercase().as_str() {
        "trace" => Level::TRACE,
        "debug" => Level::DEBUG,
        "warn" => Level::WARN,
        "error" => Level::ERROR,
        _ => Level::INFO,
    };

    let subscriber = FmtSubscriber::builder()
        .with_max_level(level)
        .finish();
    let _ = tracing::subscriber::set_global_default(subscriber);

    match cli.command {
        None => {
            println!("{}", "=========================================================".bright_blue());
            println!("{}", "      SPAD-LINX: Clean-room Flight Sim Hardware Bridge   ".bright_cyan().bold());
            println!("{}", "      Cross-Platform SPAD.neXt Profile Engine for Linux  ".bright_cyan());
            println!("{}", "=========================================================".bright_blue());
            println!("Launching SPAD-LINX Interactive Graphical Interface...");
            gui::run_gui(None, "127.0.0.1".to_string(), 45454)
        }
        Some(Commands::Gui { profile, simconnect }) => {
            let (host, port) = parse_host_port(&simconnect);
            println!("{}", "=========================================================".bright_blue());
            println!("{}", "      SPAD-LINX: Clean-room Flight Sim Hardware Bridge   ".bright_cyan().bold());
            println!("{}", "      Cross-Platform SPAD.neXt Profile Engine for Linux  ".bright_cyan());
            println!("{}", "=========================================================".bright_blue());
            println!("Launching SPAD-LINX Interactive Graphical Interface...");
            gui::run_gui(profile, host, port)
        }
        Some(cmd) => {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?;
            rt.block_on(run_cli_command(cmd))
        }
    }
}

fn parse_host_port(s: &str) -> (String, u16) {
    if let Some((h, p)) = s.split_once(':') {
        (h.to_string(), p.parse::<u16>().unwrap_or(45454))
    } else {
        (s.to_string(), 45454)
    }
}

async fn run_cli_command(cmd: Commands) -> Result<()> {
    println!("{}", "=========================================================".bright_blue());
    println!("{}", "      SPAD-LINX: Clean-room Flight Sim Hardware Bridge   ".bright_cyan().bold());
    println!("{}", "      Cross-Platform SPAD.neXt Profile Engine for Linux  ".bright_cyan());
    println!("{}", "=========================================================".bright_blue());

    match cmd {
        Commands::Gui { .. } => unreachable!(),
        Commands::Run { profile, simconnect } => {
            info!("Loading SPAD.neXt profile from {:?}", profile);
            let prof = ProfileParser::parse_file(&profile)?;
            println!("{} Loaded profile: {}", "✔".green().bold(), prof.name.as_deref().unwrap_or("Unnamed").bold());

            let sc = Arc::new(SimConnectClient::connect(&simconnect).await?);
            sc.open("spad-linx").await?;

            let bridge = Arc::new(SpadLvarBridge::new(sc.clone()));
            bridge.start().await?;

            let executor = Arc::new(ProfileExecutor::new(prof, sc.clone(), bridge.clone()));

            println!("{} Engine running. Listening for events...", "✈".green().bold());
            // Keep running
            loop {
                sleep(Duration::from_secs(1)).await;
            }
        }

        Commands::Profile { action } => match action {
            ProfileCommands::Inspect { path } => {
                let prof = ProfileParser::parse_file(&path)?;
                println!("Profile Name: {}", prof.name.unwrap_or_default().bold());
                println!("Format Version: {:?}", prof.version);
                println!("Stock Profile: {:?}", prof.is_stock_profile);
                if let Some(vars) = prof.variables {
                    println!("\nVariables ({}):", vars.items.len());
                    for v in vars.items {
                        println!("  - {} (Scope: {:?}, Default: {:?})", v.name.yellow(), v.scope, v.value);
                    }
                }
                if let Some(devs) = prof.devices {
                    println!("\nConfigured Devices ({}):", devs.items.len());
                    for d in devs.items {
                        println!("  * Device: {} [VID: {:?}, PID: {:?}]",
                            d.name.unwrap_or_default().cyan().bold(), d.vendor_id, d.product_id);
                        for p in d.pages {
                            println!("    Page: {} (Default: {:?})", p.name.unwrap_or_default(), p.is_default);
                            if let Some(evs) = p.events {
                                println!("      Events count: {}", evs.items.len());
                                for ev in evs.items.iter().take(8) {
                                    println!("        - BoundTo: {}", ev.bound_to.green());
                                }
                                if evs.items.len() > 8 {
                                    println!("        ... and {} more events", evs.items.len() - 8);
                                }
                            }
                        }
                    }
                }
            }
            ProfileCommands::CreateSample { output } => {
                let sample_xml = create_default_spad_profile();
                std::fs::write(&output, sample_xml)?;
                println!("{} Sample SPAD.neXt profile written to {:?}", "✔".green().bold(), output);
            }
        },

        Commands::Bridge { action } => match action {
            BridgeCommands::ListLvars { simconnect } => {
                let sc = Arc::new(SimConnectClient::connect(&simconnect).await?);
                sc.open("spad-linx-lvar-scan").await?;

                let bridge = SpadLvarBridge::new(sc.clone());
                bridge.start().await?;

                println!("Waiting for bridge connection handshake...");
                sleep(Duration::from_millis(500)).await;

                bridge.start_lvar_discovery().await?;
                println!("Scanning MSFS gauge variables (LVARs)...");
                sleep(Duration::from_secs(2)).await;

                let lvars = bridge.get_known_lvars().await;
                let aircraft = bridge.get_active_aircraft().await;
                println!("\nActive Aircraft: {}", aircraft.bold().yellow());
                println!("Discovered LVARs ({} total):", lvars.len());
                for l in &lvars {
                    println!("  - L:{}", l.cyan());
                }
            }
            BridgeCommands::Exec { code, simconnect, return_type } => {
                let sc = Arc::new(SimConnectClient::connect(&simconnect).await?);
                sc.open("spad-linx-exec").await?;

                let bridge = SpadLvarBridge::new(sc.clone());
                bridge.start().await?;
                sleep(Duration::from_millis(400)).await;

                let ret_code = if return_type.eq_ignore_ascii_case("string") {
                    CALC_RET_STRING
                } else {
                    CALC_RET_DOUBLE
                };

                println!("Executing RPN calculator code: {}", code.bold().yellow());
                let result = bridge.execute_calculator_code(&code, ret_code).await?;
                println!("{} Execution Result:", "✔".green().bold());
                println!("  Status Op: {}", result.operation);
                println!("  Numeric Value: {}", result.value);
                if !result.message.is_empty() {
                    println!("  String Message: '{}'", result.message);
                }
            }
        },

        Commands::Devices { action } => match action {
            DeviceCommands::Scan => {
                println!("Scanning USB bus for Saitek / Logitech flight panels...");
                #[cfg(feature = "hardware")]
                {
                    let devs = UsbPanelScanner::scan_devices();
                    if devs.is_empty() {
                        println!("No physical Saitek panels detected on USB bus (udev / hidraw).");
                        println!("Note: Check udev rules or permissions if devices are plugged in.");
                    } else {
                        println!("{} Found {} panel(s):", "✔".green().bold(), devs.len());
                        for (vid, pid, mfg, prod) in devs {
                            println!("  * [{:04X}:{:04X}] {} - {}", vid, pid, mfg, prod.bold().green());
                        }
                    }
                }
                #[cfg(not(feature = "hardware"))]
                {
                    println!("Compiled without 'hardware' feature.");
                }
            }
        },

        Commands::Proton { action } => match action {
            ProtonCommands::Setup { prefix, port } => {
                let pfx_path = match prefix {
                    Some(p) => p,
                    None => match ProtonConfigurator::find_msfs_proton_prefix() {
                        Some(p) => {
                            println!("{} Detected MSFS Steam Proton prefix: {:?}", "✔".green().bold(), p);
                            p
                        }
                        None => {
                            eprintln!("{} Could not auto-detect MSFS Steam Proton prefix.", "✖".red().bold());
                            eprintln!("Please specify with --prefix <path_to_pfx>");
                            return Ok(());
                        }
                    },
                };

                match ProtonConfigurator::find_appdata_dir(&pfx_path) {
                    Some(appdata) => {
                        let xml_path = ProtonConfigurator::install_simconnect_xml(&appdata, port)?;
                        println!("{} Successfully generated SimConnect.xml for Proton at {:?}", "✔".green().bold(), xml_path);
                        println!("MSFS on Proton will now listen on 127.0.0.1:{} for native Linux clients!", port);
                    }
                    None => {
                        eprintln!("{} AppData directory not found in Proton prefix: {:?}", "✖".red().bold(), pfx_path);
                    }
                }
            }
        },

        Commands::TestVirtual { profile } => {
            println!("Testing profile in Virtual Emulator mode: {:?}", profile);
            let prof = ProfileParser::parse_file(&profile)?;
            println!("Profile: {}", prof.name.unwrap_or_default().bold());

            let mut virtual_host = VirtualPanelHost::new();
            println!("\nSimulating Multi Panel AP_BUTTON Press...");
            if let Some(ev) = virtual_host.simulate_event("0x0D06", "AP_BUTTON", "press") {
                println!("Generated event: {:?}", ev);
            }

            println!("Simulating Multi Panel TUNER_INNER_INC Rotary Click (Clockwise)...");
            if let Some(ev) = virtual_host.simulate_event("0x0D06", "TUNER_INNER_INC", "clockwise") {
                println!("Generated event: {:?}", ev);
            }

            println!("Simulating Switch Panel GEAR_DOWN Flip...");
            if let Some(ev) = virtual_host.simulate_event("0x0D67", "GEAR_DOWN", "on") {
                println!("Generated event: {:?}", ev);
            }

            println!("\n{} Virtual test completed successfully!", "✔".green().bold());
        }
    }

    Ok(())
}

fn create_default_spad_profile() -> String {
    r#"<?xml version="1.0" encoding="utf-8"?>
<Profile xmlns:xsd="http://www.w3.org/2001/XMLSchema" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" Version="1" IsStockProfile="false" xmlns="http://www.fsgs.com/SPAD">
  <Name>Cessna 172 Skyhawk MSFS 2020</Name>
  <Variables>
    <Variable Name="LOCAL:AUTOPILOT_TARGET_ALT" Scope="PROFILE" Value="5000" Type="System.Double" />
  </Variables>
  <Options>
    <Option Key="AIRCRAFT_NAME" Value="Cessna 172 Skyhawk G1000 Asobo" />
  </Options>
  <Devices>
    <!-- Saitek Multi Panel (Autopilot & Flaps) -->
    <Device VendorID="0x06A3" ProductID="0x0D06" DevicePath="" Serial="SPAD0D06" DeviceIndex="0" Name="Saitek Multi Panel" ID="0D06_0">
      <DevicePage ID="00000000-0000-0000-0000-000000000000" IsDefaultPage="true" Name="Default">
        <Events>
          <Event BoundTo="AP_BUTTON" ConfigID="AP_TOGGLE">
            <EventDefinitions>
              <EventDefinition Trigger="PRESSED" Priority="0" IsFinal="true">
                <EventActions>
                  <EventActionControl Target="SIMCONNECT:AP_MASTER" />
                </EventActions>
              </EventDefinition>
            </EventDefinitions>
          </Event>
          <Event BoundTo="TUNER_INNER_INC" ConfigID="ALT_INCREASE">
            <EventDefinitions>
              <EventDefinition Trigger="CLOCKWISE" Priority="0" IsFinal="true">
                <EventConditions>
                  <EventCondition Target="LOCAL:AUTOPILOT_TARGET_ALT" Comparator="Less" Value="50000" />
                </EventConditions>
                <EventActions>
                  <EventActionControl Target="SIMCONNECT:AP_ALT_VAR_INC" />
                  <EventActionChangeValue Target="LOCAL:AUTOPILOT_TARGET_ALT" Operation="Increment" Value="100" />
                </EventActions>
              </EventDefinition>
            </EventDefinitions>
          </Event>
          <Event BoundTo="TUNER_INNER_DEC" ConfigID="ALT_DECREASE">
            <EventDefinitions>
              <EventDefinition Trigger="COUNTER_CLOCKWISE" Priority="0" IsFinal="true">
                <EventConditions>
                  <EventCondition Target="LOCAL:AUTOPILOT_TARGET_ALT" Comparator="Greater" Value="0" />
                </EventConditions>
                <EventActions>
                  <EventActionControl Target="SIMCONNECT:AP_ALT_VAR_DEC" />
                  <EventActionChangeValue Target="LOCAL:AUTOPILOT_TARGET_ALT" Operation="Decrement" Value="100" />
                </EventActions>
              </EventDefinition>
            </EventDefinitions>
          </Event>
        </Events>
      </DevicePage>
    </Device>

    <!-- Saitek Switch Panel (Gear & Magnetos) -->
    <Device VendorID="0x06A3" ProductID="0x0D67" DevicePath="" Serial="SPAD0D67" DeviceIndex="0" Name="Saitek Switch Panel" ID="0D67_0">
      <DevicePage ID="00000000-0000-0000-0000-000000000001" IsDefaultPage="true" Name="Default">
        <Events>
          <Event BoundTo="GEAR_DOWN" ConfigID="GEAR_EXTEND">
            <EventDefinitions>
              <EventDefinition Trigger="ON" Priority="0" IsFinal="true">
                <EventActions>
                  <EventActionControl Target="SIMCONNECT:GEAR_DOWN" />
                </EventActions>
              </EventDefinition>
            </EventDefinitions>
          </Event>
          <Event BoundTo="GEAR_UP" ConfigID="GEAR_RETRACT">
            <EventDefinitions>
              <EventDefinition Trigger="ON" Priority="0" IsFinal="true">
                <EventActions>
                  <EventActionControl Target="SIMCONNECT:GEAR_UP" />
                </EventActions>
              </EventDefinition>
            </EventDefinitions>
          </Event>
        </Events>
      </DevicePage>
    </Device>
  </Devices>
</Profile>
"#.to_string()
}
