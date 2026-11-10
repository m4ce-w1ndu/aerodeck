//! PMDG aircraft support (737 for MSFS, 777 for MSFS) through the PMDG SDK:
//!
//! * **Events**: `PMDGNG3:EVT_MCP_HEADING_SELECTOR` is sent as SimConnect custom event
//!   `#<id>`, with PMDG mouse flags as the parameter (left click, wheel up/down).
//! * **Data**: the aircraft broadcasts its cockpit state in a client data area
//!   (`PMDG_NG3_Data`, `PMDG_777X_Data`) once `EnableDataBroadcast=1` is set in the
//!   aircraft's options file.
//!
//! Event IDs and data layout come from the SDK header installed with the aircraft.

pub mod sdk;

use anyhow::{anyhow, Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};
use tracing::{debug, info, warn};

use crate::simconnect::{RecvClientData, SimConnectClient};
pub use sdk::{PmdgFamily, PmdgSdk};

/// PMDG mouse flags (event parameters), from the SDK header.
pub const MOUSE_FLAG_LEFTSINGLE: u32 = 0x2000_0000;
pub const MOUSE_FLAG_LEFTRELEASE: u32 = 0x0002_0000;
pub const MOUSE_FLAG_WHEEL_UP: u32 = 0x0000_4000;
pub const MOUSE_FLAG_WHEEL_DOWN: u32 = 0x0000_2000;

/// `SIMCONNECT_CLIENT_DATA_PERIOD_ON_SET`
const CLIENT_DATA_PERIOD_ON_SET: u32 = 3;
/// `SIMCONNECT_CLIENT_DATA_REQUEST_FLAG_CHANGED`
const CLIENT_DATA_FLAG_CHANGED: u32 = 1;
const FIRST_PMDG_REQUEST_ID: u32 = 0x0003_0000;

/// An installed PMDG product.
#[derive(Debug, Clone)]
pub struct InstalledPmdg {
    pub family: PmdgFamily,
    /// Package folder name, e.g. `pmdg-aircraft-738`.
    pub package: String,
    pub header: PathBuf,
    /// `<Packages>/<package>/work/<options file>` (may not exist yet).
    pub options_file: PathBuf,
}

/// Finds installed PMDG aircraft with an SDK header in the MSFS Proton prefix.
pub fn discover_installed() -> Vec<InstalledPmdg> {
    let Some(roaming) = crate::proton::ProtonConfigurator::find_msfs_roaming_path(None) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (_, root) in crate::profile::AircraftScanner::find_packages_directories(&roaming) {
        let Ok(entries) = std::fs::read_dir(&root) else { continue };
        let mut packages: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        packages.sort();
        for pkg in packages {
            let Some(name) = pkg.file_name().and_then(|n| n.to_str()).map(str::to_string) else { continue };
            if !name.to_ascii_lowercase().starts_with("pmdg-aircraft-") {
                continue;
            }
            for family in PmdgFamily::ALL {
                let header = pkg.join("Documentation").join("SDK").join(family.header_file());
                if header.is_file() {
                    out.push(InstalledPmdg {
                        family,
                        options_file: roaming.join("Packages").join(&name).join("work").join(family.options_file()),
                        package: name.clone(),
                        header,
                    });
                }
            }
        }
    }
    out
}

/// Loaded SDK per family (first installed package wins), cached for the process lifetime.
pub fn installed_sdks() -> &'static [Arc<PmdgSdk>] {
    static CACHE: OnceLock<Vec<Arc<PmdgSdk>>> = OnceLock::new();
    CACHE.get_or_init(|| {
        let mut sdks: Vec<Arc<PmdgSdk>> = Vec::new();
        for inst in discover_installed() {
            if sdks.iter().any(|s| s.family == inst.family) {
                continue;
            }
            match PmdgSdk::load(inst.family, &inst.header) {
                Ok(sdk) => {
                    info!(
                        "{} SDK loaded from {}: {} events, {} data fields ({} bytes)",
                        inst.family.label(),
                        inst.package,
                        sdk.events.len(),
                        sdk.fields.len(),
                        sdk.data_size
                    );
                    sdks.push(Arc::new(sdk));
                }
                Err(e) => warn!("Cannot read {}: {:#}", inst.header.display(), e),
            }
        }
        sdks
    })
}

/// True when the options file has `EnableDataBroadcast=1` in its `[SDK]` section.
pub fn broadcast_enabled(options_file: &Path) -> bool {
    std::fs::read_to_string(options_file)
        .map(|t| sdk_section_value(&t, "EnableDataBroadcast").as_deref() == Some("1"))
        .unwrap_or(false)
}

fn sdk_section_value(text: &str, key: &str) -> Option<String> {
    let mut in_sdk = false;
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            in_sdk = l.eq_ignore_ascii_case("[SDK]");
        } else if in_sdk {
            if let Some((k, v)) = l.split_once('=') {
                if k.trim().eq_ignore_ascii_case(key) {
                    return Some(v.trim().to_string());
                }
            }
        }
    }
    None
}

