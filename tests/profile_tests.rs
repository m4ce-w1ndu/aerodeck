use aerodeck::profile::*;
use std::collections::HashMap;

#[test]
fn test_spad_profile_parsing() {
    let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<Profile Version="1" IsStockProfile="false" xmlns="http://www.fsgs.com/SPAD">
  <Name>Test Profile</Name>
  <Variables>
    <Variable Name="LOCAL:TEST_ALT" Scope="PROFILE" Value="3500" Type="System.Double" />
  </Variables>
  <Devices>
    <Device VendorID="0x06A3" ProductID="0x0D06" DevicePath="" Serial="SPAD0D06" DeviceIndex="0" Name="Saitek Multi Panel" ID="0D06_0">
      <DevicePage ID="00000000-0000-0000-0000-000000000000" IsDefaultPage="true" Name="Default">
        <Events>
          <Event BoundTo="TUNER_INNER_INC" ConfigID="ALT_INC">
            <EventDefinitions>
              <EventDefinition Trigger="CLOCKWISE" Priority="0" IsFinal="true">
                <EventConditions>
                  <EventCondition Target="LOCAL:TEST_ALT" Comparator="Less" Value="10000" />
                </EventConditions>
                <EventActions>
                  <EventActionControl Target="SIMCONNECT:AP_ALT_VAR_INC" />
                </EventActions>
              </EventDefinition>
            </EventDefinitions>
          </Event>
        </Events>
      </DevicePage>
    </Device>
  </Devices>
</Profile>"#;

    let profile = ProfileParser::parse_str(xml).expect("Should parse SPAD XML profile");
    assert_eq!(profile.name.as_deref(), Some("Test Profile"));
    assert_eq!(profile.version, Some(1));
    assert_eq!(profile.variables.unwrap().items.len(), 1);

    let devices = profile.devices.unwrap();
    assert_eq!(devices.items.len(), 1);
    let dev = &devices.items[0];
    assert_eq!(dev.name.as_deref(), Some("Saitek Multi Panel"));
    assert_eq!(dev.pages.len(), 1);

    let events = dev.pages[0].events.as_ref().unwrap();
    assert_eq!(events.items[0].bound_to, "TUNER_INNER_INC");
}

#[test]
fn test_condition_evaluation() {
    let mut lvars = HashMap::new();
    lvars.insert("A32NX_AUTOPILOT_1_ACTIVE".to_string(), 1.0);
    lvars.insert("A32NX_AUTOTHRUST_STATUS".to_string(), 2.0);

    let mut locals = HashMap::new();
    locals.insert("ALT_MODE".to_string(), "SELECTED".to_string());

    let evaluator = ConditionEvaluator::new(&lvars, &locals);

    // Test 1: Equals
    let c1 = EventConditionSimple {
        target: "LVAR:A32NX_AUTOPILOT_1_ACTIVE".to_string(),
        comparator: Some("Equals".to_string()),
        value: Some("1".to_string()),
    };
    assert!(evaluator.evaluate_simple(&c1));

    // Test 2: Less
    let c2 = EventConditionSimple {
        target: "LVAR:A32NX_AUTOTHRUST_STATUS".to_string(),
        comparator: Some("Less".to_string()),
        value: Some("1".to_string()),
    };
    assert!(!evaluator.evaluate_simple(&c2));

    // Test 3: String equals
    let c3 = EventConditionSimple {
        target: "LOCAL:ALT_MODE".to_string(),
        comparator: Some("Equals".to_string()),
        value: Some("SELECTED".to_string()),
    };
    assert!(evaluator.evaluate_simple(&c3));
}

