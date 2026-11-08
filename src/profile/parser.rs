use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

use super::schema::SpadProfile;

pub struct ProfileParser;

impl ProfileParser {
    pub fn parse_file<P: AsRef<Path>>(path: P) -> Result<SpadProfile> {
        let p = path.as_ref();
        let content = fs::read_to_string(p)
            .with_context(|| format!("Failed to read profile file at {:?}", p))?;
        let mut prof = Self::parse_str(&content)?;
        let mut needs_migration = false;

        if prof.name.as_deref().unwrap_or("").trim().is_empty() {
            if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                prof.name = Some(stem.replace('_', " "));
                needs_migration = true;
            }
        }

        if prof.version.is_none() {
            prof.version = Some(2);
            needs_migration = true;
        }

        if let Some(ref mut devs) = prof.devices {
            for dev in &mut devs.items {
                if dev.pages.is_empty() {
                    dev.pages.push(super::schema::DevicePage {
                        id: Some("Default".to_string()),
                        is_default: Some(true),
                        name: Some("Default".to_string()),
                        events: Some(super::schema::EventList { items: Vec::new() }),
                    });
                    needs_migration = true;
                }
            }
        }

        // If migration upgraded an older file, create a .bak backup if not already present
        if needs_migration {
            let bak_path = p.with_extension("xml.bak");
            if !bak_path.exists() {
                let _ = fs::copy(p, &bak_path);
            }
        }

        Ok(prof)
    }

    pub fn parse_str(xml_str: &str) -> Result<SpadProfile> {
        // Parse with quick-xml serde deserializer
        let mut profile: SpadProfile = quick_xml::de::from_str(xml_str)
            .context("Failed to parse SPAD.neXt XML profile")?;
        profile.consolidate_duplicate_devices();
        Ok(profile)
    }

    pub fn serialize_to_string(profile: &SpadProfile) -> Result<String> {
        let mut clean = profile.clone();
        if let Some(ref mut devs) = clean.devices {
            for dev in &mut devs.items {
                for page in &mut dev.pages {
                    if let Some(ref mut evs) = page.events {
                        for ev in &mut evs.items {
                            if let Some(ref mut defs) = ev.event_definitions {
                                for def in &mut defs.items {
                                    if let Some(ref mut acts) = def.actions {
                                        acts.items.retain(|a| !matches!(a, super::schema::EventActionItem::Unknown));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        let serialized = quick_xml::se::to_string_with_root("Profile", &clean)
            .context("Failed to serialize SPAD.neXt profile to XML")?;
        Ok(format!("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n{}", serialized))
    }

    pub fn save_file<P: AsRef<Path>>(path: P, profile: &SpadProfile) -> Result<()> {
        let p = path.as_ref();
        if let Some(parent) = p.parent() {
            if !parent.as_os_str().is_empty() {
                let _ = fs::create_dir_all(parent);
            }
        }
        let xml = Self::serialize_to_string(profile)?;
        fs::write(p, xml).with_context(|| format!("Failed to write profile file to {:?}", p))?;
        Ok(())
    }
}
