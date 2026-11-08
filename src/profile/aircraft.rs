use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::info;

use crate::profile::{AssignedAircraftEntry, ProfileParser};
use crate::proton::ProtonConfigurator;

pub type DiscoveredAircraft = InstalledAircraft;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstalledAircraft {
    /// Exact SimConnect TITLE (e.g. "Airbus A320 Neo Asobo")
    pub title: String,
    /// Manufacturer (e.g. "Airbus", "Boeing", "Cessna")
    pub manufacturer: String,
    /// Model name (e.g. "A320neo", "747-8i", "172SP Skyhawk G1000")
    pub model: String,
    /// ICAO type designator (e.g. "A20N", "B748", "C172")
    pub icao: String,
    /// Category / Role (e.g. "Commercial Airliner", "Single Engine Prop", "Rotorcraft")
    pub category: String,
    /// SimObject folder name (e.g. "Asobo_A320_NEO") used to match SimConnect AircraftLoaded paths
    pub simobject_folder: String,
    /// Package source ("Official/Steam", "Official/OneStore", "Community", or "Built-in")
    pub package_source: String,
    /// True if aircraft config is encrypted in cfg.fsarchive (Premium/Deluxe)
    pub is_encrypted: bool,
}

impl InstalledAircraft {
    pub fn display_label(&self) -> String {
        if !self.icao.is_empty() {
            format!("{} {} [{}] ({})", self.manufacturer, self.model, self.icao, self.title)
        } else {
            format!("{} {} ({})", self.manufacturer, self.model, self.title)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AircraftAssignmentManager {
    /// Maps Aircraft Title -> Profile File Path
    pub assignments: HashMap<String, String>,
    /// Optional fallback profile for unassigned aircraft (SPAD.neXt PROFILE_MAKEDEFAULT parity)
    pub default_profile: Option<String>,
    /// Automatically switch active profile when MSFS aircraft changes
    pub auto_switch_enabled: bool,
}

impl Default for AircraftAssignmentManager {
    fn default() -> Self {
        Self {
            assignments: HashMap::new(),
            default_profile: Some("default_c172.xml".to_string()),
            auto_switch_enabled: true,
        }
    }
}

pub const ASSIGNMENTS_FILE: &str = "aircraft_assignments.json";

impl AircraftAssignmentManager {
    pub fn load_or_default() -> Self {
        let path = Path::new(ASSIGNMENTS_FILE);
        if let Ok(content) = fs::read_to_string(path) {
            if let Some(mgr) = Self::from_conf_str(&content) {
                return mgr;
            }
        }
        let mut mgr = Self::default();
        // Scan existing XML profiles in current dir and ./profiles for <AssignedAircrafts>
        mgr.import_from_xml_profiles();
        let _ = mgr.save();
        mgr
    }

    pub fn save(&self) -> Result<()> {
        let mut out = String::new();
        out.push_str(&format!("auto_switch_enabled={}\n", self.auto_switch_enabled));
        if let Some(ref def) = self.default_profile {
            out.push_str(&format!("default_profile={}\n", def));
        }
        let mut keys: Vec<_> = self.assignments.keys().collect();
        keys.sort();
        for k in keys {
            if let Some(v) = self.assignments.get(k) {
                out.push_str(&format!("bind:{}={}\n", k, v));
            }
        }
        fs::write(ASSIGNMENTS_FILE, out)?;
        Ok(())
    }

    fn from_conf_str(content: &str) -> Option<Self> {
        let mut assignments = HashMap::new();
        let mut default_profile = None;
        let mut auto_switch_enabled = true;
        let mut parsed_any = false;

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if let Some(val) = trimmed.strip_prefix("auto_switch_enabled=") {
                auto_switch_enabled = val.trim().eq_ignore_ascii_case("true");
                parsed_any = true;
            } else if let Some(val) = trimmed.strip_prefix("default_profile=") {
                let v = val.trim();
                if !v.is_empty() {
                    default_profile = Some(v.to_string());
                }
                parsed_any = true;
            } else if let Some(rest) = trimmed.strip_prefix("bind:") {
                if let Some((ac, prof)) = rest.split_once('=') {
                    let ac_clean = ac.trim();
                    let prof_clean = prof.trim();
                    if !ac_clean.is_empty() && !prof_clean.is_empty() {
                        assignments.insert(ac_clean.to_string(), prof_clean.to_string());
                        parsed_any = true;
                    }
                }
            }
        }

        if parsed_any {
            Some(Self {
                assignments,
                default_profile,
                auto_switch_enabled,
            })
        } else {
            None
        }
    }

    /// Scans local `.xml` profiles for `<AssignedAircrafts><Aircraft Name="..."/></AssignedAircrafts>`
    pub fn import_from_xml_profiles(&mut self) {
        let mut xml_files = Vec::new();
        if let Ok(entries) = fs::read_dir(".") {
            for e in entries.flatten() {
                if e.path().extension().and_then(|s| s.to_str()) == Some("xml") {
                    xml_files.push(e.path());
                }
            }
        }
        if let Ok(entries) = fs::read_dir("profiles") {
            for e in entries.flatten() {
                if e.path().extension().and_then(|s| s.to_str()) == Some("xml") {
                    xml_files.push(e.path());
                }
            }
        }

        for xml_path in xml_files {
            if let Ok(prof) = ProfileParser::parse_file(&xml_path) {
                if let Some(assigned) = prof.assigned_aircrafts {
                    let path_str = xml_path
                        .strip_prefix("./")
                        .unwrap_or(&xml_path)
                        .to_string_lossy()
                        .to_string();
                    for item in assigned.items {
                        if !item.name.trim().is_empty() {
                            self.assignments
                                .entry(item.name.trim().to_string())
                                .or_insert_with(|| path_str.clone());
                        }
                    }
                }
            }
        }
    }

    /// Binds multiple aircraft titles to a profile path in a single batch
    pub fn assign_aircrafts(&mut self, aircraft_titles: &[String], profile_path: &str) -> Result<()> {
        let clean_profile = profile_path.trim().to_string();
        for ac in aircraft_titles {
            let clean_title = ac.trim().to_string();
            if !clean_title.is_empty() {
                self.assignments.insert(clean_title, clean_profile.clone());
            }
        }
        self.save()?;

        // Also persist inside the XML profile's <AssignedAircrafts> tag if the file exists
        let p = Path::new(&clean_profile);
        if p.exists() {
            if let Ok(mut prof) = ProfileParser::parse_file(p) {
                let mut list = prof.assigned_aircrafts.unwrap_or_default();
                for ac in aircraft_titles {
                    let clean_title = ac.trim();
                    if !clean_title.is_empty()
                        && !list.items.iter().any(|a| a.name.eq_ignore_ascii_case(clean_title))
                    {
                        list.items.push(AssignedAircraftEntry {
                            name: clean_title.to_string(),
                        });
                    }
                }
                prof.assigned_aircrafts = Some(list);
                if let Ok(xml) = quick_xml::se::to_string_with_root("Profile", &prof) {
                    let full = format!("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n{}", xml);
                    let _ = fs::write(p, full);
                }
            }
        }

        Ok(())
    }

    /// Removes bindings for multiple aircraft titles in a single batch
    pub fn unassign_aircrafts(&mut self, aircraft_titles: &[String]) -> Result<()> {
        for ac in aircraft_titles {
            let _ = self.unassign_aircraft(ac);
        }
        Ok(())
    }

    /// Binds an aircraft title to a profile path and updates both `aircraft_assignments.json` and the profile XML
    pub fn assign_aircraft(&mut self, aircraft_title: &str, profile_path: &str) -> Result<()> {
        let clean_title = aircraft_title.trim().to_string();
        let clean_profile = profile_path.trim().to_string();
        self.assignments.insert(clean_title.clone(), clean_profile.clone());
        self.save()?;

        // Also persist inside the XML profile's <AssignedAircrafts> tag if the file exists
        let p = Path::new(&clean_profile);
        if p.exists() {
            if let Ok(mut prof) = ProfileParser::parse_file(p) {
                let mut list = prof.assigned_aircrafts.unwrap_or_default();
                if !list.items.iter().any(|a| a.name.eq_ignore_ascii_case(&clean_title)) {
                    list.items.push(AssignedAircraftEntry {
                        name: clean_title,
                    });
                }
                prof.assigned_aircrafts = Some(list);
                if let Ok(xml) = quick_xml::se::to_string_with_root("Profile", &prof) {
                    let full = format!("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n{}", xml);
                    let _ = fs::write(p, full);
                }
            }
        }

        Ok(())
    }

    /// Removes an aircraft binding
    pub fn unassign_aircraft(&mut self, aircraft_title: &str) -> Result<()> {
        let prev = self.assignments.remove(aircraft_title.trim());
        self.save()?;

        if let Some(profile_path) = prev {
            let p = Path::new(&profile_path);
            if p.exists() {
                if let Ok(mut prof) = ProfileParser::parse_file(p) {
                    if let Some(mut list) = prof.assigned_aircrafts {
                        list.items.retain(|a| !a.name.eq_ignore_ascii_case(aircraft_title.trim()));
                        prof.assigned_aircrafts = if list.items.is_empty() {
                            None
                        } else {
                            Some(list)
                        };
                        if let Ok(xml) = quick_xml::se::to_string_with_root("Profile", &prof) {
                            let full = format!("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n{}", xml);
                            let _ = fs::write(p, full);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Returns all aircraft titles bound to a specific profile path
    pub fn aircrafts_for_profile(&self, profile_path: &str) -> Vec<String> {
        let norm_target = profile_path.trim().trim_start_matches("./");
        let mut out: Vec<String> = self
            .assignments
            .iter()
            .filter(|(_, p)| p.trim().trim_start_matches("./") == norm_target)
            .map(|(ac, _)| ac.clone())
            .collect();
        out.sort();
        out
    }

    /// Resolves a SimConnect `AircraftLoaded` path (e.g. `SimObjects\AirPlanes\Asobo_A320_NEO\aircraft.cfg`)
    /// or `TITLE` string into the canonical aircraft title using the indexed aircraft list.
    pub fn normalize_aircraft_identifier(
        raw_input: &str,
        installed: &[InstalledAircraft],
    ) -> String {
        let trimmed = raw_input.trim().trim_matches('\0').trim();
        if trimmed.is_empty() {
            return String::new();
        }

        // Check if it looks like a SimObjects file path from RequestSystemState("AircraftLoaded")
        let normalized_slashes = trimmed.replace('\\', "/");
        if normalized_slashes.to_lowercase().contains("simobjects/")
            || normalized_slashes.to_lowercase().ends_with(".cfg")
        {
            let parts: Vec<&str> = normalized_slashes.split('/').collect();
            if parts.len() >= 2 {
                let folder = parts[parts.len() - 2];
                // Strip livery suffixes if present to match base or exact SimObject folder
                if let Some(found) = installed
                    .iter()
                    .find(|a| a.simobject_folder.eq_ignore_ascii_case(folder))
                {
                    return found.title.clone();
                }
                if let Some(found) = installed.iter().find(|a| {
                    folder
                        .to_lowercase()
                        .starts_with(&a.simobject_folder.to_lowercase())
                }) {
                    return found.title.clone();
                }
                return folder.to_string();
            }
        }

        trimmed.to_string()
    }

    /// Finds the profile bound to `aircraft_title` (supporting exact match, SimObject folder match,
    /// livery prefix match, and optional default profile fallback).
    pub fn resolve_profile_for_aircraft(
        &self,
        aircraft_title: &str,
        installed: &[InstalledAircraft],
    ) -> Option<PathBuf> {
        let canonical = Self::normalize_aircraft_identifier(aircraft_title, installed);
        if canonical.is_empty() {
            return None;
        }

        // 1. Exact case-insensitive match on title
        if let Some((_, prof)) = self
            .assignments
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(&canonical))
        {
            let p = PathBuf::from(prof);
            if p.exists() {
                return Some(p);
            }
        }

        // 2. Prefix / Livery family match (e.g. "Airbus A320 Neo Orbit" matches "Airbus A320 Neo Asobo" or "A320")
        let lower_canon = canonical.to_lowercase();
        for (bound_ac, prof) in &self.assignments {
            let lower_bound = bound_ac.to_lowercase();
            let base_bound = lower_bound.trim_end_matches(" asobo").trim();
            if !base_bound.is_empty()
                && (lower_canon.contains(base_bound) || lower_bound.contains(&lower_canon))
            {
                let p = PathBuf::from(prof);
                if p.exists() {
                    return Some(p);
                }
            }
        }

        // 3. Match by ICAO or Model from installed list
        if let Some(ac_info) = installed
            .iter()
            .find(|a| a.title.eq_ignore_ascii_case(&canonical) || a.simobject_folder.eq_ignore_ascii_case(&canonical))
        {
            for (bound_ac, prof) in &self.assignments {
                if let Some(bound_info) = installed.iter().find(|a| a.title.eq_ignore_ascii_case(bound_ac)) {
                    if (!ac_info.icao.is_empty() && ac_info.icao.eq_ignore_ascii_case(&bound_info.icao))
                        || (!ac_info.model.is_empty() && ac_info.model.eq_ignore_ascii_case(&bound_info.model))
                    {
                        let p = PathBuf::from(prof);
                        if p.exists() {
                            return Some(p);
                        }
                    }
                }
            }
        }

        // 4. Default fallback profile for unassigned aircraft
        if let Some(ref def) = self.default_profile {
            let p = PathBuf::from(def);
            if p.exists() {
                return Some(p);
            }
        }

        None
    }
}

pub struct AircraftScanner;

impl AircraftScanner {
    /// Discovers all installed MSFS aircraft from the user's Proton prefix (`Official/Steam`, `Official/OneStore`, `Community`)
    pub fn discover_installed_aircraft(custom_prefix: Option<PathBuf>) -> Vec<InstalledAircraft> {
        let mut aircraft_list = Vec::new();

        if let Some(roaming) = ProtonConfigurator::find_msfs_roaming_path(custom_prefix) {
            let packages_dirs = Self::find_packages_directories(&roaming);
            for (source_label, pkg_root) in packages_dirs {
                Self::scan_packages_folder(&pkg_root, &source_label, &mut aircraft_list);
            }
        }

        if aircraft_list.is_empty() {
            aircraft_list = Self::builtin_fallback_aircraft();
        } else {
            aircraft_list.sort_by(|a, b| {
                a.manufacturer
                    .to_lowercase()
                    .cmp(&b.manufacturer.to_lowercase())
                    .then_with(|| a.model.to_lowercase().cmp(&b.model.to_lowercase()))
                    .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
            });
            aircraft_list.dedup_by(|a, b| a.title.eq_ignore_ascii_case(&b.title));
            info!("Indexed {} installed MSFS aircraft from Packages", aircraft_list.len());
        }

        aircraft_list
    }

    fn find_packages_directories(roaming: &Path) -> Vec<(String, PathBuf)> {
        let mut roots = Vec::new();
        let mut base_packages = roaming.join("Packages");

        // Also parse UserCfg.opt for InstalledPackagesPath
        let user_cfg = roaming.join("UserCfg.opt");
        if let Ok(content) = fs::read_to_string(&user_cfg) {
            for line in content.lines().rev() {
                let trimmed = line.trim();
                if trimmed.starts_with("InstalledPackagesPath") {
                    let parts: Vec<&str> = trimmed.split('"').collect();
                    if parts.len() >= 2 {
                        let win_path = parts[1];
                        if let Some(pfx) = ProtonConfigurator::resolve_pfx_root(roaming) {
                            if let Some(rest) = win_path
                                .strip_prefix("C:\\")
                                .or_else(|| win_path.strip_prefix("c:\\"))
                            {
                                let mapped = pfx.join("drive_c").join(rest.replace('\\', "/"));
                                if mapped.exists() {
                                    base_packages = mapped;
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }

        let official_steam = base_packages.join("Official").join("Steam");
        if official_steam.exists() {
            roots.push(("Official/Steam".to_string(), official_steam));
        }
        let official_onestore = base_packages.join("Official").join("OneStore");
        if official_onestore.exists() {
            roots.push(("Official/OneStore".to_string(), official_onestore));
        }
        let community = base_packages.join("Community");
        if community.exists() {
            roots.push(("Community".to_string(), community));
        }

        roots
    }

    fn scan_packages_folder(pkg_root: &Path, source_label: &str, out: &mut Vec<InstalledAircraft>) {
        let entries = match fs::read_dir(pkg_root) {
            Ok(e) => e,
            Err(_) => return,
        };

        for entry in entries.flatten() {
            let pkg_path = entry.path();
            if !pkg_path.is_dir() {
                continue;
            }
            let pkg_name = match pkg_path.file_name().and_then(|s| s.to_str()) {
                Some(n) => n.to_lowercase(),
                None => continue,
            };

            // Skip pure livery packs and generic AI traffic packs to keep the aircraft selector clean
            if pkg_name.contains("-livery")
                || pkg_name.contains("generic-")
                || pkg_name.ends_with("-common")
            {
                continue;
            }

            // Find SimObjects/Airplanes or SimObjects/AirPlanes or SimObjects/Rotorcraft
            let simobjects_dir = Self::find_case_insensitive_subdir(&pkg_path, "SimObjects");
            let simobjects_dir = match simobjects_dir {
                Some(d) => d,
                None => continue,
            };

            let mut category_dirs = Vec::new();
            if let Some(d) = Self::find_case_insensitive_subdir(&simobjects_dir, "Airplanes") {
                category_dirs.push(d);
            }
            if let Some(d) = Self::find_case_insensitive_subdir(&simobjects_dir, "Rotorcraft") {
                category_dirs.push(d);
            }

            if category_dirs.is_empty() {
                continue;
            }

            let loc_map = Self::parse_en_us_locpak(&pkg_path);
            let (manifest_title, manifest_manufacturer) = Self::parse_manifest_json(&pkg_path);

            for cat_dir in category_dirs {
                let sub_entries = match fs::read_dir(&cat_dir) {
                    Ok(e) => e,
                    Err(_) => continue,
                };

                for sub in sub_entries.flatten() {
                    let ac_dir = sub.path();
                    if !ac_dir.is_dir() {
                        continue;
                    }
                    let simobject_folder = ac_dir
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("")
                        .to_string();

                    if let Some(cfg_path) = Self::find_case_insensitive_file(&ac_dir, "aircraft.cfg") {
                        Self::parse_aircraft_cfg(
                            &cfg_path,
                            &simobject_folder,
                            source_label,
                            &loc_map,
                            &manifest_title,
                            &manifest_manufacturer,
                            out,
                        );
                    } else if Self::find_case_insensitive_file(&ac_dir, "cfg.fsarchive").is_some() {
                        // Encrypted Premium/Deluxe aircraft (e.g., B787-10, C172 Classic, SR22, Longitude)
                        let manufacturer = loc_map
                            .get("AIRCRAFT.UI_MANUFACTURER")
                            .cloned()
                            .or_else(|| manifest_manufacturer.clone())
                            .unwrap_or_else(|| "Microsoft / Asobo".to_string());
                        let model = loc_map
                            .get("AIRCRAFT.UI_MODEL")
                            .cloned()
                            .or_else(|| manifest_title.clone())
                            .unwrap_or_else(|| simobject_folder.replace('_', " "));
                        let title = format!("{} {} Asobo", manufacturer, model);
                        out.push(InstalledAircraft {
                            title,
                            manufacturer,
                            model,
                            icao: String::new(),
                            category: "Premium / Deluxe Aircraft".to_string(),
                            simobject_folder,
                            package_source: source_label.to_string(),
                            is_encrypted: true,
                        });
                    }
                }
            }
        }
    }

    fn parse_aircraft_cfg(
        cfg_path: &Path,
        simobject_folder: &str,
        source_label: &str,
        loc_map: &HashMap<String, String>,
        manifest_title: &Option<String>,
        manifest_manufacturer: &Option<String>,
        out: &mut Vec<InstalledAircraft>,
    ) {
        let content = match fs::read_to_string(cfg_path) {
            Ok(c) => c,
            Err(_) => return,
        };

        let mut icao = String::new();
        let mut icao_mfg = String::new();
        let mut icao_model = String::new();

        let mut in_general = false;
        let mut in_fltsim = false;

        let mut cur_title = String::new();
        let mut cur_mfg = String::new();
        let mut cur_type = String::new();
        let mut cur_role = String::new();
        let mut cur_air_traffic = false;
        let mut cur_selectable = true;

        let flush_variation = |title: &str,
                               mfg: &str,
                               ui_type: &str,
                               role: &str,
                               air_traffic: bool,
                               selectable: bool,
                               icao: &str,
                               icao_mfg: &str,
                               icao_model: &str,
                               out: &mut Vec<InstalledAircraft>| {
            if title.is_empty() || air_traffic || !selectable || title.to_lowercase().contains("airtraffic") {
                return;
            }
            let resolved_mfg = Self::resolve_loc_value(mfg, loc_map)
                .or_else(|| manifest_manufacturer.clone())
                .unwrap_or_else(|| {
                    if !icao_mfg.is_empty() {
                        icao_mfg.to_string()
                    } else {
                        "Asobo / MSFS".to_string()
                    }
                });
            let resolved_model = Self::resolve_loc_value(ui_type, loc_map)
                .or_else(|| manifest_title.clone())
                .unwrap_or_else(|| {
                    if !icao_model.is_empty() {
                        icao_model.to_string()
                    } else {
                        title.to_string()
                    }
                });
            let resolved_role = if role.is_empty() {
                "Aircraft".to_string()
            } else {
                role.to_string()
            };

            out.push(InstalledAircraft {
                title: title.to_string(),
                manufacturer: resolved_mfg,
                model: resolved_model,
                icao: icao.to_uppercase(),
                category: resolved_role,
                simobject_folder: simobject_folder.to_string(),
                package_source: source_label.to_string(),
                is_encrypted: false,
            });
        };

        for raw_line in content.lines() {
            let line = raw_line.split(';').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                if in_fltsim {
                    flush_variation(
                        &cur_title,
                        &cur_mfg,
                        &cur_type,
                        &cur_role,
                        cur_air_traffic,
                        cur_selectable,
                        &icao,
                        &icao_mfg,
                        &icao_model,
                        out,
                    );
                }
                let sec = line[1..line.len() - 1].trim().to_uppercase();
                in_general = sec == "GENERAL";
                // Only index primary user-flyable [FLTSIM.0] variation per SimObject folder to avoid duplicate liveries
                in_fltsim = sec == "FLTSIM.0";
                cur_title.clear();
                cur_mfg.clear();
                cur_type.clear();
                cur_role.clear();
                cur_air_traffic = false;
                cur_selectable = true;
                continue;
            }

            if let Some((k, v)) = line.split_once('=') {
                let key = k.trim().to_lowercase();
                let val = v.trim().trim_matches('"').trim();
                if in_general {
                    match key.as_str() {
                        "icao_type_designator" => icao = val.to_string(),
                        "icao_manufacturer" => icao_mfg = val.to_string(),
                        "icao_model" => icao_model = val.to_string(),
                        _ => {}
                    }
                } else if in_fltsim {
                    match key.as_str() {
                        "title" => cur_title = val.to_string(),
                        "ui_manufacturer" => cur_mfg = val.to_string(),
                        "ui_type" => cur_type = val.to_string(),
                        "ui_typerole" => cur_role = val.to_string(),
                        "isairtraffic" => cur_air_traffic = val == "1" || val.eq_ignore_ascii_case("true"),
                        "isuserselectable" => cur_selectable = val != "0" && !val.eq_ignore_ascii_case("false"),
                        _ => {}
                    }
                }
            }
        }

        if in_fltsim {
            flush_variation(
                &cur_title,
                &cur_mfg,
                &cur_type,
                &cur_role,
                cur_air_traffic,
                cur_selectable,
                &icao,
                &icao_mfg,
                &icao_model,
                out,
            );
        }
    }

    fn resolve_loc_value(raw: &str, loc_map: &HashMap<String, String>) -> Option<String> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }
        if let Some(key) = trimmed.strip_prefix("TT:") {
            if let Some(resolved) = loc_map.get(key) {
                if !resolved.trim().is_empty() {
                    return Some(resolved.trim().to_string());
                }
            }
            return None;
        }
        if let Some(stripped) = trimmed.strip_prefix("$$:") {
            return Some(stripped.trim().to_string());
        }
        Some(trimmed.to_string())
    }

    fn parse_en_us_locpak(pkg_dir: &Path) -> HashMap<String, String> {
        let mut map = HashMap::new();
        let loc_path = match Self::find_case_insensitive_file(pkg_dir, "en-US.locPak") {
            Some(p) => p,
            None => return map,
        };
        let content = match fs::read_to_string(loc_path) {
            Ok(c) => c,
            Err(_) => return map,
        };
        if let Ok(re) = regex::Regex::new(r#""([^"]+)"\s*:\s*"([^"]*)""#) {
            for caps in re.captures_iter(&content) {
                if let (Some(k), Some(v)) = (caps.get(1), caps.get(2)) {
                    let key = k.as_str().trim();
                    if key.starts_with("AIRCRAFT.") || key.contains("ATCCOM.") {
                        map.insert(key.to_string(), v.as_str().trim().to_string());
                    }
                }
            }
        }
        map
    }

    fn parse_manifest_json(pkg_dir: &Path) -> (Option<String>, Option<String>) {
        let manifest_path = match Self::find_case_insensitive_file(pkg_dir, "manifest.json") {
            Some(p) => p,
            None => return (None, None),
        };
        let content = match fs::read_to_string(manifest_path) {
            Ok(c) => c,
            Err(_) => return (None, None),
        };
        let extract_field = |field: &str| -> Option<String> {
            let pattern = format!(r#""{}"\s*:\s*"([^"]*)""#, field);
            let re = regex::Regex::new(&pattern).ok()?;
            let caps = re.captures(&content)?;
            let val = caps.get(1)?.as_str().trim();
            if val.is_empty() {
                None
            } else {
                Some(val.to_string())
            }
        };
        (extract_field("title"), extract_field("manufacturer"))
    }

    fn find_case_insensitive_subdir(parent: &Path, target: &str) -> Option<PathBuf> {
        let entries = fs::read_dir(parent).ok()?;
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                if let Some(name) = p.file_name().and_then(|s| s.to_str()) {
                    if name.eq_ignore_ascii_case(target) {
                        return Some(p);
                    }
                }
            }
        }
        None
    }

    fn find_case_insensitive_file(parent: &Path, target: &str) -> Option<PathBuf> {
        let entries = fs::read_dir(parent).ok()?;
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file() {
                if let Some(name) = p.file_name().and_then(|s| s.to_str()) {
                    if name.eq_ignore_ascii_case(target) {
                        return Some(p);
                    }
                }
            }
        }
        None
    }

    pub fn builtin_fallback_aircraft() -> Vec<InstalledAircraft> {
        vec![
            InstalledAircraft {
                title: "Airbus A320 Neo Asobo".to_string(),
                manufacturer: "Airbus".to_string(),
                model: "A320neo".to_string(),
                icao: "A20N".to_string(),
                category: "Commercial Airliner".to_string(),
                simobject_folder: "Asobo_A320_NEO".to_string(),
                package_source: "Built-in".to_string(),
                is_encrypted: false,
            },
            InstalledAircraft {
                title: "Boeing 747-8i Asobo".to_string(),
                manufacturer: "Boeing".to_string(),
                model: "747-8 Intercontinental".to_string(),
                icao: "B748".to_string(),
                category: "Commercial Airliner".to_string(),
                simobject_folder: "Asobo_B747_8i".to_string(),
                package_source: "Built-in".to_string(),
                is_encrypted: false,
            },
            InstalledAircraft {
                title: "Cessna Skyhawk G1000 Asobo".to_string(),
                manufacturer: "Cessna".to_string(),
                model: "172SP Skyhawk G1000".to_string(),
                icao: "C172".to_string(),
                category: "Single Engine Prop".to_string(),
                simobject_folder: "Asobo_C172sp_AS1000".to_string(),
                package_source: "Built-in".to_string(),
                is_encrypted: false,
            },
            InstalledAircraft {
                title: "Cessna 208B Grand Caravan EX".to_string(),
                manufacturer: "Cessna".to_string(),
                model: "208B Grand Caravan EX".to_string(),
                icao: "C208".to_string(),
                category: "Single Engine Turboprop".to_string(),
                simobject_folder: "Asobo_208B_GRAND_CARAVAN_EX".to_string(),
                package_source: "Built-in".to_string(),
                is_encrypted: false,
            },
            InstalledAircraft {
                title: "Daher TBM 930 Asobo".to_string(),
                manufacturer: "Daher".to_string(),
                model: "TBM 930".to_string(),
                icao: "TBM9".to_string(),
                category: "Single Engine Turboprop".to_string(),
                simobject_folder: "Asobo_TBM930".to_string(),
                package_source: "Built-in".to_string(),
                is_encrypted: false,
            },
        ]
    }
}
