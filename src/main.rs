#![allow(dead_code, unused_imports, unused_variables)]

mod simconnect;
mod bridge;
mod profile;
mod hardware;
mod proton;
mod gui;
mod pmdg;

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
#[command(name = "aerodeck")]
#[command(author = "AeroDeck developers")]
#[command(version)]
#[command(about = "AeroDeck: Advanced Flight Simulation Hardware & Telemetry Suite (SPAD.neXt Profile Compatible)", long_about = None)]
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

    /// Run profile with SimConnect and flight hardware (SPAD.neXt profile compatible)
    Run {
        #[arg(short, long)]
        profile: PathBuf,

        #[arg(short, long, default_value = "127.0.0.1:45454")]
        simconnect: String,
    },

    /// Profile management commands (SPAD.neXt compatible XML profiles)
    Profile {
        #[command(subcommand)]
        action: ProfileCommands,
    },

    /// MSFS WASM LVAR Bridge direct commands
    Bridge {
        #[command(subcommand)]
        action: BridgeCommands,
    },

    /// Scan and test connected hardware flight controllers and panels
    Devices {
        #[command(subcommand)]
        action: DeviceCommands,
    },

    /// Steam Proton configuration helper
    Proton {
        #[command(subcommand)]
        action: ProtonCommands,
    },

    /// PMDG aircraft SDK (737 / 777): status and data broadcast setup
    Pmdg {
        #[command(subcommand)]
        action: PmdgCommands,
    },

    /// Interactive virtual test session (simulate events without hardware)
    TestVirtual {
        #[arg(short, long)]
        profile: PathBuf,
    },
}