#[test]
fn test_spad_v2_profile_and_token_generation() {
    let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<Profile xmlns:xsd="http://www.w3.org/2001/XMLSchema" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" Version="2" xmlns="http://www.fsgs.com/SPAD">
  <ProfileInformation>
    <Description />
    <Author>SIMON</Author>
    <LastChanged>2024-07-02T22:47:13.2712611+02:00</LastChanged>
    <ProfileID>6b0eb2db-1870-4cb9-ad8b-3a94ff7fc197</ProfileID>
  </ProfileInformation>
  <Devices>
    <Device VendorID="0x4098" ProductID="0xBC27" DevicePath="\\?\hid#vid_4098&amp;pid_bc27" DeviceIndex="0" ID="{8AEBB4C0-25BD-11EF-8002-444553540000}">
      <DevicePage ID="fe95bc17-d77d-4dc8-b592-aa95a9392b49" IsDefaultPage="true" Name="Default">
        <Events>
          <Event BoundTo="X_AXIS" isEnabled="true">
            <EventDefinitions>
              <EventDefinition Trigger="VALUE">
                <EventConditions />
                <EventActions>
                  <EventActionCustomAxis TargetDataDefinition="SIMCONNECT:AXIS_AILERONS_SET" ValueOperation="Set" AxisRangeMinimum="-16384" AxisRangeMaximum="16383" IsInverted="true" AlwaysSendRaw="true" SendRawValue="true" />
                </EventActions>
              </EventDefinition>
            </EventDefinitions>
          </Event>
          <Event BoundTo="BUTTON_3">
            <EventDefinitions>
              <EventDefinition Trigger="PRESS">
                <EventConditions>
                  <EventCondition ConditionComparator="Equals" ConditionTargetValue="1" ConditionValueSource="LVAR:A32NX_AP1" />
                </EventConditions>
                <EventActions>
                  <EventActionControl TargetDataDefinition="MSFS:AUTOPILOT_OFF" Value="0" ValueOperation="Once" />
                </EventActions>
              </EventDefinition>
            </EventDefinitions>
          </Event>
        </Events>
      </DevicePage>
    </Device>
  </Devices>
</Profile>"#;

    let profile = ProfileParser::parse_str(xml).expect("Should parse SPAD Version=2 XML profile");
    assert_eq!(profile.version, Some(2));
    assert_eq!(
        profile.profile_info.as_ref().and_then(|i| i.author.as_deref()),
        Some("SIMON")
    );
    let dev = &profile.devices.as_ref().unwrap().items[0];
    assert!(dev.matches_vid_pid(0x4098, 0xBC27));
    assert!(dev.matches_device_id("JOY_4098_BC27_0"));
    assert!(bound_to_matches("X_AXIS", "AXIS_X"));

    let acct = SpadAccountConfig::load_or_autodiscover();
    let token = acct.generate_x_token();
    assert!(!token.is_empty());

    let win_profiles = ProfileRepository::discover_windows_spad_profiles(None);
    for wp in &win_profiles {
        let parsed = ProfileParser::parse_file(std::path::Path::new(&wp.source_path))
            .unwrap_or_else(|e| panic!("Failed to parse Windows SPAD profile {}: {}", wp.name, e));
        assert!(parsed.name.is_some());
    }
}

