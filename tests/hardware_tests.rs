use spad_linx::hardware::*;

#[test]
fn test_multi_panel_decoding_and_encoding() {
    let mut panel = MultiPanel::new();

    // Test input report with AP button pressed (bit 0 of byte 2)
    let report = [0x00, 0x00, 0x01];
    let events = panel.decode_input(&report);
    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0],
        DeviceEvent::ButtonPress { bound_to: "AP_BUTTON".to_string() }
    );

    // Test input report with Rotary Encoder Click Clockwise (bit 5 of byte 0)
    let report2 = [0x20, 0x00, 0x00];
    let events2 = panel.decode_input(&report2);
    assert_eq!(events2.len(), 2); // Release of AP button + Rotary turn
    assert!(events2.contains(&DeviceEvent::RotaryTurn {
        bound_to: "TUNER_INNER_INC".to_string(),
        clockwise: true
    }));

    // Test 7-segment display encoding
    panel.set_display("05000", " 1000", 0x01); // Alt 5000, VS 1000, AP led on
    let display_bytes = panel.encode_display();
    assert_eq!(display_bytes.len(), 12);
    assert_eq!(display_bytes[0], 0);
    assert_eq!(display_bytes[1], 5);
    assert_eq!(display_bytes[2], 0);
    assert_eq!(display_bytes[3], 0);
    assert_eq!(display_bytes[4], 0);
    assert_eq!(display_bytes[10], 0x01); // AP LED bit set
}

#[test]
fn test_switch_panel_gear_and_switches() {
    let mut panel = SwitchPanel::new();

    // Flip battery switch ON (bit 0 of byte 0)
    let report = [0x01, 0x00, 0x00];
    let events = panel.decode_input(&report);
    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0],
        DeviceEvent::SwitchChange {
            bound_to: "BATTERY_SWITCH".to_string(),
            state: true
        }
    );

    // Gear lever DOWN (bit 3 of byte 2)
    let report2 = [0x01, 0x00, 0x08];
    let events2 = panel.decode_input(&report2);
    assert_eq!(events2.len(), 1);
    assert_eq!(
        events2[0],
        DeviceEvent::SwitchChange {
            bound_to: "GEAR_DOWN".to_string(),
            state: true
        }
    );

    // Test gear LED output report
    panel.set_gear_leds(0x07); // 3 green LEDs
    assert_eq!(panel.encode_display(), vec![0x07]);
}

#[test]
fn test_radio_panel_frequency_encoding() {
    let mut panel = RadioPanel::new();
    panel.set_displays("122.80", "118.00", "121.50", "124.85");
    let report = panel.encode_display();
    assert_eq!(report.len(), 20);

    // Top active: '1', '2', '2.', '8', '0'
    // '2.' has decimal dot bit (0x80) -> 2 | 0x80 = 0x82
    assert_eq!(report[0], 1);
    assert_eq!(report[1], 2);
    assert_eq!(report[2], 2 | 0x80);
    assert_eq!(report[3], 8);
    assert_eq!(report[4], 0);
}
