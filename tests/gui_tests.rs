use aerodeck::gui::presets::{COMMON_CALCULATOR_CODES, SIMCONNECT_AXIS_PRESETS, SIMCONNECT_PRESETS, STANDARD_DEVICES};

#[test]
fn test_presets_coverage() {
    assert!(!SIMCONNECT_PRESETS.is_empty());
    assert!(!STANDARD_DEVICES.is_empty());
    assert!(!COMMON_CALCULATOR_CODES.is_empty());
    assert!(!SIMCONNECT_AXIS_PRESETS.is_empty());

    // Verify key flight presets exist
    let has_battery = SIMCONNECT_PRESETS.iter().any(|p| p.name == "TOGGLE_MASTER_BATTERY");
    let has_alt_inc = SIMCONNECT_PRESETS.iter().any(|p| p.name == "AP_ALT_VAR_INC");
    let has_gear = SIMCONNECT_PRESETS.iter().any(|p| p.name == "GEAR_TOGGLE");

    assert!(has_battery);
    assert!(has_alt_inc);
    assert!(has_gear);

    // Verify key analog axis presets exist
    let has_aileron = SIMCONNECT_AXIS_PRESETS.iter().any(|p| p.name == "AXIS_AILERONS_SET" && p.is_bipolar);
    let has_elevator = SIMCONNECT_AXIS_PRESETS.iter().any(|p| p.name == "AXIS_ELEVATOR_SET" && p.is_bipolar);
    let has_throttle = SIMCONNECT_AXIS_PRESETS.iter().any(|p| p.name == "AXIS_THROTTLE_SET" && !p.is_bipolar);
    let has_flaps = SIMCONNECT_AXIS_PRESETS.iter().any(|p| p.name == "AXIS_FLAPS_SET" && !p.is_bipolar);

    assert!(has_aileron, "Must have aileron roll axis preset");
    assert!(has_elevator, "Must have elevator pitch axis preset");
    assert!(has_throttle, "Must have master throttle axis preset");
    assert!(has_flaps, "Must have flaps axis preset");
}

#[test]
fn test_standard_devices_inputs() {
    let multi = STANDARD_DEVICES.iter().find(|d| d.id == "multi_panel").unwrap();
    assert!(multi.inputs.contains(&"AP_BUTTON_AP"));
    assert!(multi.inputs.contains(&"AP_ROTARY_CW"));

    let switch = STANDARD_DEVICES.iter().find(|d| d.id == "switch_panel").unwrap();
    assert!(switch.inputs.contains(&"SWITCH_BATTERY"));
    assert!(switch.inputs.contains(&"GEAR_LEVER_DOWN"));

    let radio = STANDARD_DEVICES.iter().find(|d| d.id == "radio_panel").unwrap();
    assert!(radio.inputs.contains(&"RADIO_ACT_STBY"));
}

#[test]
fn test_hardware_device_enumeration() {
    use aerodeck::hardware::enumerator::DeviceEnumerator;
    let devices = DeviceEnumerator::enumerate_all();
    assert!(!devices.is_empty(), "Should enumerate at least the virtual device");

    for dev in &devices {
        assert!(!dev.name.is_empty());
        assert!(!dev.id.is_empty());
        assert!(!dev.inputs.is_empty(), "Device {} has no inputs", dev.name);
    }

    // Verify virtual device has axes and axis ranges configured
    let virt = devices.iter().find(|d| d.id == "virtual_flight_panel").unwrap();
    assert!(!virt.axes.is_empty());
    assert!(virt.axis_ranges.contains_key("SIM_THROTTLE_1"));
    assert_eq!(virt.axis_ranges.get("SIM_THROTTLE_1"), Some(&(0, 16383)));
}

