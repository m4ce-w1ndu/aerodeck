use anyhow::Result;

pub const VENDOR_SAITEK: u16 = 0x06A3;
pub const PID_RADIO_PANEL: u16 = 0x0D05;
pub const PID_MULTI_PANEL: u16 = 0x0D06;
pub const PID_SWITCH_PANEL: u16 = 0x0D67;
pub const PID_BIP: u16 = 0x0B4E;
pub const PID_FIP: u16 = 0xA2AE;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceEvent {
    ButtonPress { bound_to: String },
    ButtonRelease { bound_to: String },
    RotaryTurn { bound_to: String, clockwise: bool },
    SwitchChange { bound_to: String, state: bool },
}

pub trait FlightPanel: Send + Sync {
    fn name(&self) -> &str;
    fn product_id(&self) -> u16;
    fn decode_input(&mut self, report: &[u8]) -> Vec<DeviceEvent>;
    fn encode_display(&self) -> Vec<u8>;
}
