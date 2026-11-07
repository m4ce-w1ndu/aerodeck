use super::device::{DeviceEvent, FlightPanel, PID_BIP};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileColor {
    Off = 0,
    Green = 1,
    Amber = 2,
    Red = 3,
}

pub struct BipPanel {
    tiles: [TileColor; 24],
}

impl Default for BipPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl BipPanel {
    pub fn new() -> Self {
        Self {
            tiles: [TileColor::Off; 24],
        }
    }

    pub fn set_tile(&mut self, index: usize, color: TileColor) {
        if index < 24 {
            self.tiles[index] = color;
        }
    }
}

impl FlightPanel for BipPanel {
    fn name(&self) -> &str {
        "Saitek BIP Panel"
    }

    fn product_id(&self) -> u16 {
        PID_BIP
    }

    fn decode_input(&mut self, _report: &[u8]) -> Vec<DeviceEvent> {
        // BIP has no physical switch inputs
        Vec::new()
    }

    fn encode_display(&self) -> Vec<u8> {
        // Output report for BIP: 10 bytes
        // Each tile takes 2 bits: 00=Off, 01=Green, 10=Amber, 11=Red
        let mut report = vec![0u8; 10];
        report[0] = 0; // report ID

        for (i, tile) in self.tiles.iter().enumerate() {
            let byte_idx = 1 + (i / 4);
            let bit_offset = (i % 4) * 2;
            let val = *tile as u8;
            report[byte_idx] |= val << bit_offset;
        }

        report
    }
}