#[test]
fn test_proton_library_and_simconnect_xml() {
    use aerodeck::proton::ProtonConfigurator;

    let xml = ProtonConfigurator::generate_simconnect_xml_with_scope(2048, "local");
    assert!(xml.contains("<Port>2048</Port>"));
    assert!(xml.contains("<Port>500</Port>"));
    assert!(xml.contains("Custom\\SimConnect"));
    assert!(xml.contains("<Address>127.0.0.1</Address>"));

    let cfg = ProtonConfigurator::generate_simconnect_cfg("127.0.0.1", 2048);
    assert!(cfg.contains("Port=2048"));
    assert!(cfg.contains("Protocol=IPv4"));

    // Verify libraryfolders.vdf discovery, unattended setup, and Wine Named-Pipe Bridge self-test if Steam is installed
    let libs = ProtonConfigurator::discover_steam_libraries();
    if !libs.is_empty() {
        if let Some(roaming) = ProtonConfigurator::find_msfs_roaming_path(None) {
            assert!(roaming.exists());

            // Run unattended setup (auto-provisions SimConnect.xml on unprivileged port 45454 + stages aerodeck-pipe-bridge.exe)
            let report = ProtonConfigurator::ensure_unattended_setup(None)
                .expect("Unattended Proton setup should succeed");
            assert!(report.active_port >= 1024);
            assert!(report.helper_deployed_path.exists());

            let ports = ProtonConfigurator::read_configured_ipv4_ports(&roaming);
            assert!(
                ports.iter().any(|(_, p)| *p == 45454 || *p >= 1024),
                "SimConnect.xml should contain an unprivileged IPv4 port after unattended setup"
            );

            // Verify Wine Named-Pipe Bridge self-test runs inside the Proton prefix
            if report.proton_wine_bin.is_some() {
                let test_res = ProtonConfigurator::run_pipe_bridge_self_test(None)
                    .expect("Wine Named-Pipe Bridge self-test should pass");
                assert!(test_res.contains("SELF_TEST_OK"));
            }
        }
    }
}

#[test]
fn test_aircraft_discovery_binding_and_repository_profiles() {
    use aerodeck::profile::{
        AircraftAssignmentManager, AircraftScanner, ProfileParser, ProfileRepository,
    };

    // 1. Built-in repository profiles parse cleanly (in memory: tests must never write
    //    into the user's ./profiles directory)
    let catalog = ProfileRepository::builtin_catalog();
    assert!(
        catalog.len() >= 7,
        "Repository catalog must contain categorized profiles for Winwing, Thrustmaster TCA, and Saitek"
    );
    for entry in &catalog {
        let parsed = ProfileParser::parse_str(&entry.xml_content)
            .unwrap_or_else(|e| panic!("Profile {} failed to parse: {}", entry.filename, e));
        assert!(
            parsed.devices.as_ref().is_some_and(|d| !d.items.is_empty()),
            "Profile {} must have device bindings",
            entry.filename
        );
    }

    // 2. Verify MSFS Aircraft Discovery from Proton prefix (or fallback)
    let aircrafts = AircraftScanner::discover_installed_aircraft(None);
    assert!(
        aircrafts.len() >= 15,
        "Should discover installed MSFS aircraft (found {})",
        aircrafts.len()
    );
    let has_a320 = aircrafts
        .iter()
        .any(|a| a.title.to_lowercase().contains("a320") || a.icao == "A20N");
    let has_c172 = aircrafts
        .iter()
        .any(|a| a.title.to_lowercase().contains("skyhawk") || a.title.to_lowercase().contains("172"));
    assert!(has_a320, "Must discover Airbus A320neo");
    assert!(has_c172, "Must discover Cessna 172 Skyhawk");

    // 3. Aircraft-to-profile resolution, in memory only (no save to aircraft_assignments.json)
    let mut mgr = AircraftAssignmentManager::default();
    let fixture = "tests/fixtures/profiles/FBW A32NX.xml";
    mgr.assignments.insert("Airbus A320 Neo Asobo".to_string(), fixture.to_string());

    // Verify SimConnect SystemState("AircraftLoaded") path normalization -> Canonical Title -> Bound Profile
    let simconnect_path = r"SimObjects\AirPlanes\Asobo_A320_NEO\aircraft.cfg";
    let norm = AircraftAssignmentManager::normalize_aircraft_identifier(simconnect_path, &aircrafts);
    assert!(
        norm.to_lowercase().contains("a320"),
        "Normalized SimConnect path '{}' should resolve to A320 title, got '{}'",
        simconnect_path,
        norm
    );

    let resolved = mgr
        .resolve_profile_for_aircraft(simconnect_path, &aircrafts)
        .expect("Should resolve bound profile for A320 SimConnect path");
    assert!(
        resolved
            .to_string_lossy()
            .ends_with("FBW A32NX.xml"),
        "Expected the bound fixture profile, got {:?}",
        resolved
    );
}
