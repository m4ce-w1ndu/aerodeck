use super::device::{DeviceEvent, FlightPanel, PID_MULTI_PANEL};

pub struct MultiPanel {
    last_report: [u8; 3],
    upper_display: String, // 5 chars
    lower_display: String, // 5 chars
    ap_leds: u8,           // bitmask: AP, HDG, NAV, IAS, ALT, VS, APR, REV
}

impl Default for MultiPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl MultiPanel {
    pub fn new() -> Self {
        Self {
            last_report: [0; 3],
            upper_display: "     ".to_string(),
            lower_display: "     ".to_string(),
            ap_leds: 0,
        }
    }

    pub fn set_display(&mut self, upper: &str, lower: &str, leds: u8) {
        self.upper_display = format!("{:>5}", upper);
        self.lower_display = format!("{:>5}", lower);
        self.ap_leds = leds;
    }
}

impl FlightPanel for MultiPanel {
    fn name(&self) -> &str {
        "Saitek Multi Panel"
    }

    fn product_id(&self) -> u16 {
        PID_MULTI_PANEL
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

        // Mode switch rotary
        let modes = [
            (0x01, "MODE_ALT"),
            (0x02, "MODE_VS"),
            (0x04, "MODE_IAS"),
            (0x08, "MODE_HDG"),
            (0x10, "MODE_CRS"),
        ];

        for (mask, name) in modes {
            if (b0 & mask) != 0 && (prev0 & mask) == 0 {
                events.push(DeviceEvent::SwitchChange { bound_to: name.to_string(), state: true });
            }
        }

        // Rotary encoder dial
        if (b0 & 0x20) != 0 && (prev0 & 0x20) == 0 {
            events.push(DeviceEvent::RotaryTurn { bound_to: "TUNER_INNER_INC".to_string(), clockwise: true });
        }
        if (b0 & 0x40) != 0 && (prev0 & 0x40) == 0 {
            events.push(DeviceEvent::RotaryTurn { bound_to: "TUNER_INNER_DEC".to_string(), clockwise: false });
        }

        // Auto throttle ARM/OFF
        let at_state = (b0 & 0x80) != 0;
        let prev_at = (prev0 & 0x80) != 0;
        if at_state != prev_at {
            events.push(DeviceEvent::SwitchChange { bound_to: "AUTOTHROTTLE".to_string(), state: at_state });
        }

        // Flaps UP / DOWN
        if (b1 & 0x01) != 0 && (prev1 & 0x01) == 0 {
            events.push(DeviceEvent::ButtonPress { bound_to: "FLAPS_UP".to_string() });
        }
        if (b1 & 0x02) != 0 && (prev1 & 0x02) == 0 {
            events.push(DeviceEvent::ButtonPress { bound_to: "FLAPS_DOWN".to_string() });
        }

        // Pitch trim UP / DOWN wheel
        if (b1 & 0x04) != 0 && (prev1 & 0x04) == 0 {
            events.push(DeviceEvent::RotaryTurn { bound_to: "PITCH_TRIM_UP".to_string(), clockwise: false });
        }
        if (b1 & 0x08) != 0 && (prev1 & 0x08) == 0 {
            events.push(DeviceEvent::RotaryTurn { bound_to: "PITCH_TRIM_DOWN".to_string(), clockwise: true });
        }

        // AP Buttons
        let buttons = [
            (0x01, "AP_BUTTON"),
            (0x02, "HDG_BUTTON"),
            (0x04, "NAV_BUTTON"),
            (0x08, "IAS_BUTTON"),
            (0x10, "ALT_BUTTON"),
            (0x20, "VS_BUTTON"),
            (0x40, "APR_BUTTON"),
            (0x80, "REV_BUTTON"),
        ];

        for (mask, name) in buttons {
            let pressed = (b2 & mask) != 0;
            let was_pressed = (prev2 & mask) != 0;
            if pressed && !was_pressed {
                events.push(DeviceEvent::ButtonPress { bound_to: name.to_string() });
            } else if !pressed && was_pressed {
                events.push(DeviceEvent::ButtonRelease { bound_to: name.to_string() });
            }
        }

        self.last_report = [b0, b1, b2];
        events
    }

    fn encode_display(&self) -> Vec<u8> {
        // Multi panel output report: 12 bytes
        // Bytes 0-4: 5 upper digits
        // Bytes 5-9: 5 lower digits
        // Byte 10: LED bitmask
        // Byte 11: 0
        let mut out = vec![0xEE; 12];
        let encode_digit = |c: char| -> u8 {
            match c {
                '0'..='9' => (c as u8) - b'0',
                '-' => 0xEF,
                _ => 0xEE, // Blank
            }
        };

        let upper: Vec<char> = self.upper_display.chars().collect();
        for (i, c) in upper.iter().take(5).enumerate() {
            out[i] = encode_digit(*c);
        }

        let lower: Vec<char> = self.lower_display.chars().collect();
        for (i, c) in lower.iter().take(5).enumerate() {
            out[5 + i] = encode_digit(*c);
        }

        out[10] = self.ap_leds;
        out[11] = 0;
        out
    }
}
