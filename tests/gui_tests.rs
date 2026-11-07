use spad_linx::gui::presets::{SIMCONNECT_PRESETS, STANDARD_DEVICES, COMMON_CALCULATOR_CODES};

#[test]
fn test_presets_coverage() {
    assert!(!SIMCONNECT_PRESETS.is_empty());
    assert!(!STANDARD_DEVICES.is_empty());
    assert!(!COMMON_CALCULATOR_CODES.is_empty());

    // Verify key flight presets exist
    let has_battery = SIMCONNECT_PRESETS.iter().any(|p| p.name == "TOGGLE_MASTER_BATTERY");
    let has_alt_inc = SIMCONNECT_PRESETS.iter().any(|p| p.name == "AP_ALT_VAR_INC");
    let has_gear = SIMCONNECT_PRESETS.iter().any(|p| p.name == "GEAR_TOGGLE");

    assert!(has_battery);
    assert!(has_alt_inc);
    assert!(has_gear);
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
    use spad_linx::hardware::enumerator::DeviceEnumerator;
    let devices = DeviceEnumerator::enumerate_all();
    assert!(!devices.is_empty(), "Should enumerate at least the virtual device");

    for dev in &devices {
        assert!(!dev.name.is_empty());
        assert!(!dev.id.is_empty());
        assert!(!dev.inputs.is_empty(), "Device {} has no inputs", dev.name);
    }
}
