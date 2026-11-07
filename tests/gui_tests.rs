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

    // Verify libraryfolders.vdf discovery if Steam is installed on this host
    let libs = ProtonConfigurator::discover_steam_libraries();
    if !libs.is_empty() {
        if let Some(roaming) = ProtonConfigurator::find_msfs_roaming_path(None) {
            assert!(roaming.exists());
            let ports = ProtonConfigurator::read_configured_ipv4_ports(&roaming);
            assert!(!ports.is_empty(), "Should parse IPv4 port from existing SimConnect.xml");
        }
    }
}
