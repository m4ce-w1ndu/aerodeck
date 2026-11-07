use super::device::{DeviceEvent, FlightPanel, PID_SWITCH_PANEL};

pub struct SwitchPanel {
    last_report: [u8; 3],
    gear_leds: u8, // Bits 0-2: Green Left, Nose, Right; Bits 3-5: Red Left, Nose, Right
}

impl Default for SwitchPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl SwitchPanel {
    pub fn new() -> Self {
        Self {
            last_report: [0; 3],
            gear_leds: 0x07, // All 3 green by default (gear down & locked)
        }
    }

    pub fn set_gear_leds(&mut self, leds: u8) {
        self.gear_leds = leds;
    }
}

impl FlightPanel for SwitchPanel {
    fn name(&self) -> &str {
        "Saitek Switch Panel"
    }

    fn product_id(&self) -> u16 {
        PID_SWITCH_PANEL
    }

    fn decode_input(&mut self, report: &[u8]) -> Vec<DeviceEvent> {
        let mut events = Vec::new();
        if report.len() < 3 {
            return events;
        }

        let b0 = report[0];
        let b1 = report[1];
        let b2 = report[2];

        let prev0 = self.last_report[0];
        let prev1 = self.last_report[1];
        let prev2 = self.last_report[2];

        // Byte 0 Switches
        let switches_b0 = [
            (0x01, "BATTERY_SWITCH"),
            (0x02, "ALTERNATOR_SWITCH"),
            (0x04, "AVIONICS_SWITCH"),
            (0x08, "FUEL_PUMP_SWITCH"),
            (0x10, "DEICE_SWITCH"),
            (0x20, "PITOT_HEAT_SWITCH"),
            (0x40, "COWL_FLAPS_SWITCH"),
            (0x80, "PANEL_LIGHTS_SWITCH"),
        ];

        for (mask, name) in switches_b0 {
            let state = (b0 & mask) != 0;
            let prev = (prev0 & mask) != 0;
            if state != prev {
                events.push(DeviceEvent::SwitchChange { bound_to: name.to_string(), state });
            }
        }

        // Byte 1 Switches & Gear
        let switches_b1 = [
            (0x01, "BEACON_LIGHTS_SWITCH"),
            (0x02, "NAV_LIGHTS_SWITCH"),
            (0x04, "STROBE_LIGHTS_SWITCH"),
            (0x08, "TAXI_LIGHTS_SWITCH"),
            (0x10, "LANDING_LIGHTS_SWITCH"),
            (0x20, "MAGNETO_OFF"),
            (0x40, "MAGNETO_RIGHT"),
            (0x80, "MAGNETO_LEFT"),
        ];

        for (mask, name) in switches_b1 {
            let state = (b1 & mask) != 0;
            let prev = (prev1 & mask) != 0;
            if state != prev {
                events.push(DeviceEvent::SwitchChange { bound_to: name.to_string(), state });
            }
        }

        // Byte 2 Magneto BOTH, START, Gear UP, Gear DOWN
        let switches_b2 = [
            (0x01, "MAGNETO_BOTH"),
            (0x02, "STARTER_START"),
            (0x04, "GEAR_UP"),
            (0x08, "GEAR_DOWN"),
        ];

        for (mask, name) in switches_b2 {
            let state = (b2 & mask) != 0;
            let prev = (prev2 & mask) != 0;
            if state != prev {
                events.push(DeviceEvent::SwitchChange { bound_to: name.to_string(), state });
            }
        }

        self.last_report = [b0, b1, b2];
        events
    }

    fn encode_display(&self) -> Vec<u8> {
        // Output report: 1 byte representing the gear LEDs
        // Bit 0: Green Left, Bit 1: Green Nose, Bit 2: Green Right
        // Bit 3: Red Left, Bit 4: Red Nose, Bit 5: Red Right
        vec![self.gear_leds]
    }
}