/// Sets `[SDK] EnableDataBroadcast=1` in the aircraft options file, keeping everything
/// else (and the file's line endings) intact. A `.bak` copy is written first.
/// Returns `false` if it was already enabled. Takes effect the next time the aircraft loads.
pub fn enable_broadcast(options_file: &Path) -> Result<bool> {
    let original = match std::fs::read_to_string(options_file) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e).with_context(|| format!("reading {}", options_file.display())),
    };
    if sdk_section_value(&original, "EnableDataBroadcast").as_deref() == Some("1") {
        return Ok(false);
    }
    let updated = set_sdk_key(&original, "EnableDataBroadcast", "1");

    let dir = options_file.parent().ok_or_else(|| anyhow!("invalid options path"))?;
    if !dir.is_dir() {
        return Err(anyhow!(
            "{} does not exist yet: load the aircraft in MSFS once, then try again",
            dir.display()
        ));
    }
    if !original.is_empty() {
        std::fs::write(options_file.with_extension("ini.bak"), &original)?;
    }
    std::fs::write(options_file, updated).with_context(|| format!("writing {}", options_file.display()))?;
    Ok(true)
}

fn set_sdk_key(text: &str, key: &str, value: &str) -> String {
    let nl = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let mut sdk_start = None;
    let mut sdk_end = lines.len();
    for (i, l) in lines.iter().enumerate() {
        let t = l.trim();
        if t.starts_with('[') {
            if sdk_start.is_some() {
                sdk_end = i;
                break;
            }
            if t.eq_ignore_ascii_case("[SDK]") {
                sdk_start = Some(i);
            }
        }
    }
    let entry = format!("{}={}", key, value);
    match sdk_start {
        Some(start) => {
            let existing = (start + 1..sdk_end).find(|&i| {
                lines[i].split_once('=').is_some_and(|(k, _)| k.trim().eq_ignore_ascii_case(key))
            });
            match existing {
                Some(i) => lines[i] = entry,
                None => lines.insert(start + 1, entry),
            }
        }
        None => {
            if lines.last().is_some_and(|l| !l.trim().is_empty()) {
                lines.push(String::new());
            }
            lines.push("[SDK]".to_string());
            lines.push(entry);
        }
    }
    let mut out = lines.join(nl);
    out.push_str(nl);
    out
}

/// Live PMDG data, fed by the SimConnect reader task.
#[derive(Default)]
pub struct PmdgStore {
    slots: RwLock<Vec<Slot>>,
}

struct Slot {
    sdk: Arc<PmdgSdk>,
    request_id: u32,
    data: Option<Vec<u8>>,
}

/// Status line for the UI.
#[derive(Debug, Clone)]
pub struct PmdgStatus {
    pub family: PmdgFamily,
    pub events: usize,
    pub fields: usize,
    pub receiving_data: bool,
}

impl PmdgStore {
    pub fn apply(&self, cd: &RecvClientData) {
        let Ok(mut slots) = self.slots.write() else { return };
        if let Some(slot) = slots.iter_mut().find(|s| s.request_id == cd.request_id) {
            if slot.data.is_none() {
                info!("Receiving {} cockpit data", slot.sdk.family.label());
            }
            slot.data = Some(cd.data.clone());
        }
    }

    fn sdk(&self, family: PmdgFamily) -> Option<Arc<PmdgSdk>> {
        self.slots.read().ok()?.iter().find(|s| s.sdk.family == family).map(|s| s.sdk.clone())
    }

    fn contains(&self, family: PmdgFamily) -> bool {
        self.sdk(family).is_some()
    }
}

impl SimConnectClient {
    /// Registers every installed PMDG SDK on this connection (idempotent).
    pub async fn ensure_pmdg(&self) -> Result<()> {
        for sdk in installed_sdks() {
            if self.pmdg_store.contains(sdk.family) {
                continue;
            }
            let request_id = {
                let mut slots = self.pmdg_store.slots.write().map_err(|_| anyhow!("PMDG store poisoned"))?;
                let id = FIRST_PMDG_REQUEST_ID + slots.len() as u32;
                slots.push(Slot { sdk: sdk.clone(), request_id: id, data: None });
                id
            };
            debug!("Registering {} data area '{}'", sdk.family.label(), sdk.data_name);
            self.map_client_data_name_to_id(&sdk.data_name, sdk.data_id).await?;
            self.add_to_client_data_definition(sdk.data_definition, 0, sdk.data_size as u32).await?;
            self.request_client_data_with_flags(
                sdk.data_id,
                request_id,
                sdk.data_definition,
                CLIENT_DATA_PERIOD_ON_SET,
                CLIENT_DATA_FLAG_CHANGED,
            )
            .await?;
        }
        Ok(())
    }