#[test]
fn test_multi_device_profile_merge_and_roundtrip() {
    let joy_xml = r#"<?xml version="1.0" encoding="utf-8"?>
<Profile Version="2">
  <Name>A320 Combined</Name>
  <Devices>
    <Device VendorID="0x4098" ProductID="0xBC27" Name="WINCTRL URSA MINOR 32 Joystick L" ID="JOY_4098_BC27">
      <DevicePage ID="Default" IsDefaultPage="true" Name="Default">
        <Events>
          <Event BoundTo="BUTTON_3">
            <EventDefinitions>
              <EventDefinition Trigger="PRESS">
                <EventActions>
                  <EventActionControl Target="SIMCONNECT:AP_MASTER" Value="0" />
                </EventActions>
              </EventDefinition>
            </EventDefinitions>
          </Event>
        </Events>
      </DevicePage>
    </Device>
  </Devices>
</Profile>"#;

    let tca_xml = r#"<?xml version="1.0" encoding="utf-8"?>
<Profile Version="2">
  <Name>TCA Quadrant Snippet</Name>
  <Devices>
    <Device VendorID="0x044F" ProductID="0x0407" Name="Thrustmaster TCA Q-Eng 1&amp;2" ID="TCA_044F_0407">
      <DevicePage ID="Default" IsDefaultPage="true" Name="Default">
        <Events>
          <Event BoundTo="AXIS_X">
            <EventDefinitions>
              <EventDefinition Trigger="VALUE">
                <EventActions>
                  <EventActionCustomAxis Target="SIMCONNECT:THROTTLE1_AXIS_SET_EX1" />
                </EventActions>
              </EventDefinition>
            </EventDefinitions>
          </Event>
        </Events>
      </DevicePage>
    </Device>
  </Devices>
</Profile>"#;

    let mut base_prof = ProfileParser::parse_str(joy_xml).unwrap();
    let tca_prof = ProfileParser::parse_str(tca_xml).unwrap();

    // Merge TCA Quadrant into the same profile that already holds the Winwing Joystick
    let summary = base_prof.merge_from_profile(&tca_prof, None, true);
    assert_eq!(summary.total_devices_in_profile, 2);

    // Both devices must exist and retain their respective actions
    let joy_dev = base_prof
        .find_device_for_connected(0x4098, 0xBC27, "JOY_4098_BC27")
        .expect("Winwing Joystick must still exist in profile");
    assert_eq!(joy_dev.mapped_input_count(), 1);

    let tca_dev = base_prof
        .find_device_for_connected(0x044F, 0x0407, "TCA_044F_0407")
        .expect("Thrustmaster TCA Quadrant must exist in profile");
    assert_eq!(tca_dev.mapped_input_count(), 1);

    // Serialize and re-parse to verify full multi-device persistence
    let serialized = ProfileParser::serialize_to_string(&base_prof).expect("Should serialize multi-device profile");
    let reparsed = ProfileParser::parse_str(&serialized).expect("Should re-parse serialized multi-device profile");
    assert_eq!(reparsed.devices.as_ref().unwrap().items.len(), 2);
}

#[test]
fn test_aircraft_pattern_matching_and_editing() {
    let mut profile = SpadProfile {
        version: Some(2),
        is_stock_profile: Some(false),
        name: Some("PMDG 737 Profile".to_string()),
        variables: None,
        options: None,
        assigned_aircrafts: None,
        devices: None,
        profile_info: None,
    };

    assert!(!profile.matches_aircraft_title("PMDG 737-700 Air Canada"));

    // Add exact pattern
    profile.add_aircraft_pattern("PMDG 737-700*");
    assert!(profile.matches_aircraft_title("PMDG 737-700 Air Canada"));
    assert!(profile.matches_aircraft_title("PMDG 737-700 Southwest"));
    assert!(!profile.matches_aircraft_title("Fenix A320"));

    // Add wildcard
    profile.add_aircraft_pattern("B737");
    assert!(profile.matches_aircraft_title("Boeing B737 NG"));

    assert_eq!(profile.aircraft_patterns().len(), 2);

    // Remove pattern
    profile.remove_aircraft_pattern("B737");
    assert_eq!(profile.aircraft_patterns().len(), 1);
    assert!(!profile.matches_aircraft_title("Boeing B737 NG"));
    assert!(profile.matches_aircraft_title("PMDG 737-700"));
}

