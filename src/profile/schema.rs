use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename = "Profile")]
pub struct SpadProfile {
    #[serde(rename = "@Version", default, deserialize_with = "lenient::opt_num", skip_serializing_if = "Option::is_none")]
    pub version: Option<i32>,

    #[serde(rename = "@IsStockProfile", default, deserialize_with = "lenient::opt_bool", skip_serializing_if = "Option::is_none")]
    pub is_stock_profile: Option<bool>,

    #[serde(rename = "Name", default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    #[serde(rename = "Variables", default, skip_serializing_if = "Option::is_none")]
    pub variables: Option<ProfileVariablesList>,

    #[serde(rename = "Options", default, skip_serializing_if = "Option::is_none")]
    pub options: Option<ProfileOptionsList>,

    #[serde(rename = "AssignedAircrafts", default, skip_serializing_if = "Option::is_none")]
    pub assigned_aircrafts: Option<AssignedAircraftsList>,

    #[serde(rename = "Devices", default, skip_serializing_if = "Option::is_none")]
    pub devices: Option<DeviceList>,

    #[serde(rename = "ProfileInformation", default, skip_serializing_if = "Option::is_none")]
    pub profile_info: Option<ProfileInformation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProfileInformation {
    #[serde(rename = "@ProfileID", default, skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,

    #[serde(rename = "@Version", default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    #[serde(rename = "Author", default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,

    #[serde(rename = "Description", default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(rename = "LastChanged", default, skip_serializing_if = "Option::is_none")]
    pub last_changed: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AssignedAircraftsList {
    #[serde(rename = "Aircraft", default)]
    pub items: Vec<AssignedAircraftEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssignedAircraftEntry {
    #[serde(rename = "@Name")]
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProfileVariablesList {
    #[serde(rename = "Variable", default)]
    pub items: Vec<ProfileVariable>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileVariable {
    #[serde(rename = "@Name")]
    pub name: String,

    #[serde(rename = "@Scope", default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,

    #[serde(rename = "@Value", default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,

    #[serde(rename = "@Type", default, skip_serializing_if = "Option::is_none")]
    pub var_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProfileOptionsList {
    #[serde(rename = "Option", default)]
    pub items: Vec<ProfileOption>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileOption {
    #[serde(rename = "@Key")]
    pub key: String,

    #[serde(rename = "@Value", default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeviceList {
    #[serde(rename = "Device", default)]
    pub items: Vec<DeviceProfile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceProfile {
    #[serde(rename = "@VendorID", default, skip_serializing_if = "Option::is_none")]
    pub vendor_id: Option<String>,

    #[serde(rename = "@ProductID", default, skip_serializing_if = "Option::is_none")]
    pub product_id: Option<String>,

    #[serde(rename = "@DevicePath", default, skip_serializing_if = "Option::is_none")]
    pub device_path: Option<String>,

    #[serde(rename = "@Serial", default, skip_serializing_if = "Option::is_none")]
    pub serial: Option<String>,

    #[serde(rename = "@DeviceIndex", default, deserialize_with = "lenient::opt_num", skip_serializing_if = "Option::is_none")]
    pub device_index: Option<i32>,

    #[serde(rename = "@Name", default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    #[serde(rename = "@ID", default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    #[serde(rename = "DevicePage", default)]
    pub pages: Vec<DevicePage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevicePage {
    #[serde(rename = "@ID", default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    #[serde(rename = "@IsDefaultPage", default, deserialize_with = "lenient::opt_bool", skip_serializing_if = "Option::is_none")]
    pub is_default: Option<bool>,

    #[serde(rename = "@Name", default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    #[serde(rename = "Events", default, skip_serializing_if = "Option::is_none")]
    pub events: Option<EventList>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EventList {
    #[serde(rename = "Event", default)]
    pub items: Vec<SpadEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpadEvent {
    #[serde(rename = "@BoundTo")]
    pub bound_to: String,

    #[serde(rename = "@ConfigID", default, skip_serializing_if = "Option::is_none")]
    pub config_id: Option<String>,

    #[serde(rename = "EventDefinitions", default, skip_serializing_if = "Option::is_none")]
    pub event_definitions: Option<EventDefinitionsList>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EventDefinitionsList {
    #[serde(rename = "EventDefinition", default)]
    pub items: Vec<EventDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventDefinition {
    #[serde(rename = "@Trigger", default, skip_serializing_if = "Option::is_none")]
    pub trigger: Option<String>,

    #[serde(rename = "@Priority", default, deserialize_with = "lenient::opt_num", skip_serializing_if = "Option::is_none")]
    pub priority: Option<i32>,

    #[serde(rename = "@IsFinal", default, deserialize_with = "lenient::opt_bool", skip_serializing_if = "Option::is_none")]
    pub is_final: Option<bool>,

    #[serde(rename = "@ConditionBinding", default, skip_serializing_if = "Option::is_none")]
    pub condition_binding: Option<String>, // "All" or "Any"

    #[serde(rename = "EventConditions", default, skip_serializing_if = "Option::is_none")]
    pub conditions: Option<EventConditionsList>,

    #[serde(rename = "EventActions", default, skip_serializing_if = "Option::is_none")]
    pub actions: Option<EventActionsList>,

    /// Per-definition options. Axis trigger zones use `ZONEMIN` / `ZONEMAX` (percent of
    /// travel) and `HYSTERESIS` (percent) with the `ZONE_ENTER` / `ZONE_LEAVE` triggers.
    #[serde(rename = "Options", default, skip_serializing_if = "Option::is_none")]
    pub options: Option<ProfileOptionsList>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EventConditionsList {
    #[serde(rename = "$value", default)]
    pub items: Vec<EventConditionItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventConditionItem {
    #[serde(rename = "EventCondition", alias = "EventConditionSimple")]
    Simple(EventConditionSimple),

    #[serde(rename = "EventConditionExpression")]
    Expression(EventConditionExpression),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventConditionSimple {
    #[serde(rename = "@Target", alias = "@ConditionValueSource", default)]
    pub target: String,

    #[serde(rename = "@Comparator", alias = "@ConditionComparator", default, skip_serializing_if = "Option::is_none")]
    pub comparator: Option<String>, // Equals, Less, Greater, Unequal, Mask, Range, etc.

    #[serde(rename = "@Value", alias = "@ConditionTargetValue", default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventConditionExpression {
    #[serde(rename = "@Expression")]
    pub expression: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EventActionsList {
    #[serde(rename = "$value", default)]
    pub items: Vec<EventActionItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventActionItem {
    #[serde(rename = "EventActionControl")]
    Control(EventActionControl),

    #[serde(rename = "EventActionCustomAxis")]
    CustomAxis(EventActionCustomAxis),

    #[serde(rename = "EventActionRangedAxis")]
    RangedAxis(EventActionRangedAxis),

    #[serde(rename = "EventActionChangeValue")]
    ChangeValue(EventActionChangeValue),

    #[serde(rename = "EventActionScript")]
    Script(EventActionScript),

    #[serde(rename = "EventActionDelay")]
    Delay(EventActionDelay),

    #[serde(rename = "EventActionDisplayValue")]
    DisplayValue(EventActionDisplayValue),

    #[serde(rename = "EventActionLED")]
    Led(EventActionLED),

    #[serde(rename = "EventActionCommand")]
    Command(EventActionCommand),

    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventActionControl {
    #[serde(rename = "@Target", alias = "@TargetDataDefinition", default)]
    pub target: String, // e.g. "SIMCONNECT:AP_ALT_VAR_INC"

    #[serde(rename = "@Value", default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,

    /// `<Parameter Value="..."/>` children (SPAD.neXt writes event parameters this way).
    #[serde(rename = "Parameter", default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<ControlParameter>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlParameter {
    #[serde(rename = "@Value", default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

impl EventActionControl {
    /// The event parameter: the `Value` attribute, else the first `<Parameter>`.
    pub fn effective_value(&self) -> Option<&str> {
        self.value
            .as_deref()
            .filter(|v| !v.trim().is_empty())
            .or_else(|| self.parameters.iter().find_map(|p| p.value.as_deref()))
    }
}

/// SPAD.neXt calibrated lever (`LEVER_NOREVERSE_THROTTLE`, `LEVER_MIXTURE`, `LEVER_FLAPS`, ...).
/// Options: `MIN`/`MINTO`, `IDLE`/`IDLETO`, `MAX`/`MAXTO` (zone edges, raw device values when
/// `USERAWVALUE=True`) and `OPTION1..4` (engines driven by this lever).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventActionRangedAxis {
    #[serde(rename = "@ConfigID", default)]
    pub config_id: String,

    #[serde(rename = "Options", default, skip_serializing_if = "Option::is_none")]
    pub options: Option<ProfileOptionsList>,
}

impl EventActionRangedAxis {
    pub fn option(&self, key: &str) -> Option<&str> {
        self.options
            .as_ref()?
            .items
            .iter()
            .find(|o| o.key.eq_ignore_ascii_case(key))
            .and_then(|o| o.value.as_deref())
    }

    pub fn option_f64(&self, key: &str) -> Option<f64> {
        self.option(key).and_then(|v| v.trim().parse().ok())
    }

    pub fn option_bool(&self, key: &str) -> bool {
        self.option(key).is_some_and(|v| v.trim().eq_ignore_ascii_case("true"))
    }

    pub fn set_option(&mut self, key: &str, value: impl Into<String>) {
        let value = value.into();
        let opts = self.options.get_or_insert_with(|| ProfileOptionsList { items: Vec::new() });
        match opts.items.iter_mut().find(|o| o.key.eq_ignore_ascii_case(key)) {
            Some(o) => o.value = Some(value),
            None => opts.items.push(ProfileOption { key: key.to_string(), value: Some(value) }),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventActionCustomAxis {
    #[serde(rename = "@Target", alias = "@TargetDataDefinition", default)]
    pub target: String, // e.g. "SIMCONNECT:THROTTLE1_AXIS_SET_EX1"

    #[serde(rename = "@Value", default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,

    #[serde(rename = "Options", default, skip_serializing_if = "Option::is_none")]
    pub options: Option<ProfileOptionsList>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventActionChangeValue {
    #[serde(rename = "@Target", alias = "@TargetDataDefinition", default)]
    pub target: String, // e.g. "LVAR:A32NX_AUTOPILOT_1_ACTIVE" or "LOCAL:FOO"

    #[serde(rename = "@Operation", alias = "@ValueOperation", default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<String>, // "Set", "Increment", "Decrement", "ToggleBit"

    #[serde(rename = "@Value", default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventActionScript {
    #[serde(rename = "@Script", default, skip_serializing_if = "Option::is_none")]
    pub script: Option<String>,

    #[serde(rename = "@Language", default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventActionDelay {
    #[serde(rename = "@Delay", default, deserialize_with = "lenient::num_or_zero")]
    pub delay_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventActionDisplayValue {
    #[serde(rename = "@Display", default, skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,

    #[serde(rename = "@Value", default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,

    /// Data source shown on the display, e.g. `LVAR:ngx_SPDwindow` or `SIMCONNECT:NAV ACTIVE FREQUENCY:1`.
    #[serde(rename = "@TargetDataDefinition", default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,

    /// Number format mask, e.g. `000`, `0000`, `-----`.
    #[serde(rename = "@DisplayFormat", default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,

    #[serde(rename = "@DisplayFormatting", default, skip_serializing_if = "Option::is_none")]
    pub formatting: Option<String>,

    /// Input whose display this writes to, when not the event's own input.
    #[serde(rename = "@TargetName", default, skip_serializing_if = "Option::is_none")]
    pub target_name: Option<String>,

    #[serde(rename = "@TargetDisplay", default, skip_serializing_if = "Option::is_none")]
    pub target_display: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventActionLED {
    #[serde(rename = "@LED", default, skip_serializing_if = "Option::is_none")]
    pub led: Option<String>,

    #[serde(rename = "@State", default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventActionCommand {
    #[serde(rename = "@Command", default)]
    pub command: String,
}

/// Checks if a SPAD.neXt `BoundTo` string matches a Linux evdev input name
/// (e.g. Windows SPAD `X_AXIS` <-> Linux `AXIS_X`, `Z_AXIS` <-> `AXIS_Z`, etc.)
pub fn bound_to_matches(profile_bound: &str, input_name: &str) -> bool {
    if profile_bound.eq_ignore_ascii_case(input_name) {
        return true;
    }
    let norm = |s: &str| -> String {
        match s.to_uppercase().as_str() {
            // SPAD.neXt (DirectInput) names <-> Linux evdev names
            "X_AXIS" | "AXIS_X" | "ABS_X" => "AXIS_X".to_string(),
            "Y_AXIS" | "AXIS_Y" | "ABS_Y" => "AXIS_Y".to_string(),
            "Z_AXIS" | "AXIS_Z" | "ABS_Z" => "AXIS_Z".to_string(),
            "RX_AXIS" | "AXIS_RX" | "ABS_RX" => "AXIS_RX".to_string(),
            "RY_AXIS" | "AXIS_RY" | "ABS_RY" => "AXIS_RY".to_string(),
            "RZ_AXIS" | "AXIS_RZ" | "ABS_RZ" => "AXIS_RZ".to_string(),
            "SLIDER_0" | "AXIS_THROTTLE" | "ABS_THROTTLE" => "AXIS_THROTTLE".to_string(),
            "SLIDER_1" | "AXIS_RUDDER" | "ABS_RUDDER" => "AXIS_RUDDER".to_string(),
            other => other.to_string(),
        }
    };
    norm(profile_bound) == norm(input_name)
}

impl DeviceProfile {
    /// Checks if this `<Device>` entry matches a connected hardware ID/name or VID/PID
    pub fn matches_device_id(&self, device_id: &str) -> bool {
        if self.id.as_deref() == Some(device_id)
            || self.product_id.as_deref() == Some(device_id)
            || self.name.as_deref() == Some(device_id)
        {
            return true;
        }
        let clean_hex = |s: &str| {
            s.trim()
                .trim_start_matches("0x")
                .trim_start_matches("0X")
                .to_lowercase()
        };
        if let (Some(vid), Some(pid)) = (&self.vendor_id, &self.product_id) {
            let v = clean_hex(vid);
            let p = clean_hex(pid);
            let dev_lower = device_id.to_lowercase();
            if dev_lower.contains(&format!("{}_{}", v, p))
                || dev_lower.contains(&format!("{}:{}", v, p))
            {
                return true;
            }
        }
        false
    }

    pub fn matches_vid_pid(&self, vendor_id: u16, product_id: u16) -> bool {
        if let Some((v, p)) = self.parsed_vid_pid() {
            return v == vendor_id && p == product_id;
        }
        false
    }

    pub fn parsed_vid_pid(&self) -> Option<(u16, u16)> {
        let parse_hex = |s: &str| -> Option<u16> {
            let trimmed = s.trim().trim_start_matches("0x").trim_start_matches("0X");
            u16::from_str_radix(trimmed, 16).ok()
        };
        if let (Some(v_str), Some(p_str)) = (&self.vendor_id, &self.product_id) {
            if let (Some(v), Some(p)) = (parse_hex(v_str), parse_hex(p_str)) {
                return Some((v, p));
            }
        }
        None
    }

    /// Number of inputs (`<Event>`) on this device that have at least one configured action
    pub fn mapped_input_count(&self) -> usize {
        let mut count = 0;
        for page in &self.pages {
            if let Some(ref evs) = page.events {
                for ev in &evs.items {
                    let has_actions = ev
                        .event_definitions
                        .as_ref()
                        .map(|defs| {
                            defs.items.iter().any(|d| {
                                d.actions
                                    .as_ref()
                                    .map(|a| !a.items.is_empty())
                                    .unwrap_or(false)
                            })
                        })
                        .unwrap_or(false);
                    if has_actions {
                        count += 1;
                    }
                }
            }
        }
        count
    }

    /// Total number of actions across all inputs on this device
    pub fn total_action_count(&self) -> usize {
        let mut total = 0;
        for page in &self.pages {
            if let Some(ref evs) = page.events {
                for ev in &evs.items {
                    if let Some(ref defs) = ev.event_definitions {
                        for def in &defs.items {
                            if let Some(ref acts) = def.actions {
                                total += acts.items.len();
                            }
                        }
                    }
                }
            }
        }
        total
    }

    /// Human-friendly name for this `<Device>` entry even when SPAD v2 XML omits `Name`
    pub fn friendly_device_name(&self) -> String {
        if let Some(ref n) = self.name {
            if !n.trim().is_empty() {
                return n.clone();
            }
        }
        if let Some((vid, pid)) = self.parsed_vid_pid() {
            match (vid, pid) {
                (0x4098, 0xBC27) => "WINCTRL URSA MINOR 32 Joystick L".to_string(),
                (0x4098, 0xBC28) => "WINCTRL URSA MINOR 32 Joystick R".to_string(),
                (0x4098, 0xBE62) => "Winwing Orion Throttle Base II".to_string(),
                (0x4098, 0xBEE0) | (0x4098, 0xBB10) => "Winwing FCU / EFIS Panel".to_string(),
                (0x044F, 0x0407) => "Thrustmaster TCA Q-Eng 1&2".to_string(),
                (0x044F, 0x0405) => "Thrustmaster TCA Sidestick Airbus".to_string(),
                (0x044F, 0xB68F) => "Thrustmaster T-Pendular-Rudder".to_string(),
                (0x06A3, 0x0D05) => "Logitech / Saitek Pro Flight Radio Panel".to_string(),
                (0x06A3, 0x0D06) => "Logitech / Saitek Pro Flight Multi Panel".to_string(),
                (0x06A3, 0x0D67) => "Logitech / Saitek Pro Flight Switch Panel".to_string(),
                (0x06A3, 0x0C2D) => "Logitech / Saitek Pro Flight Throttle Quadrant".to_string(),
                (0x06A3, 0x0BAC) => "Logitech / Saitek Pro Flight Yoke".to_string(),
                (0x06A3, 0x0763) => "Logitech / Saitek Pro Flight Rudder Pedals".to_string(),
                (0x294B, 0x1900) => "Honeycomb Alpha Flight Controls".to_string(),
                (0x294B, 0x1901) => "Honeycomb Bravo Throttle Quadrant".to_string(),
                _ => format!("Controller {:04X}:{:04X}", vid, pid),
            }
        } else {
            self.id
                .clone()
                .unwrap_or_else(|| "Unknown Controller".to_string())
        }
    }
}

#[derive(Debug, Clone)]
pub struct MergeSummary {
    pub merged_device_names: Vec<String>,
    pub total_devices_in_profile: usize,
    pub total_events_merged: usize,
}

impl SpadProfile {
    /// Finds the `<Device>` block in this profile that matches the given hardware controller
    pub fn find_device_for_connected(
        &self,
        vendor_id: u16,
        product_id: u16,
        device_id: &str,
    ) -> Option<&DeviceProfile> {
        let devs = self.devices.as_ref()?;
        // Prefer exact VID:PID or ID match that has events, or first match
        devs.items
            .iter()
            .filter(|d| d.matches_vid_pid(vendor_id, product_id) || d.matches_device_id(device_id))
            .max_by_key(|d| d.total_action_count())
    }

    /// Finds or creates the `<Device>` block in this profile for the given hardware controller
    pub fn ensure_device_for_connected(
        &mut self,
        vendor_id: u16,
        product_id: u16,
        device_id: &str,
        device_name: &str,
        device_path: Option<&str>,
    ) -> &mut DeviceProfile {
        if self.devices.is_none() {
            self.devices = Some(DeviceList { items: Vec::new() });
        }
        let devs = self.devices.as_mut().unwrap();

        let existing_idx = devs
            .items
            .iter()
            .enumerate()
            .filter(|(_, d)| {
                d.matches_vid_pid(vendor_id, product_id) || d.matches_device_id(device_id)
            })
            .max_by_key(|(_, d)| d.total_action_count())
            .map(|(idx, _)| idx);

        let idx = match existing_idx {
            Some(i) => {
                if devs.items[i].name.is_none() {
                    devs.items[i].name = Some(device_name.to_string());
                }
                i
            }
            None => {
                devs.items.push(DeviceProfile {
                    vendor_id: Some(format!("0x{:04X}", vendor_id)),
                    product_id: Some(format!("0x{:04X}", product_id)),
                    device_path: device_path.map(|s| s.to_string()),
                    serial: None,
                    device_index: Some(0),
                    name: Some(device_name.to_string()),
                    id: Some(device_id.to_string()),
                    pages: vec![DevicePage {
                        id: Some("Default".to_string()),
                        is_default: Some(true),
                        name: Some("Default".to_string()),
                        events: Some(EventList { items: Vec::new() }),
                    }],
                });
                devs.items.len() - 1
            }
        };

        &mut devs.items[idx]
    }

    /// Merges device mappings from `incoming` into `self` without losing other devices in `self`.
    ///
    /// - If `target_device` is `Some((vid, pid, dev_id, dev_name))`, imports `incoming`'s device
    ///   mappings specifically into that target controller inside `self` (retargeting VID:PID if
    ///   `incoming` was authored for a single device or another variant).
    /// - If `target_device` is `None`, merges all non-empty `<Device>` entries from `incoming`
    ///   into `self` matched by VID:PID, preserving any other devices already present in `self`.
    pub fn merge_from_profile(
        &mut self,
        incoming: &SpadProfile,
        target_device: Option<(u16, u16, &str, &str)>,
        replace_existing_device_events: bool,
    ) -> MergeSummary {
        // 1. Merge Variables
        if let Some(ref inc_vars) = incoming.variables {
            let vars = self
                .variables
                .get_or_insert_with(|| ProfileVariablesList { items: Vec::new() });
            for v in &inc_vars.items {
                if !vars.items.iter().any(|ev| ev.name == v.name) {
                    vars.items.push(v.clone());
                }
            }
        }

        // 2. Merge AssignedAircrafts
        if let Some(ref inc_acs) = incoming.assigned_aircrafts {
            let acs = self
                .assigned_aircrafts
                .get_or_insert_with(|| AssignedAircraftsList { items: Vec::new() });
            for ac in &inc_acs.items {
                if !acs
                    .items
                    .iter()
                    .any(|ea| ea.name.eq_ignore_ascii_case(&ac.name))
                {
                    acs.items.push(ac.clone());
                }
            }
        }

        if self.devices.is_none() {
            self.devices = Some(DeviceList { items: Vec::new() });
        }

        let mut merged_device_names = Vec::new();
        let mut total_events_merged = 0;

        let inc_devs = incoming
            .devices
            .as_ref()
            .map(|d| d.items.as_slice())
            .unwrap_or(&[]);

        if let Some((target_vid, target_pid, target_id, target_name)) = target_device {
            // Find matching device(s) in `incoming` by VID:PID, or if none match, take all devices with actions
            let matching_inc: Vec<&DeviceProfile> = inc_devs
                .iter()
                .filter(|d| d.matches_vid_pid(target_vid, target_pid) && d.total_action_count() > 0)
                .collect();

            let sources_to_apply: Vec<&DeviceProfile> = if !matching_inc.is_empty() {
                matching_inc
            } else {
                let non_empty: Vec<&DeviceProfile> = inc_devs
                    .iter()
                    .filter(|d| d.total_action_count() > 0)
                    .collect();
                if !non_empty.is_empty() {
                    non_empty
                } else {
                    inc_devs.iter().collect()
                }
            };

            let target_dev = self.ensure_device_for_connected(
                target_vid,
                target_pid,
                target_id,
                target_name,
                None,
            );
            if target_dev.pages.is_empty() {
                target_dev.pages.push(DevicePage {
                    id: Some("Default".to_string()),
                    is_default: Some(true),
                    name: Some("Default".to_string()),
                    events: Some(EventList { items: Vec::new() }),
                });
            }
            if replace_existing_device_events {
                for p in &mut target_dev.pages {
                    if let Some(ref mut evs) = p.events {
                        evs.items.clear();
                    }
                }
            }

            let target_events = target_dev.pages[0]
                .events
                .get_or_insert_with(|| EventList { items: Vec::new() });

            for src_dev in sources_to_apply {
                for src_page in &src_dev.pages {
                    if let Some(ref src_evs) = src_page.events {
                        for src_ev in &src_evs.items {
                            total_events_merged += 1;
                            if let Some(existing) = target_events
                                .items
                                .iter_mut()
                                .find(|e| bound_to_matches(&e.bound_to, &src_ev.bound_to))
                            {
                                *existing = src_ev.clone();
                            } else {
                                target_events.items.push(src_ev.clone());
                            }
                        }
                    }
                }
            }
            merged_device_names.push(format!(
                "{} (0x{:04X}:0x{:04X})",
                target_name, target_vid, target_pid
            ));
        } else {
            // Auto-merge all devices from `incoming` into `self` by VID:PID
            for src_dev in inc_devs {
                let src_actions = src_dev.total_action_count();
                let dev_label = src_dev.friendly_device_name();
                if let Some((vid, pid)) = src_dev.parsed_vid_pid() {
                    // Check if self already has a device for this VID:PID
                    let existing_has_actions = self
                        .find_device_for_connected(vid, pid, "")
                        .map(|d| d.total_action_count() > 0)
                        .unwrap_or(false);

                    // Do not overwrite an existing populated device with an empty stub device
                    if src_actions == 0 && existing_has_actions {
                        continue;
                    }

                    let dev_id = src_dev
                        .id
                        .clone()
                        .unwrap_or_else(|| format!("DEV_{:04X}_{:04X}", vid, pid));
                    let target_dev =
                        self.ensure_device_for_connected(vid, pid, &dev_id, &dev_label, None);
                    if target_dev.pages.is_empty() {
                        target_dev.pages.push(DevicePage {
                            id: Some("Default".to_string()),
                            is_default: Some(true),
                            name: Some("Default".to_string()),
                            events: Some(EventList { items: Vec::new() }),
                        });
                    }
                    if replace_existing_device_events && src_actions > 0 {
                        for p in &mut target_dev.pages {
                            if let Some(ref mut evs) = p.events {
                                evs.items.clear();
                            }
                        }
                    }
                    let target_events = target_dev.pages[0]
                        .events
                        .get_or_insert_with(|| EventList { items: Vec::new() });

                    for src_page in &src_dev.pages {
                        if let Some(ref src_evs) = src_page.events {
                            for src_ev in &src_evs.items {
                                total_events_merged += 1;
                                if let Some(existing) = target_events
                                    .items
                                    .iter_mut()
                                    .find(|e| bound_to_matches(&e.bound_to, &src_ev.bound_to))
                                {
                                    *existing = src_ev.clone();
                                } else {
                                    target_events.items.push(src_ev.clone());
                                }
                            }
                        }
                    }
                    if src_actions > 0 || !merged_device_names.contains(&dev_label) {
                        merged_device_names.push(format!(
                            "{} (0x{:04X}:0x{:04X})",
                            dev_label, vid, pid
                        ));
                    }
                } else {
                    // Device without VID:PID (e.g. custom ID)
                    let devs = self.devices.as_mut().unwrap();
                    devs.items.push(src_dev.clone());
                    merged_device_names.push(dev_label);
                }
            }
        }

        // Deduplicate devices with identical VID:PID in `self.devices` so all events for a VID:PID live in one clean `<Device>` entry
        self.consolidate_duplicate_devices();

        let total_devices_in_profile = self
            .devices
            .as_ref()
            .map(|d| d.items.iter().filter(|dev| dev.total_action_count() > 0).count().max(d.items.len()))
            .unwrap_or(0);

        MergeSummary {
            merged_device_names,
            total_devices_in_profile,
            total_events_merged,
        }
    }

    /// Consolidates multiple `<Device>` entries that share the same `(VendorID, ProductID)`
    /// into a single `<Device>` entry so all mappings for that controller are unified.
    pub fn consolidate_duplicate_devices(&mut self) {
        let Some(ref mut devs) = self.devices else {
            return;
        };
        let mut consolidated: Vec<DeviceProfile> = Vec::new();
        for dev in devs.items.drain(..) {
            if let Some((vid, pid)) = dev.parsed_vid_pid() {
                if let Some(existing) = consolidated
                    .iter_mut()
                    .find(|d| d.matches_vid_pid(vid, pid))
                {
                    if existing.name.is_none() && dev.name.is_some() {
                        existing.name = dev.name.clone();
                    }
                    if existing.pages.is_empty() {
                        existing.pages = dev.pages;
                    } else {
                        let target_events = existing.pages[0]
                            .events
                            .get_or_insert_with(|| EventList { items: Vec::new() });
                        for page in dev.pages {
                            if let Some(evs) = page.events {
                                for ev in evs.items {
                                    if let Some(ex_ev) = target_events
                                        .items
                                        .iter_mut()
                                        .find(|e| bound_to_matches(&e.bound_to, &ev.bound_to))
                                    {
                                        // Same input in both entries: keep the definitions of both
                                        // (e.g. hat north in one, south in the other), skipping
                                        // exact duplicates.
                                        let target_defs = ex_ev
                                            .event_definitions
                                            .get_or_insert_with(|| EventDefinitionsList { items: Vec::new() });
                                        for def in ev.event_definitions.map(|d| d.items).unwrap_or_default() {
                                            let dup = target_defs.items.iter().any(|t| format!("{:?}", t) == format!("{:?}", def));
                                            if !dup {
                                                target_defs.items.push(def);
                                            }
                                        }
                                    } else {
                                        target_events.items.push(ev);
                                    }
                                }
                            }
                        }
                    }
                    continue;
                }
            }
            consolidated.push(dev);
        }
        devs.items = consolidated;
    }

    /// Checks if this profile matches an aircraft title reported by MSFS
    pub fn matches_aircraft_title(&self, sim_title: &str) -> bool {
        let sim_clean = sim_title.trim();
        if sim_clean.is_empty() {
            return false;
        }
        if let Some(ref assigned) = self.assigned_aircrafts {
            let sim_lower = sim_clean.to_lowercase();
            for item in &assigned.items {
                let pat = item.name.trim().to_lowercase();
                if pat.is_empty() {
                    continue;
                }
                if pat == "*" || pat == sim_lower {
                    return true;
                }
                if pat.contains('*') {
                    let parts: Vec<&str> = pat.split('*').collect();
                    if parts.len() == 2 {
                        let (prefix, suffix) = (parts[0], parts[1]);
                        if sim_lower.starts_with(prefix) && sim_lower.ends_with(suffix) {
                            return true;
                        }
                    }
                }
                if sim_lower.contains(&pat) || pat.contains(&sim_lower) {
                    return true;
                }
            }
        }
        false
    }

    /// Returns true if this profile is configured as a fallback profile (has '*' pattern)
    pub fn is_fallback_profile(&self) -> bool {
        self.aircraft_patterns().iter().any(|p| p.trim() == "*")
    }

    /// Sets whether this profile acts as a generic fallback profile
    pub fn set_fallback_profile(&mut self, fallback: bool) {
        if fallback {
            self.add_aircraft_pattern("*");
        } else {
            self.remove_aircraft_pattern("*");
        }
    }

    /// Returns list of aircraft patterns assigned to this profile
    pub fn aircraft_patterns(&self) -> Vec<String> {
        self.assigned_aircrafts
            .as_ref()
            .map(|l| l.items.iter().map(|a| a.name.clone()).collect())
            .unwrap_or_default()
    }

    /// Adds an aircraft pattern to this profile
    pub fn add_aircraft_pattern(&mut self, pattern: &str) {
        let clean = pattern.trim().to_string();
        if clean.is_empty() {
            return;
        }
        let list = self
            .assigned_aircrafts
            .get_or_insert_with(|| AssignedAircraftsList { items: Vec::new() });
        if !list.items.iter().any(|a| a.name.eq_ignore_ascii_case(&clean)) {
            list.items.push(AssignedAircraftEntry { name: clean });
        }
    }

    /// Removes an aircraft pattern from this profile
    pub fn remove_aircraft_pattern(&mut self, pattern: &str) {
        if let Some(ref mut list) = self.assigned_aircrafts {
            list.items.retain(|a| !a.name.eq_ignore_ascii_case(pattern.trim()));
            if list.items.is_empty() {
                self.assigned_aircrafts = None;
            }
        }
    }

    /// Replaces the layout of a specific device inside this profile with a new layout (one-time copy).
    /// Returns the previous DeviceProfile for one-step undo.
    pub fn replace_device_layout(
        &mut self,
        vendor_id: u16,
        product_id: u16,
        mut new_layout: DeviceProfile,
    ) -> Option<DeviceProfile> {
        let devs = self.devices.get_or_insert_with(|| DeviceList { items: Vec::new() });
        let prev_idx = devs.items.iter().position(|d| d.matches_vid_pid(vendor_id, product_id));
        let prev = prev_idx.map(|idx| devs.items.remove(idx));

        new_layout.vendor_id = Some(format!("0x{:04X}", vendor_id));
        new_layout.product_id = Some(format!("0x{:04X}", product_id));
        devs.items.push(new_layout);
        prev
    }

    /// Clears all bindings for a specific device, keeping the device entry with an empty page
    pub fn clear_device_layout(&mut self, vendor_id: u16, product_id: u16) -> Option<DeviceProfile> {
        let devs = self.devices.as_mut()?;
        let d = devs.items.iter_mut().find(|d| d.matches_vid_pid(vendor_id, product_id))?;
        let prev = d.clone();
        for page in &mut d.pages {
            page.events = Some(EventList { items: Vec::new() });
        }
        Some(prev)
    }

    /// Removes a device from this profile entirely
    pub fn remove_device(&mut self, vendor_id: u16, product_id: u16) -> Option<DeviceProfile> {
        let devs = self.devices.as_mut()?;
        let idx = devs.items.iter().position(|d| d.matches_vid_pid(vendor_id, product_id))?;
        Some(devs.items.remove(idx))
    }

    /// Ensures a device entry exists in this profile for the given hardware controller
    pub fn add_device_for_hardware(&mut self, vendor_id: u16, product_id: u16, name: &str) {
        let devs = self.devices.get_or_insert_with(|| DeviceList { items: Vec::new() });
        if !devs.items.iter().any(|d| d.matches_vid_pid(vendor_id, product_id)) {
            devs.items.push(DeviceProfile {
                vendor_id: Some(format!("0x{:04X}", vendor_id)),
                product_id: Some(format!("0x{:04X}", product_id)),
                device_path: None,
                serial: None,
                device_index: Some(0),
                name: Some(name.to_string()),
                id: Some(format!("DEV_{:04X}_{:04X}", vendor_id, product_id)),
                pages: vec![DevicePage {
                    id: Some("Default".to_string()),
                    is_default: Some(true),
                    name: Some("Default".to_string()),
                    events: Some(EventList { items: Vec::new() }),
                }],
            });
        }
    }
}

/// SPAD.neXt writes empty attributes (`IsFinal=""`, `Priority=""`) and capitalised booleans
/// (`True`); these deserializers treat empty as absent instead of failing the whole profile.
mod lenient {
    use serde::{Deserialize, Deserializer};
    use std::str::FromStr;

    pub fn opt_bool<'de, D: Deserializer<'de>>(d: D) -> Result<Option<bool>, D::Error> {
        let s = Option::<String>::deserialize(d)?;
        Ok(s.and_then(|s| match s.trim().to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" => Some(true),
            "false" | "0" | "no" => Some(false),
            _ => None,
        }))
    }

    pub fn opt_num<'de, D: Deserializer<'de>, T: FromStr>(d: D) -> Result<Option<T>, D::Error> {
        let s = Option::<String>::deserialize(d)?;
        Ok(s.and_then(|s| s.trim().parse::<T>().ok()))
    }

    pub fn num_or_zero<'de, D: Deserializer<'de>, T: FromStr + Default>(d: D) -> Result<T, D::Error> {
        Ok(opt_num(d)?.unwrap_or_default())
    }
}

/// Axis trigger zone of a `ZONE_ENTER` / `ZONE_LEAVE` definition, in percent of travel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AxisZone {
    pub min: f64,
    pub max: f64,
    pub hysteresis: f64,
    /// Gap (percent) on both sides of the zone where the input's proportional axis output
    /// is frozen, so a zone at the end of the travel (spoiler ARM) does not move the axis.
    /// `None`: the zone only fires triggers and the axis output is untouched.
    pub deadzone: Option<f64>,
}

impl AxisZone {
    pub const DEFAULT_HYSTERESIS: f64 = 3.0;

    pub fn of(def: &EventDefinition) -> Option<Self> {
        let t = def.trigger.as_deref()?.to_ascii_uppercase();
        if t != "ZONE_ENTER" && t != "ZONE_LEAVE" {
            return None;
        }
        let get = |k: &str| {
            def.options
                .as_ref()?
                .items
                .iter()
                .find(|o| o.key.eq_ignore_ascii_case(k))
                .and_then(|o| o.value.as_deref())
                .and_then(|v| v.trim().parse::<f64>().ok())
        };
        let (a, b) = (get("ZONEMIN")?, get("ZONEMAX")?);
        Some(Self {
            min: a.min(b).clamp(0.0, 100.0),
            max: a.max(b).clamp(0.0, 100.0),
            hysteresis: get("HYSTERESIS").unwrap_or(Self::DEFAULT_HYSTERESIS).max(0.0),
            deadzone: get("DEADZONE").map(|d| d.max(0.0)),
        })
    }

    pub fn to_options(self) -> ProfileOptionsList {
        let o = |k: &str, v: f64| ProfileOption { key: k.to_string(), value: Some(format!("{}", v)) };
        let mut items = vec![o("ZONEMIN", self.min), o("ZONEMAX", self.max), o("HYSTERESIS", self.hysteresis)];
        if let Some(d) = self.deadzone {
            items.push(o("DEADZONE", d));
        }
        ProfileOptionsList { items }
    }

    /// Travel (percent) where the proportional axis output is frozen: the zone plus its
    /// deadzone, and at least the hysteresis band so the zone is left (e.g. spoilers
    /// disarmed) before the axis starts moving again.
    pub fn frozen_band(self) -> Option<(f64, f64)> {
        let pad = self.deadzone?.max(self.hysteresis);
        Some(((self.min - pad).max(0.0), (self.max + pad).min(100.0)))
    }

    /// Next inside/outside state for position `pos` (percent): entering needs the zone
    /// itself, leaving needs to pass the hysteresis band, so the edge does not chatter.
    pub fn next_state(self, inside: bool, pos: f64) -> bool {
        if inside {
            pos >= self.min - self.hysteresis && pos <= self.max + self.hysteresis
        } else {
            pos >= self.min && pos <= self.max
        }
    }

    /// Key shared by the ENTER and LEAVE definitions of the same zone.
    pub fn key(self) -> String {
        format!("{:.2}-{:.2}", self.min, self.max)
    }
}

/// Maps an axis position (percent) onto `0.0..=1.0` over the travel left outside the frozen
/// `bands`, so the remaining range still covers the full output. Returns the output and
/// whether the position is inside a band (where the output stays at the band's edge).
pub fn axis_outside_bands(pos: f64, bands: &[(f64, f64)]) -> (f64, bool) {
    let pos = pos.clamp(0.0, 100.0);
    let mut merged: Vec<(f64, f64)> = bands.iter().copied().filter(|(a, b)| b > a).collect();
    merged.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut joined: Vec<(f64, f64)> = Vec::new();
    for (a, b) in merged {
        match joined.last_mut() {
            Some(last) if a <= last.1 => last.1 = last.1.max(b),
            _ => joined.push((a, b)),
        }
    }
    let frozen: f64 = joined.iter().map(|(a, b)| b - a).sum();
    let free = 100.0 - frozen;
    let below: f64 = joined.iter().map(|(a, b)| (pos.min(*b) - a).max(0.0)).sum();
    let inside = joined.iter().any(|(a, b)| pos >= *a && pos <= *b);
    if free <= f64::EPSILON {
        return (0.0, inside);
    }
    (((pos - below) / free).clamp(0.0, 1.0), inside)
}
