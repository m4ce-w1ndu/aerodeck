use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

use super::schema::SpadProfile;

pub struct ProfileParser;

impl ProfileParser {
    pub fn parse_file<P: AsRef<Path>>(path: P) -> Result<SpadProfile> {
        let content = fs::read_to_string(path.as_ref())
            .with_context(|| format!("Failed to read profile file at {:?}", path.as_ref()))?;
        Self::parse_str(&content)
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
