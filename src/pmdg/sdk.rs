//! Parser for the PMDG SDK C headers shipped with the aircraft
//! (`<package>/Documentation/SDK/PMDG_*_SDK.h`).
//!
//! The headers are read from the user's installation at runtime rather than bundled, so
//! event IDs and the data layout always match the installed aircraft version.

use anyhow::{anyhow, Context, Result};
use std::collections::HashMap;
use std::path::Path;

/// Which PMDG product family a header describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PmdgFamily {
    /// 737 for MSFS (`PMDG_NG3_SDK.h`)
    Ng3,
    /// 777 for MSFS (`PMDG_777X_SDK.h`)
    B777x,
}

impl PmdgFamily {
    pub const ALL: [PmdgFamily; 2] = [PmdgFamily::Ng3, PmdgFamily::B777x];

    pub fn header_file(self) -> &'static str {
        match self {
            PmdgFamily::Ng3 => "PMDG_NG3_SDK.h",
            PmdgFamily::B777x => "PMDG_777X_SDK.h",
        }
    }

    /// Macro prefix inside the header (`PMDG_NG3_DATA_NAME`, ...).
    fn macro_prefix(self) -> &'static str {
        match self {
            PmdgFamily::Ng3 => "PMDG_NG3",
            PmdgFamily::B777x => "PMDG_777X",
        }
    }

    /// Data provider prefixes SPAD.neXt profiles use for this family.
    pub fn profile_prefixes(self) -> &'static [&'static str] {
        match self {
            PmdgFamily::Ng3 => &["PMDGNG3", "PMDG737", "PMDGNGX"],
            PmdgFamily::B777x => &["PMDG777", "PMDG777X"],
        }
    }

    pub fn from_profile_prefix(prefix: &str) -> Option<Self> {
        let p = prefix.to_ascii_uppercase();
        Self::ALL.into_iter().find(|f| f.profile_prefixes().contains(&p.as_str()))
    }

    /// Options file inside `<Packages>/<package>/work/` holding the `[SDK]` switches.
    pub fn options_file(self) -> &'static str {
        match self {
            PmdgFamily::Ng3 => "737NG3_Options.ini",
            PmdgFamily::B777x => "777_Options.ini",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            PmdgFamily::Ng3 => "PMDG 737",
            PmdgFamily::B777x => "PMDG 777",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldType {
    Bool,
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    F32,
}

impl FieldType {
    fn parse(tokens: &[&str]) -> Option<Self> {
        let unsigned = tokens.first() == Some(&"unsigned");
        let base = if unsigned { tokens.get(1)? } else { tokens.first()? };
        Some(match (*base, unsigned) {
            ("bool", _) => FieldType::Bool,
            ("char", false) => FieldType::I8,
            ("char", true) => FieldType::U8,
            ("short", false) => FieldType::I16,
            ("short", true) => FieldType::U16,
            ("int" | "long", false) => FieldType::I32,
            ("int" | "long", true) => FieldType::U32,
            ("float", _) => FieldType::F32,
            _ => return None,
        })
    }

    /// Size and alignment (MSVC, default packing).
    pub fn size(self) -> usize {
        match self {
            FieldType::Bool | FieldType::I8 | FieldType::U8 => 1,
            FieldType::I16 | FieldType::U16 => 2,
            FieldType::I32 | FieldType::U32 | FieldType::F32 => 4,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Field {
    pub name: String,
    pub ty: FieldType,
    pub offset: usize,
    /// Element count (1 for scalars).
    pub count: usize,
    pub is_array: bool,
}

#[derive(Debug, Clone)]
pub enum FieldValue {
    Number(f64),
    Text(String),
}

impl FieldValue {
    pub fn as_string(&self) -> String {
        match self {
            FieldValue::Number(n) => {
                if n.fract() == 0.0 { format!("{}", *n as i64) } else { n.to_string() }
            }
            FieldValue::Text(s) => s.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PmdgSdk {
    pub family: PmdgFamily,
    pub data_name: String,
    pub data_id: u32,
    pub data_definition: u32,
    pub fields: Vec<Field>,
    /// `sizeof` the data struct.
    pub data_size: usize,
    /// Upper-case event name (`EVT_MCP_HEADING_SELECTOR`) -> event ID.
    pub events: HashMap<String, u32>,
}

impl PmdgSdk {
    pub fn load(family: PmdgFamily, path: &Path) -> Result<Self> {
        let text = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        Self::parse(family, &String::from_utf8_lossy(&text))
    }

    pub fn parse(family: PmdgFamily, source: &str) -> Result<Self> {
        let source = strip_comments(source);
        let defines = resolve_defines(&source);
        let m = family.macro_prefix();
        let get = |suffix: &str| {
            defines
                .numbers
                .get(&format!("{}_{}", m, suffix))
                .copied()
                .ok_or_else(|| anyhow!("{}_{} not found in header", m, suffix))
        };
        let data_name = defines
            .strings
            .get(&format!("{}_DATA_NAME", m))
            .cloned()
            .ok_or_else(|| anyhow!("{}_DATA_NAME not found in header", m))?;
        let data_id = get("DATA_ID")? as u32;
        let data_definition = get("DATA_DEFINITION")? as u32;

        let (fields, data_size) = parse_struct(&source, &format!("{}_Data", m), &defines.numbers)?;

        let events = defines
            .numbers
            .iter()
            .filter(|(k, _)| k.starts_with("EVT_"))
            .map(|(k, v)| (k.to_ascii_uppercase(), *v as u32))
            .collect();

        Ok(Self { family, data_name, data_id, data_definition, fields, data_size, events })
    }

    /// Looks up an event by SPAD.neXt name; spaces and underscores are interchangeable
    /// (`EVT MCP HEADING SELECTOR` == `EVT_MCP_HEADING_SELECTOR`), and `EVT_` is optional.
    pub fn event_id(&self, name: &str) -> Option<u32> {
        let norm = name.trim().replace([' ', '-'], "_").to_ascii_uppercase();
        self.events
            .get(&norm)
            .or_else(|| self.events.get(&format!("EVT_{}", norm)))
            .copied()
    }

    /// Resolves `MCP_Course_0` (array element) or `MCP_Heading` (scalar), case-insensitive.
    pub fn find_field(&self, name: &str) -> Option<(&Field, usize)> {
        let name = name.trim();
        if let Some(f) = self.fields.iter().find(|f| f.name.eq_ignore_ascii_case(name)) {
            return Some((f, 0));
        }
        let (base, idx) = name.rsplit_once('_')?;
        let idx: usize = idx.parse().ok()?;
        let f = self.fields.iter().find(|f| f.is_array && f.name.eq_ignore_ascii_case(base))?;
        (idx < f.count).then_some((f, idx))
    }

    /// Decodes a field from a received data block.
    pub fn read_field(&self, data: &[u8], field: &Field, index: usize) -> Option<FieldValue> {
        // `char[]` arrays are zero-terminated strings.
        if field.is_array && field.ty == FieldType::I8 {
            let bytes = data.get(field.offset..field.offset + field.count)?;
            let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
            return Some(FieldValue::Text(String::from_utf8_lossy(&bytes[..end]).trim().to_string()));
        }
        let at = field.offset + index * field.ty.size();
        let b = data.get(at..at + field.ty.size())?;
        let v = match field.ty {
            FieldType::Bool => (b[0] != 0) as u8 as f64,
            FieldType::I8 => b[0] as i8 as f64,
            FieldType::U8 => b[0] as f64,
            FieldType::I16 => i16::from_le_bytes([b[0], b[1]]) as f64,
            FieldType::U16 => u16::from_le_bytes([b[0], b[1]]) as f64,
            FieldType::I32 => i32::from_le_bytes(b.try_into().ok()?) as f64,
            FieldType::U32 => u32::from_le_bytes(b.try_into().ok()?) as f64,
            FieldType::F32 => f32::from_le_bytes(b.try_into().ok()?) as f64,
        };
        Some(FieldValue::Number(v))
    }

    /// Every field value, keyed the way SPAD.neXt names them (`MCP_COURSE_0`), upper-case.
    pub fn decode_all(&self, data: &[u8]) -> Vec<(String, FieldValue)> {
        let mut out = Vec::with_capacity(self.fields.len() * 2);
        for f in &self.fields {
            if f.name.eq_ignore_ascii_case("reserved") {
                continue;
            }
            if !f.is_array || f.ty == FieldType::I8 {
                if let Some(v) = self.read_field(data, f, 0) {
                    out.push((f.name.to_ascii_uppercase(), v));
                }
            } else {
                for i in 0..f.count {
                    if let Some(v) = self.read_field(data, f, i) {
                        out.push((format!("{}_{}", f.name.to_ascii_uppercase(), i), v));
                    }
                }
            }
        }
        out
    }
}

fn strip_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut chars = src.chars().peekable();
    let mut in_str = false;
    while let Some(c) = chars.next() {
        if in_str {
            out.push(c);
            if c == '"' {
                in_str = false;
            }
            continue;
        }
        match (c, chars.peek()) {
            ('"', _) => {
                in_str = true;
                out.push(c);
            }
            ('/', Some('/')) => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            ('/', Some('*')) => {
                chars.next();
                let mut prev = '\0';
                for c in chars.by_ref() {
                    if prev == '*' && c == '/' {
                        break;
                    }
                    if c == '\n' {
                        out.push('\n');
                    }
                    prev = c;
                }
            }
            _ => out.push(c),
        }
    }
    out
}

struct Defines {
    numbers: HashMap<String, i64>,
    strings: HashMap<String, String>,
}

/// Collects `#define NAME <expr>` and evaluates expressions that refer to other defines
/// (`(THIRD_PARTY_EVENT_ID_MIN + 1)`, `EVT_FOO`, `(CDU_EVT_OFFSET_R + EVT_CDU_L_L2)`).
fn resolve_defines(src: &str) -> Defines {
    let mut raw: Vec<(String, String)> = Vec::new();
    let mut strings = HashMap::new();
    for line in src.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("#define") else { continue };
        let rest = rest.trim();
        let mut parts = rest.splitn(2, char::is_whitespace);
        let Some(name) = parts.next() else { continue };
        if name.contains('(') {
            continue; // function-like macro
        }
        let value = parts.next().unwrap_or("").trim().to_string();
        if let Some(s) = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')) {
            strings.insert(name.to_string(), s.to_string());
        } else if !value.is_empty() {
            raw.push((name.to_string(), value));
        }
    }

    let mut numbers: HashMap<String, i64> = HashMap::new();
    loop {
        let mut progressed = false;
        for (name, expr) in &raw {
            if numbers.contains_key(name) {
                continue;
            }
            if let Some(v) = eval_expr(expr, &numbers) {
                numbers.insert(name.clone(), v);
                progressed = true;
            }
        }
        if !progressed {
            break;
        }
    }
    Defines { numbers, strings }
}

/// Evaluates `+ - * ( )` over integer literals (decimal / hex) and known defines.
fn eval_expr(expr: &str, known: &HashMap<String, i64>) -> Option<i64> {
    let tokens = tokenize(expr)?;
    let mut pos = 0;
    let v = parse_sum(&tokens, &mut pos, known)?;
    (pos == tokens.len()).then_some(v)
}

#[derive(Debug, Clone)]
enum Tok {
    Num(i64),
    Ident(String),
    Op(char),
}

fn tokenize(s: &str) -> Option<Vec<Tok>> {
    let mut out = Vec::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = b[i] as char;
        if c.is_whitespace() {
            i += 1;
        } else if "+-*()".contains(c) {
            out.push(Tok::Op(c));
            i += 1;
        } else if c.is_ascii_digit() {
            let start = i;
            while i < b.len() && (b[i] as char).is_ascii_alphanumeric() {
                i += 1;
            }
            let lit = s[start..i].trim_end_matches(['u', 'U', 'l', 'L']);
            let v = if let Some(h) = lit.strip_prefix("0x").or_else(|| lit.strip_prefix("0X")) {
                i64::from_str_radix(h, 16).ok()?
            } else {
                lit.parse().ok()?
            };
            out.push(Tok::Num(v));
        } else if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            while i < b.len() && ((b[i] as char).is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            out.push(Tok::Ident(s[start..i].to_string()));
        } else {
            return None;
        }
    }
    Some(out)
}

fn parse_sum(t: &[Tok], pos: &mut usize, k: &HashMap<String, i64>) -> Option<i64> {
    let mut v = parse_product(t, pos, k)?;
    while let Some(Tok::Op(op @ ('+' | '-'))) = t.get(*pos) {
        *pos += 1;
        let r = parse_product(t, pos, k)?;
        v = if *op == '+' { v + r } else { v - r };
    }
    Some(v)
}

fn parse_product(t: &[Tok], pos: &mut usize, k: &HashMap<String, i64>) -> Option<i64> {
    let mut v = parse_atom(t, pos, k)?;
    while let Some(Tok::Op('*')) = t.get(*pos) {
        *pos += 1;
        v *= parse_atom(t, pos, k)?;
    }
    Some(v)
}

fn parse_atom(t: &[Tok], pos: &mut usize, k: &HashMap<String, i64>) -> Option<i64> {
    let tok = t.get(*pos)?.clone();
    *pos += 1;
    match tok {
        Tok::Num(n) => Some(n),
        Tok::Ident(id) => k.get(&id).copied(),
        Tok::Op('-') => parse_atom(t, pos, k).map(|v| -v),
        Tok::Op('(') => {
            let v = parse_sum(t, pos, k)?;
            matches!(t.get(*pos), Some(Tok::Op(')'))).then(|| *pos += 1)?;
            Some(v)
        }
        _ => None,
    }
}

/// Lays out `struct <name> { ... };` with MSVC natural alignment.
fn parse_struct(src: &str, name: &str, defines: &HashMap<String, i64>) -> Result<(Vec<Field>, usize)> {
    let start = src
        .find(&format!("struct {}", name))
        .ok_or_else(|| anyhow!("struct {} not found in header", name))?;
    let body_start = start + src[start..].find('{').ok_or_else(|| anyhow!("struct {} has no body", name))? + 1;
    let body_end = body_start + src[body_start..].find("};").ok_or_else(|| anyhow!("struct {} not terminated", name))?;

    let mut fields = Vec::new();
    let mut offset = 0usize;
    let mut max_align = 1usize;
    for decl in src[body_start..body_end].split(';') {
        let decl = decl.trim();
        if decl.is_empty() || decl.starts_with('#') {
            continue;
        }
        // "unsigned short  MCP_Course[2]" -> type tokens + declarator
        let (ty_part, declarator) = match decl.find('[') {
            Some(b) => {
                let head = &decl[..b];
                let split = head.trim_end().rfind(char::is_whitespace).ok_or_else(|| anyhow!("bad declaration '{}'", decl))?;
                (&decl[..split], &decl[split..])
            }
            None => {
                let split = decl.rfind(char::is_whitespace).ok_or_else(|| anyhow!("bad declaration '{}'", decl))?;
                (&decl[..split], &decl[split..])
            }
        };
        let ty_tokens: Vec<&str> = ty_part.split_whitespace().collect();
        let ty = FieldType::parse(&ty_tokens).ok_or_else(|| anyhow!("unsupported field type in '{}'", decl))?;

        let declarator = declarator.trim();
        let field_name = declarator.split('[').next().unwrap_or("").trim().to_string();
        let mut count = 1usize;
        let mut is_array = false;
        for dim in declarator.split('[').skip(1) {
            let expr = dim.split(']').next().unwrap_or("");
            let n = eval_expr(expr, defines).ok_or_else(|| anyhow!("cannot evaluate array size '{}' in '{}'", expr, decl))?;
            count *= n as usize;
            is_array = true;
        }

        let align = ty.size();
        max_align = max_align.max(align);
        offset = offset.div_ceil(align) * align;
        fields.push(Field { name: field_name, ty, offset, count, is_array });
        offset += ty.size() * count;
    }
    let size = offset.div_ceil(max_align) * max_align;
    Ok((fields, size))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
#define PMDG_NG3_DATA_NAME			"PMDG_NG3_Data"
#define PMDG_NG3_DATA_ID			0x4E473331
#define PMDG_NG3_DATA_DEFINITION	0x4E473332
#define SIZE 3
struct PMDG_NG3_Data
{
	unsigned char	IRS_DisplaySelector;	// Positions 0..4
	bool			IRS_annunALIGN[2];
	char			IRS_DisplayLeft[7];		// string
	unsigned short	MCP_Course[2];
	float			MCP_IASMach;			/* Mach if < 10.0 */
	bool			Flags[SIZE];
	unsigned char   reserved[255];
};
#define THIRD_PARTY_EVENT_ID_MIN        		0x00011000		// equals to 69632
#define	EVT_OH_ELEC_BATTERY_SWITCH	(THIRD_PARTY_EVENT_ID_MIN + 1)
#define EVT_FIRST					EVT_OH_ELEC_BATTERY_SWITCH
#define CDU_EVT_OFFSET_R			(EVT_FIRST - THIRD_PARTY_EVENT_ID_MIN + 100)
#define EVT_CDU_R_L2				(CDU_EVT_OFFSET_R + EVT_OH_ELEC_BATTERY_SWITCH)
"#;

    #[test]
    fn parses_layout_and_events() {
        let sdk = PmdgSdk::parse(PmdgFamily::Ng3, SAMPLE).unwrap();
        assert_eq!(sdk.data_name, "PMDG_NG3_Data");
        assert_eq!(sdk.data_id, 0x4E473331);

        let off = |n: &str| sdk.fields.iter().find(|f| f.name == n).unwrap().offset;
        assert_eq!(off("IRS_DisplaySelector"), 0);
        assert_eq!(off("IRS_annunALIGN"), 1);
        assert_eq!(off("IRS_DisplayLeft"), 3);
        assert_eq!(off("MCP_Course"), 10); // 3 + 7 = 10, already 2-aligned
        assert_eq!(off("MCP_IASMach"), 16); // 14 -> aligned to 4
        assert_eq!(off("Flags"), 20);
        assert_eq!(sdk.data_size, (20 + 3 + 255usize).div_ceil(4) * 4);

        assert_eq!(sdk.event_id("EVT OH ELEC BATTERY SWITCH"), Some(69633));
        assert_eq!(sdk.event_id("oh_elec_battery_switch"), Some(69633));
        assert_eq!(sdk.event_id("EVT_CDU_R_L2"), Some(101 + 69633));
    }

    #[test]
    fn decodes_values() {
        let sdk = PmdgSdk::parse(PmdgFamily::Ng3, SAMPLE).unwrap();
        let mut data = vec![0u8; sdk.data_size];
        data[2] = 1; // IRS_annunALIGN[1]
        data[3..7].copy_from_slice(b"N47\0");
        data[12..14].copy_from_slice(&270u16.to_le_bytes()); // MCP_Course[1]
        data[16..20].copy_from_slice(&0.78f32.to_le_bytes());

        let (f, i) = sdk.find_field("MCP_Course_1").unwrap();
        assert_eq!(sdk.read_field(&data, f, i).unwrap().as_string(), "270");
        let (f, i) = sdk.find_field("irs_annunalign_1").unwrap();
        assert_eq!(sdk.read_field(&data, f, i).unwrap().as_string(), "1");
        let (f, i) = sdk.find_field("IRS_DisplayLeft").unwrap();
        assert_eq!(sdk.read_field(&data, f, i).unwrap().as_string(), "N47");
        assert!(sdk.find_field("MCP_Course_2").is_none());

        let all: HashMap<_, _> = sdk.decode_all(&data).into_iter().collect();
        assert_eq!(all["MCP_COURSE_1"].as_string(), "270");
        assert!(!all.contains_key("RESERVED_0"));
    }
}
