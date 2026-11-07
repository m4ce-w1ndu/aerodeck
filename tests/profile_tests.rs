use spad_linx::profile::*;
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
