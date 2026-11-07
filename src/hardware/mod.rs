pub mod device;
pub mod radio_panel;
pub mod multi_panel;
pub mod switch_panel;
pub mod bip;
pub mod virtual_panel;

pub use device::*;
pub use radio_panel::RadioPanel;
pub use multi_panel::MultiPanel;
pub use switch_panel::SwitchPanel;
pub use bip::BipPanel;
pub use virtual_panel::VirtualPanelHost;

#[cfg(feature = "hardware")]
pub struct UsbPanelScanner;

#[cfg(feature = "hardware")]
impl UsbPanelScanner {
    pub fn scan_devices() -> Vec<(u16, u16, String, String)> {
        let mut list = Vec::new();
        if let Ok(api) = hidapi::HidApi::new() {
            for dev in api.device_list() {
                if dev.vendor_id() == VENDOR_SAITEK {
                    let mfg = dev.manufacturer_string().unwrap_or("Saitek").to_string();
                    let prod = dev.product_string().unwrap_or("Unknown Panel").to_string();
                    list.push((dev.vendor_id(), dev.product_id(), mfg, prod));
                }
            }
        }
        list
    }
}
