use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;
use tracing::{debug, info, trace, warn};

use crate::gui::backend::GuiEvent;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceCategory {
    FlightStick,
    ThrottleQuadrant,
    RudderPedals,
    FlightPanel,
    Gamepad,
    Virtual,
}

impl DeviceCategory {
    pub fn badge(&self) -> &'static str {
        match self {
            DeviceCategory::FlightStick => "JOYSTICK",
            DeviceCategory::ThrottleQuadrant => "THROTTLE",
            DeviceCategory::RudderPedals => "RUDDER",
            DeviceCategory::FlightPanel => "PANEL",
            DeviceCategory::Gamepad => "CONTROLLER",
            DeviceCategory::Virtual => "VIRTUAL",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConnectedDevice {
    pub id: String,
    pub name: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub category: DeviceCategory,
    pub path: Option<PathBuf>,
    pub inputs: Vec<String>,
    pub buttons: Vec<String>,
    pub axes: Vec<String>,
    pub axis_ranges: HashMap<String, (i32, i32)>,
}

impl ConnectedDevice {
    pub fn is_evdev(&self) -> bool {
        self.path.is_some()
    }
}

pub struct DeviceEnumerator;

impl DeviceEnumerator {
    /// Enumerate all real hardware flight controllers, joysticks, throttles, panels, and virtual devices
    pub fn enumerate_all() -> Vec<ConnectedDevice> {
        let mut devices = Vec::new();

        // 1. Enumerate Linux evdev input devices (/dev/input/event*)
        Self::enumerate_evdev_devices(&mut devices);

        // 2. Enumerate Saitek / Logitech flight panels via USB HID
        #[cfg(feature = "hardware")]
        Self::enumerate_hid_saitek_panels(&mut devices);

        // 3. Always provide Virtual Flight Panel Simulator
        devices.push(Self::virtual_device());

        info!("Enumerated {} connected flight & control devices", devices.len());
        devices
    }

    /// Scans Linux evdev input subsystem for joysticks, flight sticks, throttles, and rudder pedals
    fn enumerate_evdev_devices(devices: &mut Vec<ConnectedDevice>) {
        for (path, device) in evdev::enumerate() {
            let name = device.name().unwrap_or("Unknown Input Device").to_string();
            let input_id = device.input_id();
            let vid = input_id.vendor();
            let pid = input_id.product();

            let has_keys = device.supported_keys();
            let has_abs = device.supported_absolute_axes();

            if has_keys.is_none() && has_abs.is_none() {
                continue;
            }

            // Exclude non-gaming / system devices
            let name_lower = name.to_lowercase();
            if name_lower.contains("power button")
                || name_lower.contains("hd-audio")
                || name_lower.contains("pc speaker")
                || name_lower.contains("brio")
                || name_lower.contains("headset")
                || name_lower.contains("mic")
                || name_lower.contains("line out")
                || name_lower.contains("hdmi")
                || name_lower.contains("consumer control")
                || name_lower.contains("keyboard")
                || name_lower.contains("tkl")
            {
                continue;
            }

            let mut buttons = Vec::new();
            let mut axes = Vec::new();
            let mut is_controller = false;

            if let Some(keys) = has_keys {
                for key in keys.iter() {
                    let code = key.code();
                    // BTN_JOYSTICK (0x120..0x12f), BTN_GAMEPAD (0x130..0x13f), BTN_TRIGGER_HAPPY (0x2c0..0x2ff)
                    if (0x120..=0x13f).contains(&code) || (0x2c0..=0x2ff).contains(&code) {
                        is_controller = true;
                        buttons.push(format_key_name(key));
                    }
                }
            }

            if let Some(abs_axes) = has_abs {
                for axis in abs_axes.iter() {
                    let axis_name = format_axis_name(axis);
                    if axis_name != "ABS_VOLUME" {
                        axes.push(axis_name.clone());
                        if axis_name.starts_with("ABS_X")
                            || axis_name.starts_with("ABS_Y")
                            || axis_name.starts_with("ABS_Z")
                            || axis_name.starts_with("ABS_R")
                            || axis_name.starts_with("ABS_THROTTLE")
                            || axis_name.starts_with("ABS_RUDDER")
                            || axis_name.starts_with("ABS_HAT")
                        {
                            is_controller = true;
                        }
                    }
                }
            }

            if !is_controller {
                continue;
            }

            // Determine category from name or axes
            let category = if name_lower.contains("throttle")
                || name_lower.contains("tca q")
                || name_lower.contains("quadrant")
                || name_lower.contains("tpm")
            {
                DeviceCategory::ThrottleQuadrant
            } else if name_lower.contains("rudder") || name_lower.contains("pedal") {
                DeviceCategory::RudderPedals
            } else if name_lower.contains("joystick")
                || name_lower.contains("stick")
                || name_lower.contains("ursa minor")
                || name_lower.contains("warthog")
                || name_lower.contains("x52")
                || name_lower.contains("x56")
                || name_lower.contains("gladiator")
                || name_lower.contains("t.16000")
            {
                DeviceCategory::FlightStick
            } else {
                DeviceCategory::Gamepad
            };

            let mut axis_ranges = HashMap::new();
            if let Ok(abs_infos) = device.get_absinfo() {
                for (code, info) in abs_infos {
                    let axis_name = format_axis_name(code);
                    axis_ranges.insert(axis_name, (info.minimum(), info.maximum()));
                }
            }

            let mut all_inputs = Vec::new();
            all_inputs.extend(buttons.clone());
            all_inputs.extend(axes.clone());

            let dev_id = format!("evdev_{:04x}_{:04x}_{}", vid, pid, path.file_name().and_then(|f| f.to_str()).unwrap_or(""));

            debug!("Found physical controller: '{}' (ID: {}, {} buttons, {} axes)", name, dev_id, buttons.len(), axes.len());

            devices.push(ConnectedDevice {
                id: dev_id,
                name,
                vendor_id: vid,
                product_id: pid,
                category,
                path: Some(path),
                inputs: all_inputs,
                buttons,
                axes,
                axis_ranges,
            });
        }
    }

    /// Enumerate Saitek / Logitech hardware panels connected via USB HID
    #[cfg(feature = "hardware")]
    fn enumerate_hid_saitek_panels(devices: &mut Vec<ConnectedDevice>) {
        use crate::hardware::device::*;
        if let Ok(api) = hidapi::HidApi::new() {
            for dev in api.device_list() {
                if dev.vendor_id() == VENDOR_SAITEK {
                    let (name, inputs): (&str, Vec<String>) = match dev.product_id() {
                        PID_MULTI_PANEL => (
                            "Saitek Pro Flight Multi Panel",
                            vec![
                                "AP_BUTTON_AP", "AP_BUTTON_HDG", "AP_BUTTON_NAV", "AP_BUTTON_IAS",
                                "AP_BUTTON_ALT", "AP_BUTTON_VS", "AP_BUTTON_APR", "AP_BUTTON_REV",
                                "AP_KNOB_ALT", "AP_KNOB_VS", "AP_KNOB_IAS", "AP_KNOB_HDG", "AP_KNOB_CRS",
                                "AP_ROTARY_CW", "AP_ROTARY_CCW", "AP_AUTO_THROTTLE",
                                "FLAPS_LEVER_UP", "FLAPS_LEVER_DOWN", "PITCH_TRIM_UP", "PITCH_TRIM_DOWN",
                            ].into_iter().map(|s| s.to_string()).collect(),
                        ),
                        PID_RADIO_PANEL => (
                            "Saitek Pro Flight Radio Panel",
                            vec![
                                "RADIO_UPPER_COM1", "RADIO_UPPER_COM2", "RADIO_UPPER_NAV1", "RADIO_UPPER_NAV2",
                                "RADIO_UPPER_ADF", "RADIO_UPPER_DME", "RADIO_UPPER_XPDR",
                                "RADIO_LOWER_COM1", "RADIO_LOWER_COM2", "RADIO_LOWER_NAV1", "RADIO_LOWER_NAV2",
                                "RADIO_LOWER_ADF", "RADIO_LOWER_DME", "RADIO_LOWER_XPDR",
                                "RADIO_ACT_STBY", "RADIO_COARSE_CW", "RADIO_COARSE_CCW", "RADIO_FINE_CW", "RADIO_FINE_CCW",
                            ].into_iter().map(|s| s.to_string()).collect(),
                        ),
                        PID_SWITCH_PANEL => (
                            "Saitek Pro Flight Switch Panel",
                            vec![
                                "SWITCH_BATTERY", "SWITCH_ALTERNATOR", "SWITCH_AVIONICS", "SWITCH_FUEL_PUMP",
                                "SWITCH_DEICE", "SWITCH_PITOT_HEAT", "SWITCH_COWL_FLAPS",
                                "SWITCH_LIGHTS_PANEL", "SWITCH_LIGHTS_BEACON", "SWITCH_LIGHTS_NAV",
                                "SWITCH_LIGHTS_STROBE", "SWITCH_LIGHTS_TAXI", "SWITCH_LIGHTS_LANDING",
                                "SWITCH_MAG_OFF", "SWITCH_MAG_RIGHT", "SWITCH_MAG_LEFT", "SWITCH_MAG_BOTH", "SWITCH_MAG_START",
                                "GEAR_LEVER_UP", "GEAR_LEVER_DOWN",
                            ].into_iter().map(|s| s.to_string()).collect(),
                        ),
                        PID_BIP => (
                            "Saitek Backlit Information Panel (BIP)",
                            (0..24).map(|i| format!("BIP_TILE_{}", i)).collect(),
                        ),
                        _ => continue,
                    };

                    devices.push(ConnectedDevice {
                        id: format!("saitek_{:04x}_{:04x}", dev.vendor_id(), dev.product_id()),
                        name: name.to_string(),
                        vendor_id: dev.vendor_id(),
                        product_id: dev.product_id(),
                        category: DeviceCategory::FlightPanel,
                        path: None,
                        inputs: inputs.clone(),
                        buttons: inputs,
                        axes: Vec::new(),
                        axis_ranges: HashMap::new(),
                    });
                }
            }
        }
    }

    /// Virtual software controller fallback
    pub fn virtual_device() -> ConnectedDevice {
        let inputs = vec![
            "SWITCH_BATTERY", "SWITCH_ALTERNATOR", "SWITCH_AVIONICS", "SWITCH_FUEL_PUMP",
            "SWITCH_DEICE", "SWITCH_PITOT_HEAT", "SWITCH_LIGHTS_BEACON", "SWITCH_LIGHTS_NAV",
            "SWITCH_LIGHTS_STROBE", "SWITCH_LIGHTS_TAXI", "SWITCH_LIGHTS_LANDING",
            "GEAR_LEVER_UP", "GEAR_LEVER_DOWN", "SWITCH_MAG_OFF", "SWITCH_MAG_RIGHT",
            "SWITCH_MAG_LEFT", "SWITCH_MAG_BOTH", "SWITCH_MAG_START",
            "AP_BUTTON_AP", "AP_BUTTON_HDG", "AP_BUTTON_NAV", "AP_BUTTON_ALT", "AP_BUTTON_VS",
            "AP_ROTARY_CW", "AP_ROTARY_CCW", "PITCH_TRIM_UP", "PITCH_TRIM_DOWN",
            "RADIO_ACT_STBY", "RADIO_COARSE_CW", "RADIO_COARSE_CCW", "RADIO_FINE_CW", "RADIO_FINE_CCW",
        ];
        let inputs_vec: Vec<String> = inputs.into_iter().map(|s| s.to_string()).collect();

        let mut axis_ranges = HashMap::new();
        axis_ranges.insert("SIM_THROTTLE_1".to_string(), (0, 16383));
        axis_ranges.insert("SIM_THROTTLE_2".to_string(), (0, 16383));

        ConnectedDevice {
            id: "virtual_flight_panel".to_string(),
            name: "Virtual Flight Panel Simulator".to_string(),
            vendor_id: 0x0000,
            product_id: 0x0000,
            category: DeviceCategory::Virtual,
            path: None,
            inputs: inputs_vec.clone(),
            buttons: inputs_vec,
            axes: vec!["SIM_THROTTLE_1".to_string(), "SIM_THROTTLE_2".to_string()],
            axis_ranges,
        }
    }

    /// Spawns background worker threads listening to all physical evdev flight controllers
    pub fn start_hardware_listeners(
        devices: &[ConnectedDevice],
        event_tx: mpsc::UnboundedSender<GuiEvent>,
        shutdown: Arc<AtomicBool>,
    ) {
        for dev in devices {
            if let Some(ref path) = dev.path {
                let path_clone = path.clone();
                let dev_name = dev.name.clone();
                let _dev_id = dev.id.clone();
                let etx = event_tx.clone();
                let stop = shutdown.clone();

                std::thread::spawn(move || {
                    let mut ev_device = match evdev::Device::open(&path_clone) {
                        Ok(d) => d,
                        Err(e) => {
                            warn!("Cannot open evdev device {:?} for background listening: {}", path_clone, e);
                            return;
                        }
                    };

                    let mut axis_cache: HashMap<u16, (i32, i32, i32)> = HashMap::new();
                    if let Ok(infos) = ev_device.get_absinfo() {
                        for (code, info) in infos {
                            axis_cache.insert(code.0, (info.minimum(), info.maximum(), info.value()));
                        }
                    }

                    info!("Started real-time event listener on hardware controller '{}' ({:?})", dev_name, path_clone);

                    while !stop.load(Ordering::Relaxed) {
                        match ev_device.fetch_events() {
                            Ok(events) => {
                                for ev in events {
                                    match ev.destructure() {
                                        evdev::EventSummary::Key(_, key_code, val) => {
                                            let bound_to = format_key_name(key_code);
                                            let action = match val {
                                                1 => "PRESS",
                                                0 => "RELEASE",
                                                2 => "REPEAT",
                                                _ => "CHANGE",
                                            };

                                            trace!("[{}] Key event: {} -> {}", dev_name, bound_to, action);

                                            let _ = etx.send(GuiEvent::InputTriggered {
                                                device: dev_name.clone(),
                                                bound_to: bound_to.clone(),
                                                action: action.to_string(),
                                                time: Instant::now(),
                                            });

                                            let _ = etx.send(GuiEvent::Log {
                                                timestamp: chrono_now_str(),
                                                level: "INPUT".to_string(),
                                                text: format!("[{}] Hardware Event: {} -> {}", dev_name, bound_to, action),
                                            });
                                        }
                                        evdev::EventSummary::AbsoluteAxis(_, axis_code, val) => {
                                            // Handle POV / Hat Switches
                                            let axis_name = format_axis_name(axis_code);
                                            if axis_name == "ABS_HAT0Y" {
                                                if val == -1 {
                                                    send_hat_event(&etx, &dev_name, "HAT0_UP", "PRESS");
                                                } else if val == 1 {
                                                    send_hat_event(&etx, &dev_name, "HAT0_DOWN", "PRESS");
                                                } else if val == 0 {
                                                    send_hat_event(&etx, &dev_name, "HAT0_Y", "CENTER");
                                                }
                                            } else if axis_name == "ABS_HAT0X" {
                                                if val == 1 {
                                                    send_hat_event(&etx, &dev_name, "HAT0_RIGHT", "PRESS");
                                                } else if val == -1 {
                                                    send_hat_event(&etx, &dev_name, "HAT0_LEFT", "PRESS");
                                                } else if val == 0 {
                                                    send_hat_event(&etx, &dev_name, "HAT0_X", "CENTER");
                                                }
                                            } else {
                                                // Analog flight axis (ABS_X, ABS_Y, ABS_Z, ABS_RX, ABS_RY, ABS_RZ, ABS_THROTTLE, etc.)
                                                let (min, max, last_val) = axis_cache.get(&axis_code.0).copied().unwrap_or((0, 65535, val));
                                                let span = (max - min).abs();
                                                let deadband = (span / 256).max(1);

                                                if (val - last_val).abs() >= deadband || val == min || val == max {
                                                    axis_cache.insert(axis_code.0, (min, max, val));

                                                    let normalized = if min < 0 {
                                                        (val as f64 / max.max(1) as f64).clamp(-1.0, 1.0)
                                                    } else {
                                                        if span > 0 {
                                                            ((val - min) as f64 / span as f64).clamp(0.0, 1.0)
                                                        } else {
                                                            0.0
                                                        }
                                                    };

                                                    let _ = etx.send(GuiEvent::AxisMoved {
                                                        device: dev_name.clone(),
                                                        bound_to: axis_name.clone(),
                                                        raw_value: val,
                                                        normalized,
                                                        min,
                                                        max,
                                                        time: Instant::now(),
                                                    });

                                                    let _ = etx.send(GuiEvent::InputTriggered {
                                                        device: dev_name.clone(),
                                                        bound_to: axis_name.clone(),
                                                        action: format!("{:.1}%", normalized * 100.0),
                                                        time: Instant::now(),
                                                    });
                                                }
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            Err(e) => {
                                if !stop.load(Ordering::Relaxed) {
                                    debug!("evdev fetch_events error on {}: {}", dev_name, e);
                                    std::thread::sleep(std::time::Duration::from_millis(50));
                                }
                            }
                        }
                    }
                });
            }
        }
    }
}

fn send_hat_event(etx: &mpsc::UnboundedSender<GuiEvent>, dev_name: &str, bound_to: &str, action: &str) {
    let _ = etx.send(GuiEvent::InputTriggered {
        device: dev_name.to_string(),
        bound_to: bound_to.to_string(),
        action: action.to_string(),
        time: Instant::now(),
    });
}

fn format_key_name(key: evdev::KeyCode) -> String {
    let raw = format!("{:?}", key);
    if raw.starts_with("unknown key: ") {
        if let Ok(code) = raw.strip_prefix("unknown key: ").unwrap().trim().parse::<u16>() {
            format!("BUTTON_{}", code)
        } else {
            raw
        }
    } else {
        raw
    }
}

fn format_axis_name(axis: evdev::AbsoluteAxisCode) -> String {
    format!("{:?}", axis)
}

fn chrono_now_str() -> String {
    use std::time::SystemTime;
    let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs();
    let hours = (now / 3600) % 24;
    let mins = (now / 60) % 60;
    let secs = now % 60;
    format!("{:02}:{:02}:{:02}", hours, mins, secs)
}
