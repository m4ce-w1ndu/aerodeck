use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::io::{self, Cursor, Read, Write};

// Exact ClientDataArea IDs verified from SPAD-bridge-module.wasm:
// 1396987905 = 0x53445801 ('SDX\x01')
pub const SPAD_SC_DYN_VARDATA_ID: u32 = 0x53445801;
pub const SPAD_SC_DYN_VARDATA_DATA: u32 = 0x53445802;
pub const SPAD_SC_DYN_VARDUMP_ID: u32 = 0x53445803;
pub const SPAD_SC_DYN_VARDUMP_DATA: u32 = 0x53445804;
pub const SPAD_SC_DYN_HOSTREQUEST_ID: u32 = 0x53445805;
pub const SPAD_SC_DYN_HOSTREQUEST_DATA: u32 = 0x53445806;
pub const SPAD_SC_DYN_HOSTEVENT_ID: u32 = 0x53445807;
pub const SPAD_SC_DYN_HOSTEVENT_DATA: u32 = 0x53445808;
pub const SPAD_SC_DYN_HOSTREQUEST2_ID: u32 = 0x53445809;
pub const SPAD_SC_DYN_HOSTREQUEST2_DATA: u32 = 0x5344580A;

pub const HOST_SECRET: i32 = 0x01020304; // 16909060

// Operation codes verified in WASM switch table:
pub const OP_START_LVAR_SEARCH: i32 = 0;
pub const OP_SEND_LVARS: i32 = 1;
pub const OP_CONNECT: i32 = 7;
pub const OP_AIRCRAFT_NAME: i32 = 8;
pub const OP_REQ_AIRCRAFT_NAME: i32 = 9;
pub const OP_HEARTBEAT: i32 = 14;
pub const OP_EXECUTE_CALCULATOR_CODE: i32 = 16;
pub const OP_SETTING_UPDATE: i32 = 18;
pub const OP_RESET_LVARS: i32 = 10009;

// Calculator return types:
pub const CALC_RET_VOID: i32 = 0;
pub const CALC_RET_DOUBLE: i32 = 1;
pub const CALC_RET_INT: i32 = 2;
pub const CALC_RET_STRING: i32 = 3;

// HostEvent return status:
pub const EVENT_STATUS_SUCCESS: i32 = 17;
pub const EVENT_STATUS_ERROR: i32 = 13;

#[derive(Debug, Clone)]
pub struct HostRequest {
    pub secret: i32,
    pub operation: i32,
    pub dw_q400_index: i32,
    pub variable_name: String, // 256 bytes buffer
    pub variable_value: f64,
}

impl HostRequest {
    pub fn new(operation: i32, variable_name: &str, value: f64) -> Self {
        Self {
            secret: HOST_SECRET,
            operation,
            dw_q400_index: 0,
            variable_name: variable_name.to_string(),
            variable_value: value,
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(276);
        buf.write_i32::<LittleEndian>(self.secret).unwrap();
        buf.write_i32::<LittleEndian>(self.operation).unwrap();
        buf.write_i32::<LittleEndian>(self.dw_q400_index).unwrap();

        let str_bytes = self.variable_name.as_bytes();
        let copy_len = str_bytes.len().min(255);
        buf.extend_from_slice(&str_bytes[..copy_len]);
        buf.resize(12 + 256, 0); // pad variable_name to 256 bytes

        buf.write_f64::<LittleEndian>(self.variable_value).unwrap();
        buf
    }
}

#[derive(Debug, Clone)]
pub struct HostRequest2 {
    pub secret: i32,
    pub operation: i32,
    pub dw_q400_index: i32,
    pub variable_name: String, // 1024 bytes buffer for long scripts/calc code
    pub variable_value: f64,
}

impl HostRequest2 {
    pub fn new(operation: i32, calc_code: &str, ret_type: i32) -> Self {
        Self {
            secret: HOST_SECRET,
            operation,
            dw_q400_index: ret_type,
            variable_name: calc_code.to_string(),
            variable_value: 0.0,
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(1044);
        buf.write_i32::<LittleEndian>(self.secret).unwrap();
        buf.write_i32::<LittleEndian>(self.operation).unwrap();
        buf.write_i32::<LittleEndian>(self.dw_q400_index).unwrap();

        let str_bytes = self.variable_name.as_bytes();
        let copy_len = str_bytes.len().min(1023);
        buf.extend_from_slice(&str_bytes[..copy_len]);
        buf.resize(12 + 1024, 0); // pad variable_name to 1024 bytes

        buf.write_f64::<LittleEndian>(self.variable_value).unwrap();
        buf
    }
}

#[derive(Debug, Clone)]
pub struct HostEvent {
    pub secret: i32,
    pub operation: i32,
    pub has_message: i32,
    pub value: f64,
    pub message: String,
}

impl HostEvent {
    pub fn from_bytes(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() < 24 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "HostEvent packet too short"));
        }
        let mut cursor = Cursor::new(bytes);
        let secret = cursor.read_i32::<LittleEndian>()?;
        let operation = cursor.read_i32::<LittleEndian>()?;
        let has_message = cursor.read_i32::<LittleEndian>()?;
        let value = cursor.read_f64::<LittleEndian>()?;

        let mut msg_bytes = vec![0u8; bytes.len() - 20];
        cursor.read_exact(&mut msg_bytes)?;
        let end = msg_bytes.iter().position(|&b| b == 0).unwrap_or(msg_bytes.len());
        let message = String::from_utf8_lossy(&msg_bytes[..end]).to_string();

        Ok(Self {
            secret,
            operation,
            has_message,
            value,
            message,
        })
    }
}

#[derive(Debug, Clone)]
pub struct VarDump {
    pub secret: i32,
    pub start_index: i32,
    pub variable_count: i32,
    pub variable_names: Vec<String>,
}

impl VarDump {
    pub fn from_bytes(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() < 12 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "VarDump packet too short"));
        }
        let mut cursor = Cursor::new(bytes);
        let secret = cursor.read_i32::<LittleEndian>()?;
        let start_index = cursor.read_i32::<LittleEndian>()?;
        let variable_count = cursor.read_i32::<LittleEndian>()?;

        let mut names_raw = vec![0u8; bytes.len() - 12];
        cursor.read_exact(&mut names_raw)?;
        let end = names_raw.iter().position(|&b| b == 0).unwrap_or(names_raw.len());
        let raw_str = String::from_utf8_lossy(&names_raw[..end]);

        // Separated by semicolon ';' as verified in WASM code
        let variable_names: Vec<String> = raw_str
            .split(';')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();

        Ok(Self {
            secret,
            start_index,
            variable_count,
            variable_names,
        })
    }
}

#[derive(Debug, Clone)]
pub struct DynamicData {
    pub value: f64,
    pub tag: u32,
    pub name: String,
}

impl DynamicData {
    pub fn from_bytes(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() < 12 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "DynamicData packet too short"));
        }
        let mut cursor = Cursor::new(bytes);
        let value = cursor.read_f64::<LittleEndian>()?;
        let tag = cursor.read_u32::<LittleEndian>()?;

        let mut name_bytes = vec![0u8; bytes.len() - 12];
        cursor.read_exact(&mut name_bytes)?;
        let end = name_bytes.iter().position(|&b| b == 0).unwrap_or(name_bytes.len());
        let name = String::from_utf8_lossy(&name_bytes[..end]).to_string();

        Ok(Self { value, tag, name })
    }
}
