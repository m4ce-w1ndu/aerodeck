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
        if prof.name.as_deref().unwrap_or("").trim().is_empty() {
            if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                prof.name = Some(stem.replace('_', " "));
            }
        }
        Ok(prof)
    }

    pub fn parse_str(xml_str: &str) -> Result<SpadProfile> {
        // Parse with quick-xml serde deserializer
        let profile: SpadProfile = quick_xml::de::from_str(xml_str)
            .context("Failed to parse SPAD.neXt XML profile")?;
        Ok(profile)
    }

    pub fn serialize_to_string(profile: &SpadProfile) -> Result<String> {
        let serialized = quick_xml::se::to_string_with_root("Profile", profile)
            .context("Failed to serialize SPAD.neXt profile to XML")?;
        Ok(format!("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n{}", serialized))
    }
}
