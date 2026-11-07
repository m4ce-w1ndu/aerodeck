use super::device::{DeviceEvent, FlightPanel, PID_RADIO_PANEL};

pub struct RadioPanel {
    last_report: [u8; 3],
    act_display_top: String,
    stby_display_top: String,
    act_display_bottom: String,
    stby_display_bottom: String,
}

impl Default for RadioPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl RadioPanel {
    pub fn new() -> Self {
        Self {
            last_report: [0; 3],
            act_display_top: "122.80".to_string(),
            stby_display_top: "118.00".to_string(),
            act_display_bottom: "122.80".to_string(),
            stby_display_bottom: "118.00".to_string(),
        }
    }

    pub fn set_displays(&mut self, top_act: &str, top_stby: &str, bot_act: &str, bot_stby: &str) {
        self.act_display_top = top_act.to_string();
        self.stby_display_top = top_stby.to_string();
        self.act_display_bottom = bot_act.to_string();
        self.stby_display_bottom = bot_stby.to_string();
    }
}

impl FlightPanel for RadioPanel {
    fn name(&self) -> &str {
        "Saitek Radio Panel"
    }

    fn product_id(&self) -> u16 {
        PID_RADIO_PANEL
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

        // Top ACT/STBY button
        if (b0 & 0x80) != 0 && (prev0 & 0x80) == 0 {
            events.push(DeviceEvent::ButtonPress { bound_to: "ACT_STBY_TOP".to_string() });
        } else if (b0 & 0x80) == 0 && (prev0 & 0x80) != 0 {
            events.push(DeviceEvent::ButtonRelease { bound_to: "ACT_STBY_TOP".to_string() });
        }

        // Bottom ACT/STBY button
        if (b1 & 0x80) != 0 && (prev1 & 0x80) == 0 {
            events.push(DeviceEvent::ButtonPress { bound_to: "ACT_STBY_BOTTOM".to_string() });
        } else if (b1 & 0x80) == 0 && (prev1 & 0x80) != 0 {
            events.push(DeviceEvent::ButtonRelease { bound_to: "ACT_STBY_BOTTOM".to_string() });
        }

        // Concentric Rotary Encoders:
        // Top outer knob inc/dec (bits 0, 1)
        if (b2 & 0x01) != 0 && (prev2 & 0x01) == 0 {
            events.push(DeviceEvent::RotaryTurn { bound_to: "TUNER_OUTER_INC_TOP".to_string(), clockwise: true });
        }
        if (b2 & 0x02) != 0 && (prev2 & 0x02) == 0 {
            events.push(DeviceEvent::RotaryTurn { bound_to: "TUNER_OUTER_DEC_TOP".to_string(), clockwise: false });
        }

        // Top inner knob inc/dec (bits 2, 3)
        if (b2 & 0x04) != 0 && (prev2 & 0x04) == 0 {
            events.push(DeviceEvent::RotaryTurn { bound_to: "TUNER_INNER_INC_TOP".to_string(), clockwise: true });
        }
        if (b2 & 0x08) != 0 && (prev2 & 0x08) == 0 {
            events.push(DeviceEvent::RotaryTurn { bound_to: "TUNER_INNER_DEC_TOP".to_string(), clockwise: false });
        }

        // Bottom outer knob inc/dec (bits 4, 5)
        if (b2 & 0x10) != 0 && (prev2 & 0x10) == 0 {
            events.push(DeviceEvent::RotaryTurn { bound_to: "TUNER_OUTER_INC_BOTTOM".to_string(), clockwise: true });
        }
        if (b2 & 0x20) != 0 && (prev2 & 0x20) == 0 {
            events.push(DeviceEvent::RotaryTurn { bound_to: "TUNER_OUTER_DEC_BOTTOM".to_string(), clockwise: false });
        }

        // Bottom inner knob inc/dec (bits 6, 7)
        if (b2 & 0x40) != 0 && (prev2 & 0x40) == 0 {
            events.push(DeviceEvent::RotaryTurn { bound_to: "TUNER_INNER_INC_BOTTOM".to_string(), clockwise: true });
        }
        if (b2 & 0x80) != 0 && (prev2 & 0x80) == 0 {
            events.push(DeviceEvent::RotaryTurn { bound_to: "TUNER_INNER_DEC_BOTTOM".to_string(), clockwise: false });
        }

        self.last_report = [b0, b1, b2];
        events
    }

    fn encode_display(&self) -> Vec<u8> {
        // Radio panel output report: 20 bytes (or 23 bytes depending on driver mode)
        // 5 digits for Top Act, 5 for Top Stby, 5 for Bot Act, 5 for Bot Stby
        let mut out = vec![0xEE; 20];
        let parse_freq = |s: &str| -> [u8; 5] {
            let mut digits = [0xEE; 5];
            let mut d_idx = 0;
            for c in s.chars() {
                if d_idx >= 5 { break; }
                if c.is_ascii_digit() {
                    digits[d_idx] = (c as u8) - b'0';
                    d_idx += 1;
                } else if c == '.' && d_idx > 0 {
                    // Set decimal point bit on previous digit
                    digits[d_idx - 1] |= 0x80;
                }
            }
            digits
        };

        out[0..5].copy_from_slice(&parse_freq(&self.act_display_top));
        out[5..10].copy_from_slice(&parse_freq(&self.stby_display_top));
        out[10..15].copy_from_slice(&parse_freq(&self.act_display_bottom));
        out[15..20].copy_from_slice(&parse_freq(&self.stby_display_bottom));

        out
    }
}