    /// Sends a PMDG event, e.g. (`PMDGNG3`, `EVT MCP HEADING SELECTOR`, MOUSE_FLAG_WHEEL_UP).
    pub async fn transmit_pmdg_event(&self, prefix: &str, name: &str, parameter: u32) -> Result<()> {
        let family = PmdgFamily::from_profile_prefix(prefix).ok_or_else(|| anyhow!("unknown PMDG prefix '{}'", prefix))?;
        self.ensure_pmdg().await?;
        let sdk = self
            .pmdg_store
            .sdk(family)
            .ok_or_else(|| anyhow!("{} SDK header not found in the MSFS installation", family.label()))?;
        let id = sdk
            .event_id(name)
            .ok_or_else(|| anyhow!("event '{}' not in the {} SDK", name, family.label()))?;
        self.transmit_event(&format!("#{}", id), parameter).await
    }

    /// Current value of a PMDG data field (`MCP_Course_0`), if the aircraft is broadcasting.
    pub fn pmdg_value(&self, prefix: &str, name: &str) -> Option<String> {
        let family = PmdgFamily::from_profile_prefix(prefix)?;
        let slots = self.pmdg_store.slots.read().ok()?;
        let slot = slots.iter().find(|s| s.sdk.family == family)?;
        let data = slot.data.as_ref()?;
        let (field, idx) = slot.sdk.find_field(name)?;
        slot.sdk.read_field(data, field, idx).map(|v| v.as_string())
    }

    /// All received PMDG values, keyed `FAMILY_PREFIX:FIELD` (upper-case, e.g. `PMDGNG3:MCP_COURSE_0`).
    pub fn pmdg_snapshot(&self) -> HashMap<String, String> {
        let mut out = HashMap::new();
        let Ok(slots) = self.pmdg_store.slots.read() else { return out };
        for slot in slots.iter() {
            let Some(data) = &slot.data else { continue };
            let prefix = slot.sdk.family.profile_prefixes()[0];
            for (k, v) in slot.sdk.decode_all(data) {
                out.insert(format!("{}:{}", prefix, k), v.as_string());
            }
        }
        out
    }

    pub fn pmdg_status(&self) -> Vec<PmdgStatus> {
        self.pmdg_store
            .slots
            .read()
            .map(|slots| {
                slots
                    .iter()
                    .map(|s| PmdgStatus {
                        family: s.sdk.family,
                        events: s.sdk.events.len(),
                        fields: s.sdk.fields.len(),
                        receiving_data: s.data.is_some(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// Key under which [`SimConnectClient::pmdg_snapshot`] stores a profile reference.
pub fn snapshot_key(prefix: &str, name: &str) -> Option<String> {
    let family = PmdgFamily::from_profile_prefix(prefix)?;
    Some(format!("{}:{}", family.profile_prefixes()[0], name.trim().to_ascii_uppercase()))
}

/// Default parameter for a PMDG event when the profile gives none: knobs turn with the
/// mouse wheel, everything else is a left click (release on button release).
pub fn default_parameter(trigger: &str) -> u32 {
    let t = trigger.to_ascii_uppercase().replace(['_', ' '], "");
    if t.contains("COUNTERCLOCKWISE") || t.ends_with("CCW") {
        MOUSE_FLAG_WHEEL_DOWN
    } else if t.contains("CLOCKWISE") || t.ends_with("CW") {
        MOUSE_FLAG_WHEEL_UP
    } else if t.contains("RELEASE") {
        MOUSE_FLAG_LEFTRELEASE
    } else {
        MOUSE_FLAG_LEFTSINGLE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_file_editing() {
        let crlf = "[ADIRU]\r\nLastPosValid=0\r\n[EFB]\r\nX=1\r\n";
        let out = set_sdk_key(crlf, "EnableDataBroadcast", "1");
        assert!(out.ends_with("[SDK]\r\nEnableDataBroadcast=1\r\n"), "{:?}", out);
        assert!(out.starts_with("[ADIRU]\r\nLastPosValid=0\r\n"));
        assert_eq!(sdk_section_value(&out, "EnableDataBroadcast").as_deref(), Some("1"));

        let existing = "[SDK]\nEnableDataBroadcast=0\nEnableCDUBroadcast.0=1\n[Other]\nA=1\n";
        let out = set_sdk_key(existing, "EnableDataBroadcast", "1");
        assert_eq!(out, "[SDK]\nEnableDataBroadcast=1\nEnableCDUBroadcast.0=1\n[Other]\nA=1\n");

        assert_eq!(set_sdk_key("", "EnableDataBroadcast", "1"), "[SDK]\nEnableDataBroadcast=1\n");
    }

    #[test]
    fn default_parameters_follow_trigger() {
        assert_eq!(default_parameter("TUNER_CLOCKWISE"), MOUSE_FLAG_WHEEL_UP);
        assert_eq!(default_parameter("TUNER_COUNTERCLOCKWISE"), MOUSE_FLAG_WHEEL_DOWN);
        assert_eq!(default_parameter("PRESS"), MOUSE_FLAG_LEFTSINGLE);
        assert_eq!(default_parameter("RELEASE"), MOUSE_FLAG_LEFTRELEASE);
    }
}
