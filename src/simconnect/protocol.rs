use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::io::{self, Cursor, Read};

pub const SIMCONNECT_PROTOCOL_VERSION: u32 = 0x4;
pub const SIMCONNECT_OBJECT_ID_USER: u32 = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum SimConnectSendId {
    Open = 1,
    MapClientEventToSimEvent = 4,
    TransmitClientEvent = 5,
    SetDataOnSimObject = 16,
    RequestDataOnSimObject = 14,
    SubscribeToSystemEvent = 23,
    UnsubscribeFromSystemEvent = 24,
    RequestSystemState = 53,
    SetSystemState = 54,
    MapClientDataNameToId = 55,
    CreateClientData = 56,
    AddToClientDataDefinition = 57,
    ClearClientDataDefinition = 58,
    RequestClientData = 59,
    SetClientData = 60,
    Unknown = 9999,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum SimConnectRecvId {
    Null = 0,
    Exception = 1,
    Open = 2,
    Quit = 3,
    Event = 4,
    SimObjectData = 8,
    SimObjectDataByType = 9,
    ClientData = 18,
    AssignedObjectId = 19,
    SystemState = 25,
    EventFilename = 26,
    EventFrame = 27,
    Unknown = 9999,
}

impl From<u32> for SimConnectRecvId {
    fn from(id: u32) -> Self {
        match id {
            0 => SimConnectRecvId::Null,
            1 => SimConnectRecvId::Exception,
            2 => SimConnectRecvId::Open,
            3 => SimConnectRecvId::Quit,
            4 => SimConnectRecvId::Event,
            8 => SimConnectRecvId::SimObjectData,
            9 => SimConnectRecvId::SimObjectDataByType,
            18 => SimConnectRecvId::ClientData,
            19 => SimConnectRecvId::AssignedObjectId,
            25 => SimConnectRecvId::SystemState,
            26 => SimConnectRecvId::EventFilename,
            27 => SimConnectRecvId::EventFrame,
            _ => SimConnectRecvId::Unknown,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RecvHeader {
    pub size: u32,
    pub version: u32,
    pub id: SimConnectRecvId,
    pub raw_id: u32,
}

impl RecvHeader {
    pub fn read_from(cursor: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let size = cursor.read_u32::<LittleEndian>()?;
        let version = cursor.read_u32::<LittleEndian>()?;
        let raw_id = cursor.read_u32::<LittleEndian>()?;
        Ok(Self {
            size,
            version,
            id: SimConnectRecvId::from(raw_id),
            raw_id,
        })
    }
}

#[derive(Debug, Clone)]
pub struct RecvOpen {
    pub header: RecvHeader,
    pub app_name: String,
    pub app_version_major: u32,
    pub app_version_minor: u32,
    pub app_build_major: u32,
    pub app_build_minor: u32,
    pub simconnect_version_major: u32,
    pub simconnect_version_minor: u32,
    pub simconnect_build_major: u32,
    pub simconnect_build_minor: u32,
}

impl RecvOpen {
    pub fn read_from(header: RecvHeader, cursor: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let mut app_name_bytes = [0u8; 256];
        cursor.read_exact(&mut app_name_bytes)?;
        let end = app_name_bytes.iter().position(|&b| b == 0).unwrap_or(256);
        let app_name = String::from_utf8_lossy(&app_name_bytes[..end]).to_string();

        let app_version_major = cursor.read_u32::<LittleEndian>()?;
        let app_version_minor = cursor.read_u32::<LittleEndian>()?;
        let app_build_major = cursor.read_u32::<LittleEndian>()?;
        let app_build_minor = cursor.read_u32::<LittleEndian>()?;

        let simconnect_version_major = cursor.read_u32::<LittleEndian>()?;
        let simconnect_version_minor = cursor.read_u32::<LittleEndian>()?;
        let simconnect_build_major = cursor.read_u32::<LittleEndian>()?;
        let simconnect_build_minor = cursor.read_u32::<LittleEndian>()?;

        Ok(Self {
            header,
            app_name,
            app_version_major,
            app_version_minor,
            app_build_major,
            app_build_minor,
            simconnect_version_major,
            simconnect_version_minor,
            simconnect_build_major,
            simconnect_build_minor,
        })
    }
}

#[derive(Debug, Clone)]
pub struct RecvClientData {
    pub header: RecvHeader,
    pub request_id: u32,
    pub define_id: u32,
    pub flags: u32,
    pub object_id: u32,
    pub define_count: u32,
    pub data: Vec<u8>,
}

impl RecvClientData {
    pub fn read_from(header: RecvHeader, cursor: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let request_id = cursor.read_u32::<LittleEndian>()?;
        let define_id = cursor.read_u32::<LittleEndian>()?;
        let flags = cursor.read_u32::<LittleEndian>()?;
        let object_id = cursor.read_u32::<LittleEndian>()?;
        let define_count = cursor.read_u32::<LittleEndian>()?;

        let mut data = Vec::new();
        cursor.read_to_end(&mut data)?;

        Ok(Self {
            header,
            request_id,
            define_id,
            flags,
            object_id,
            define_count,
            data,
        })
    }
}

#[derive(Debug, Clone)]
pub struct RecvEvent {
    pub header: RecvHeader,
    pub group_id: u32,
    pub event_id: u32,
    pub data: u32,
}

impl RecvEvent {
    pub fn read_from(header: RecvHeader, cursor: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let group_id = cursor.read_u32::<LittleEndian>()?;
        let event_id = cursor.read_u32::<LittleEndian>()?;
        let data = cursor.read_u32::<LittleEndian>()?;
        Ok(Self {
            header,
            group_id,
            event_id,
            data,
        })
    }
}

#[derive(Debug, Clone)]
pub struct RecvException {
    pub header: RecvHeader,
    pub exception: u32,
    pub send_id: u32,
    pub index: u32,
}

impl RecvException {
    pub fn read_from(header: RecvHeader, cursor: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let exception = cursor.read_u32::<LittleEndian>()?;
        let send_id = cursor.read_u32::<LittleEndian>()?;
        let index = cursor.read_u32::<LittleEndian>()?;
        Ok(Self {
            header,
            exception,
            send_id,
            index,
        })
    }
}

#[derive(Debug, Clone)]
pub struct RecvSystemState {
    pub header: RecvHeader,
    pub request_id: u32,
    pub integer: u32,
    pub float: f32,
    pub string: String,
}

impl RecvSystemState {
    pub fn read_from(header: RecvHeader, cursor: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let request_id = cursor.read_u32::<LittleEndian>()?;
        let integer = cursor.read_u32::<LittleEndian>()?;
        let float = cursor.read_f32::<LittleEndian>()?;
        let mut str_bytes = Vec::new();
        cursor.read_to_end(&mut str_bytes)?;
        let end = str_bytes.iter().position(|&b| b == 0).unwrap_or(str_bytes.len());
        let string = String::from_utf8_lossy(&str_bytes[..end]).trim().to_string();
        Ok(Self {
            header,
            request_id,
            integer,
            float,
            string,
        })
    }
}

#[derive(Debug, Clone)]
pub struct RecvEventFilename {
    pub header: RecvHeader,
    pub group_id: u32,
    pub event_id: u32,
    pub data: u32,
    pub filename: String,
}

impl RecvEventFilename {
    pub fn read_from(header: RecvHeader, cursor: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let group_id = cursor.read_u32::<LittleEndian>()?;
        let event_id = cursor.read_u32::<LittleEndian>()?;
        let data = cursor.read_u32::<LittleEndian>()?;
        let mut str_bytes = Vec::new();
        cursor.read_to_end(&mut str_bytes)?;
        let max_len = str_bytes.len().min(260);
        let end = str_bytes[..max_len].iter().position(|&b| b == 0).unwrap_or(max_len);
        let filename = String::from_utf8_lossy(&str_bytes[..end]).trim().to_string();
        Ok(Self {
            header,
            group_id,
            event_id,
            data,
            filename,
        })
    }
}

#[derive(Debug, Clone)]
pub enum SimConnectMessage {
    Open(RecvOpen),
    Quit,
    ClientData(RecvClientData),
    Event(RecvEvent),
    SystemState(RecvSystemState),
    EventFilename(RecvEventFilename),
    Exception(RecvException),
    Raw(RecvHeader, Vec<u8>),
}

/// Helper to serialize outgoing packets
pub struct SendPacketBuilder {
    send_id_type: SimConnectSendId,
    send_seq: u32,
    body: Vec<u8>,
}

impl SendPacketBuilder {
    pub fn new(send_id_type: SimConnectSendId, send_seq: u32) -> Self {
        Self {
            send_id_type,
            send_seq,
            body: Vec::new(),
        }
    }

    pub fn write_u32(&mut self, val: u32) {
        self.body.write_u32::<LittleEndian>(val).unwrap();
    }

    pub fn write_f64(&mut self, val: f64) {
        self.body.write_f64::<LittleEndian>(val).unwrap();
    }

    pub fn write_fixed_string(&mut self, s: &str, len: usize) {
        let bytes = s.as_bytes();
        let copy_len = bytes.len().min(len.saturating_sub(1));
        self.body.extend_from_slice(&bytes[..copy_len]);
        self.body.resize(self.body.len() + (len - copy_len), 0);
    }

    pub fn write_bytes(&mut self, b: &[u8]) {
        self.body.extend_from_slice(b);
    }

    pub fn build(self) -> Vec<u8> {
        let total_size = (16 + self.body.len()) as u32;
        let mut out = Vec::with_capacity(total_size as usize);
        out.write_u32::<LittleEndian>(total_size).unwrap();
        out.write_u32::<LittleEndian>(SIMCONNECT_PROTOCOL_VERSION).unwrap();
        out.write_u32::<LittleEndian>(self.send_id_type as u32).unwrap();
        out.write_u32::<LittleEndian>(self.send_seq).unwrap();
        out.extend_from_slice(&self.body);
        out
    }
}