#[derive(Subcommand)]
enum ProfileCommands {
    /// Inspect a SPAD.neXt compatible profile file
    Inspect {
        #[arg(short, long)]
        path: PathBuf,
    },
    /// Generate a sample SPAD.neXt compatible XML profile
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
enum PmdgCommands {
    /// Show installed PMDG aircraft, their SDK and whether data broadcast is enabled
    Status,
    /// Set `[SDK] EnableDataBroadcast=1` in the options file of every installed PMDG aircraft
    EnableSdk,
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
            println!("{}", "      AERODECK: Flight Sim Hardware & Telemetry Suite     ".bright_cyan().bold());
            println!("{}", "     Compatible with SPAD.neXt Profiles (Linux/Windows)  ".bright_cyan());
            println!("{}", "=========================================================".bright_blue());
            println!("Launching AeroDeck Interactive Graphical Interface...");
            gui::run_gui(None, "127.0.0.1".to_string(), 45454)
        }
        Some(Commands::Gui { profile, simconnect }) => {
            let (host, port) = parse_host_port(&simconnect);
            println!("{}", "=========================================================".bright_blue());
            println!("{}", "      AERODECK: Flight Sim Hardware & Telemetry Suite     ".bright_cyan().bold());
            println!("{}", "     Compatible with SPAD.neXt Profiles (Linux/Windows)  ".bright_cyan());
            println!("{}", "=========================================================".bright_blue());
            println!("Launching AeroDeck Interactive Graphical Interface...");
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
    println!("{}", "      AERODECK: Flight Sim Hardware & Telemetry Suite     ".bright_cyan().bold());
    println!("{}", "     Compatible with SPAD.neXt Profiles (Linux/Windows)  ".bright_cyan());
    println!("{}", "=========================================================".bright_blue());

    match cmd {
        Commands::Gui { .. } => unreachable!(),
        Commands::Run { profile, simconnect } => {
            info!("Loading profile from {:?}", profile);
            let prof = ProfileParser::parse_file(&profile)?;
            println!("{} Loaded profile: {}", "✔".green().bold(), prof.name.as_deref().unwrap_or("Unnamed").bold());

            let sc = Arc::new(SimConnectClient::connect(&simconnect).await?);
            sc.open("aerodeck").await?;

            let bridge = Arc::new(SpadLvarBridge::new(sc.clone()));
            bridge.start().await?;

            let executor = Arc::new(ProfileExecutor::new(prof, sc.clone(), bridge.clone()));

            let devices = hardware::enumerator::DeviceEnumerator::enumerate_all();
            let (gui_tx, mut gui_rx) = tokio::sync::mpsc::unbounded_channel();
            let (hw_tx, mut hw_rx) = tokio::sync::mpsc::unbounded_channel();
            hardware::enumerator::DeviceEnumerator::start_hardware_listeners(
                &devices,
                gui_tx,
                Some(hw_tx),
                Arc::new(std::sync::atomic::AtomicBool::new(false)),
            );
            // GUI display events are not needed headless; drain them.
            tokio::spawn(async move { while gui_rx.recv().await.is_some() {} });

            println!("{} Engine running. Listening for events...", "✈".green().bold());
            while let Some(input) = hw_rx.recv().await {
                for notice in executor.take_notices() {
                    println!("{} {}", "ℹ".cyan(), notice);
                }
                use hardware::enumerator::HardwareInput;
                match input {
                    HardwareInput::Axis { device_id, bound_to, raw, min, max } => {
                        let _ = executor.dispatch_axis_event(&device_id, &bound_to, raw, min, max).await;
                    }
                    HardwareInput::Button { device_id, bound_to, trigger_prefix, pressed } => {
                        let exec = executor.clone();
                        tokio::spawn(async move {
                            exec.handle_button(&device_id, &bound_to, &trigger_prefix, pressed).await;
                        });
                    }
                }
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
                println!("{} Sample XML profile written to {:?}", "✔".green().bold(), output);
            }
        },

        Commands::Bridge { action } => match action {
            BridgeCommands::ListLvars { simconnect } => {
                let sc = Arc::new(SimConnectClient::connect(&simconnect).await?);
                sc.open("aerodeck-lvar-scan").await?;

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
                sc.open("aerodeck-exec").await?;

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
                println!("Scanning connected flight simulation controllers and panels...");
                let devs = hardware::DeviceEnumerator::enumerate_all();
                println!("{} Found {} flight & control device(s):", "✔".green().bold(), devs.len());
                for dev in devs {
                    println!("\n  {} [{}] (VID: {:04X}, PID: {:04X})",
                        dev.name.bold().green(),
                        dev.category.badge().yellow(),
                        dev.vendor_id,
                        dev.product_id
                    );
                    if let Some(ref path) = dev.path {
                        println!("    Path: {:?}", path);
                    }
                    println!("    Buttons ({}): {}", dev.buttons.len(), dev.buttons.join(", "));
                    if !dev.axes.is_empty() {
                        println!("    Axes ({}): {}", dev.axes.len(), dev.axes.join(", "));
                    }
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

        Commands::Pmdg { action } => {
            let installed = pmdg::discover_installed();
            if installed.is_empty() {
                println!("{} No PMDG aircraft with an SDK header found in the MSFS packages.", "✖".red().bold());
                return Ok(());
            }
            for inst in installed {
                let enabled = pmdg::broadcast_enabled(&inst.options_file);
                match action {
                    PmdgCommands::Status => {
                        let sdk = pmdg::PmdgSdk::load(inst.family, &inst.header);
                        println!("{} {} ({})", "✈".cyan(), inst.family.label().bold(), inst.package);
                        match sdk {
                            Ok(s) => println!("    SDK: {} events, {} data fields", s.events.len(), s.fields.len()),
                            Err(e) => println!("    SDK: {} {:#}", "unreadable:".red(), e),
                        }
                        println!(
                            "    Data broadcast: {}  ({})",
                            if enabled { "enabled".green() } else { "disabled".yellow() },
                            inst.options_file.display()
                        );
                    }
                    PmdgCommands::EnableSdk => match pmdg::enable_broadcast(&inst.options_file) {
                        Ok(true) => println!(
                            "{} {} ({}): data broadcast enabled. Reload the aircraft in MSFS to apply.",
                            "✔".green().bold(), inst.family.label(), inst.package
                        ),
                        Ok(false) => println!("{} {} ({}): already enabled", "✔".green(), inst.family.label(), inst.package),
                        Err(e) => println!("{} {} ({}): {:#}", "✖".red().bold(), inst.family.label(), inst.package, e),
                    },
                }
            }
        }

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
