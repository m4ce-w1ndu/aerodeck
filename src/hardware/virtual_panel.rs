use super::device::{DeviceEvent, FlightPanel};
use super::multi_panel::MultiPanel;
use super::radio_panel::RadioPanel;
use super::switch_panel::SwitchPanel;

pub struct VirtualPanelHost {
    pub multi: MultiPanel,
    pub radio: RadioPanel,
    pub switch: SwitchPanel,
}

impl Default for VirtualPanelHost {
    fn default() -> Self {
        Self::new()
    }
}

impl VirtualPanelHost {
    pub fn new() -> Self {
        Self {
            multi: MultiPanel::new(),
            radio: RadioPanel::new(),
            switch: SwitchPanel::new(),
        }
    }

    pub fn simulate_event(&mut self, device: &str, bound_to: &str, action: &str) -> Option<DeviceEvent> {
        match action.to_lowercase().as_str() {
            "press" => Some(DeviceEvent::ButtonPress { bound_to: bound_to.to_string() }),
            "release" => Some(DeviceEvent::ButtonRelease { bound_to: bound_to.to_string() }),
            "inc" | "clockwise" => Some(DeviceEvent::RotaryTurn { bound_to: bound_to.to_string(), clockwise: true }),
            "dec" | "counter_clockwise" => Some(DeviceEvent::RotaryTurn { bound_to: bound_to.to_string(), clockwise: false }),
            "on" => Some(DeviceEvent::SwitchChange { bound_to: bound_to.to_string(), state: true }),
            "off" => Some(DeviceEvent::SwitchChange { bound_to: bound_to.to_string(), state: false }),
            _ => None,
        }
    }
}
