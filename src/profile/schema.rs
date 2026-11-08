use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename = "Profile")]
pub struct SpadProfile {
    #[serde(rename = "@Version", default)]
    pub version: Option<i32>,

    #[serde(rename = "@IsStockProfile", default)]
    pub is_stock_profile: Option<bool>,

    #[serde(rename = "Name", default)]
    pub name: Option<String>,

    #[serde(rename = "Variables", default)]
    pub variables: Option<ProfileVariablesList>,

    #[serde(rename = "Options", default)]
    pub options: Option<ProfileOptionsList>,

    #[serde(rename = "AssignedAircrafts", default)]
    pub assigned_aircrafts: Option<AssignedAircraftsList>,

    #[serde(rename = "Devices", default)]
    pub devices: Option<DeviceList>,

    #[serde(rename = "ProfileInformation", default)]
    pub profile_info: Option<ProfileInformation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProfileInformation {
    #[serde(rename = "@ProfileID", default)]
    pub profile_id: Option<String>,

    #[serde(rename = "@Version", default)]
    pub version: Option<String>,

    #[serde(rename = "Author", default)]
    pub author: Option<String>,

    #[serde(rename = "Description", default)]
    pub description: Option<String>,

    #[serde(rename = "LastChanged", default)]
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

    #[serde(rename = "@Scope", default)]
    pub scope: Option<String>,

    #[serde(rename = "@Value", default)]
    pub value: Option<String>,

    #[serde(rename = "@Type", default)]
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

    #[serde(rename = "@Value", default)]
    pub value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeviceList {
    #[serde(rename = "Device", default)]
    pub items: Vec<DeviceProfile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceProfile {
    #[serde(rename = "@VendorID", default)]
    pub vendor_id: Option<String>,

    #[serde(rename = "@ProductID", default)]
    pub product_id: Option<String>,

    #[serde(rename = "@DevicePath", default)]
    pub device_path: Option<String>,

    #[serde(rename = "@Serial", default)]
    pub serial: Option<String>,

    #[serde(rename = "@DeviceIndex", default)]
    pub device_index: Option<i32>,

    #[serde(rename = "@Name", default)]
    pub name: Option<String>,

    #[serde(rename = "@ID", default)]
    pub id: Option<String>,

    #[serde(rename = "DevicePage", default)]
    pub pages: Vec<DevicePage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevicePage {
    #[serde(rename = "@ID", default)]
    pub id: Option<String>,

    #[serde(rename = "@IsDefaultPage", default)]
    pub is_default: Option<bool>,

    #[serde(rename = "@Name", default)]
    pub name: Option<String>,

    #[serde(rename = "Events", default)]
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

    #[serde(rename = "@ConfigID", default)]
    pub config_id: Option<String>,

    #[serde(rename = "EventDefinitions", default)]
    pub event_definitions: Option<EventDefinitionsList>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EventDefinitionsList {
    #[serde(rename = "EventDefinition", default)]
    pub items: Vec<EventDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventDefinition {
    #[serde(rename = "@Trigger", default)]
    pub trigger: Option<String>,

    #[serde(rename = "@Priority", default)]
    pub priority: Option<i32>,

    #[serde(rename = "@IsFinal", default)]
    pub is_final: Option<bool>,

    #[serde(rename = "@ConditionBinding", default)]
    pub condition_binding: Option<String>, // "All" or "Any"

    #[serde(rename = "EventConditions", default)]
    pub conditions: Option<EventConditionsList>,

    #[serde(rename = "EventActions", default)]
    pub actions: Option<EventActionsList>,
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

    #[serde(rename = "@Comparator", alias = "@ConditionComparator", default)]
    pub comparator: Option<String>, // Equals, Less, Greater, Unequal, Mask, Range, etc.

    #[serde(rename = "@Value", alias = "@ConditionTargetValue", default)]
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

    #[serde(rename = "@Value", default)]
    pub value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventActionCustomAxis {
    #[serde(rename = "@Target", alias = "@TargetDataDefinition", default)]
    pub target: String, // e.g. "SIMCONNECT:THROTTLE1_AXIS_SET_EX1"

    #[serde(rename = "@Value", default)]
    pub value: Option<String>,

    #[serde(rename = "Options", default)]
    pub options: Option<ProfileOptionsList>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventActionChangeValue {
    #[serde(rename = "@Target", alias = "@TargetDataDefinition", default)]
    pub target: String, // e.g. "LVAR:A32NX_AUTOPILOT_1_ACTIVE" or "LOCAL:FOO"

    #[serde(rename = "@Operation", default)]
    pub operation: Option<String>, // "Set", "Increment", "Decrement", "ToggleBit"

    #[serde(rename = "@Value", default)]
    pub value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventActionScript {
    #[serde(rename = "@Script", default)]
    pub script: Option<String>,

    #[serde(rename = "@Language", default)]
    pub language: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventActionDelay {
    #[serde(rename = "@Delay")]
    pub delay_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventActionDisplayValue {
    #[serde(rename = "@Display", default)]
    pub display: Option<String>,

    #[serde(rename = "@Value", default)]
    pub value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventActionLED {
    #[serde(rename = "@LED", default)]
    pub led: Option<String>,

    #[serde(rename = "@State", default)]
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
            "X_AXIS" | "AXIS_X" => "AXIS_X".to_string(),
            "Y_AXIS" | "AXIS_Y" => "AXIS_Y".to_string(),
            "Z_AXIS" | "AXIS_Z" => "AXIS_Z".to_string(),
            "RX_AXIS" | "AXIS_RX" => "AXIS_RX".to_string(),
            "RY_AXIS" | "AXIS_RY" => "AXIS_RY".to_string(),
            "RZ_AXIS" | "AXIS_RZ" => "AXIS_RZ".to_string(),
            "SLIDER_0" | "AXIS_THROTTLE" => "AXIS_THROTTLE".to_string(),
            "SLIDER_1" | "AXIS_RUDDER" => "AXIS_RUDDER".to_string(),
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
        let parse_hex = |s: &str| -> Option<u16> {
            let trimmed = s.trim().trim_start_matches("0x").trim_start_matches("0X");
            u16::from_str_radix(trimmed, 16).ok()
        };
        if let (Some(v_str), Some(p_str)) = (&self.vendor_id, &self.product_id) {
            if let (Some(v), Some(p)) = (parse_hex(v_str), parse_hex(p_str)) {
                return v == vendor_id && p == product_id;
            }
        }
        false
    }
}