#[test]
fn test_per_device_layout_replacement_and_undo() {
    let mut profile = SpadProfile {
        version: Some(2),
        is_stock_profile: Some(false),
        name: Some("A320 Combined".to_string()),
        variables: None,
        options: None,
        assigned_aircrafts: None,
        devices: None,
        profile_info: None,
    };

    // Add two devices
    profile.add_device_for_hardware(0x4098, 0xBC27, "Winwing Stick");
    profile.add_device_for_hardware(0x044F, 0x0407, "TCA Throttle");

    let devs = profile.devices.as_ref().unwrap();
    assert_eq!(devs.items.len(), 2);

    // Create a new layout for TCA Throttle with 1 event
    let new_tca_layout = DeviceProfile {
        vendor_id: Some("0x044F".to_string()),
        product_id: Some("0x0407".to_string()),
        device_path: None,
        serial: None,
        device_index: Some(0),
        name: Some("TCA Throttle".to_string()),
        id: Some("DEV_044F_0407".to_string()),
        pages: vec![DevicePage {
            id: Some("Default".to_string()),
            is_default: Some(true),
            name: Some("Default".to_string()),
            events: Some(EventList {
                items: vec![SpadEvent {
                    bound_to: "THROTTLE_AXIS".to_string(),
                    config_id: None,
                    event_definitions: Some(EventDefinitionsList {
                        items: vec![EventDefinition {
                            trigger: Some("VALUE".to_string()),
                            priority: None,
                            is_final: None,
                            condition_binding: None,
                            conditions: None,
                            actions: Some(EventActionsList {
                                items: vec![EventActionItem::Control(EventActionControl {
                                    target: "SIMCONNECT:AXIS_THROTTLE_SET".to_string(),
                                    value: None,
                                    parameters: Vec::new(),
                                })],
                            }),
                        }],
                    }),
                }],
            }),
        }],
    };

    // Replace device layout for TCA Throttle
    let prev = profile
        .replace_device_layout(0x044F, 0x0407, new_tca_layout)
        .expect("Should return previous layout");

    // The other device (Winwing Stick) must NOT be touched
    let stick = profile.find_device_for_connected(0x4098, 0xBC27, "").unwrap();
    assert_eq!(stick.name.as_deref(), Some("Winwing Stick"));

    // TCA now has the new throttle event
    let tca = profile.find_device_for_connected(0x044F, 0x0407, "").unwrap();
    assert_eq!(tca.mapped_input_count(), 1);

    // Test One-Step Undo: restore previous layout
    profile.replace_device_layout(0x044F, 0x0407, prev);
    let restored_tca = profile.find_device_for_connected(0x044F, 0x0407, "").unwrap();
    assert_eq!(restored_tca.mapped_input_count(), 0);

    // Test Clear Layout
    profile.clear_device_layout(0x4098, 0xBC27);
    let stick_after = profile.find_device_for_connected(0x4098, 0xBC27, "").unwrap();
    assert_eq!(stick_after.mapped_input_count(), 0);
}

#[test]
fn test_legacy_profile_migration_is_in_memory() {
    let dir = std::env::temp_dir().join("aerodeck_migr_test");
    let _ = std::fs::create_dir_all(&dir);
    let old_xml_path = dir.join("legacy_cessna.xml");
    let bak_path = dir.join("legacy_cessna.xml.bak");
    let _ = std::fs::remove_file(&bak_path);

    // Legacy file without Version and without Name and without Pages
    let legacy_xml = r#"<Profile><Devices><Device VendorID="0x06A3" ProductID="0x0D06" Name="Saitek Multi"></Device></Devices></Profile>"#;
    std::fs::write(&old_xml_path, legacy_xml).unwrap();

    let prof = ProfileParser::parse_file(&old_xml_path).expect("Must parse and migrate legacy file");
    assert_eq!(prof.version, Some(2));
    assert_eq!(prof.name.as_deref(), Some("legacy cessna"));
    let dev = &prof.devices.as_ref().unwrap().items[0];
    assert_eq!(dev.pages.len(), 1);
    assert_eq!(dev.pages[0].name.as_deref(), Some("Default"));

    // Reading upgrades in memory only: nothing is written next to the file
    assert!(!bak_path.exists(), "parsing must not create a .bak file");
    assert_eq!(std::fs::read_to_string(&old_xml_path).unwrap(), legacy_xml, "parsing must not modify the file");

    let _ = std::fs::remove_file(&old_xml_path);
    let _ = std::fs::remove_file(&bak_path);
}




#[test]
fn every_bundled_profile_parses() {
    let mut failures = Vec::new();
    let mut paths: Vec<_> = std::fs::read_dir("tests/fixtures/profiles").unwrap().flatten().map(|e| e.path()).collect();
    paths.push("default_c172.xml".into());
    for p in paths.into_iter().filter(|p| p.extension().is_some_and(|e| e == "xml")) {
        if let Err(e) = aerodeck::profile::ProfileParser::parse_file(&p) {
            failures.push(format!("{}: {:#}", p.display(), e));
        }
    }
    assert!(failures.is_empty(), "profiles failing to parse:\n{}", failures.join("\n"));
}
