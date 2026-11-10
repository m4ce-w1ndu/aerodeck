use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::io::Cursor;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{broadcast, Mutex};
use tracing::{error, info, trace, warn};

use super::protocol::*;

/// `SIMCONNECT_GROUP_PRIORITY_HIGHEST`
pub const SIMCONNECT_GROUP_PRIORITY_HIGHEST: u32 = 1;
/// `SIMCONNECT_EVENT_FLAG_GROUPID_IS_PRIORITY`
pub const SIMCONNECT_EVENT_FLAG_GROUPID_IS_PRIORITY: u32 = 0x0000_0010;

/// First client event ID handed out by the dynamic event registry.
/// IDs below this are reserved for fixed system-event subscriptions (e.g. 900 = AircraftLoaded).
const FIRST_DYNAMIC_EVENT_ID: u32 = 0x0001_0000;

/// Bounded record of what each outgoing packet was, so exceptions (which only carry
/// the packet sequence number) can be reported with a meaningful description.
const SENT_LOG_CAPACITY: usize = 256;

pub struct SimConnectClient {
    write_stream: Arc<Mutex<tokio::net::tcp::OwnedWriteHalf>>,
    send_seq: AtomicU32,
    msg_tx: broadcast::Sender<SimConnectMessage>,
    /// Sim event name (upper-case) -> client event ID mapped on this connection.
    event_ids: Mutex<HashMap<String, u32>>,
    next_event_id: AtomicU32,
    sent_log: Arc<std::sync::Mutex<HashMap<u32, String>>>,
    /// Live SimVar values, updated by the reader task (see `simvars.rs`).
    pub(super) simvar_store: Arc<super::simvars::SimVarStore>,
    /// Serializes SimVar subscriptions so each name gets exactly one definition.
    pub(super) simvar_subscribe_lock: Mutex<()>,
    /// PMDG SDK data areas registered on this connection (see `crate::pmdg`).
    pub(crate) pmdg_store: Arc<crate::pmdg::PmdgStore>,
}

impl SimConnectClient {
    pub async fn connect(addr: &str) -> Result<Self> {
        info!("Connecting to SimConnect server at {}", addr);
        let stream = TcpStream::connect(addr).await
            .map_err(|e| anyhow!("Failed to connect to SimConnect at {}: {}. Ensure MSFS is running with network SimConnect enabled.", addr, e))?;

        let (mut read_half, write_half) = stream.into_split();
        let (msg_tx, _) = broadcast::channel(512);
        let tx_clone = msg_tx.clone();
        let sent_log: Arc<std::sync::Mutex<HashMap<u32, String>>> = Arc::new(std::sync::Mutex::new(HashMap::new()));
        let sent_log_reader = sent_log.clone();
        let simvar_store = Arc::new(super::simvars::SimVarStore::default());
        let simvar_store_reader = simvar_store.clone();
        let pmdg_store = Arc::new(crate::pmdg::PmdgStore::default());
        let pmdg_store_reader = pmdg_store.clone();

        // Background reader loop
        tokio::spawn(async move {
            let mut len_buf = [0u8; 4];
            loop {
                // Read 4-byte dwSize
                if let Err(e) = read_half.read_exact(&mut len_buf).await {
                    error!("SimConnect read error or disconnect: {}", e);
                    let _ = tx_clone.send(SimConnectMessage::Quit);
                    break;
                }
                let packet_len = u32::from_le_bytes(len_buf) as usize;
                if packet_len < 12 {
                    error!("Invalid packet size from SimConnect: {}", packet_len);
                    break;
                }

                // Read remainder of packet
                let mut packet_buf = vec![0u8; packet_len];
                packet_buf[0..4].copy_from_slice(&len_buf);
                if let Err(e) = read_half.read_exact(&mut packet_buf[4..]).await {
                    error!("Failed to read packet body ({} bytes): {}", packet_len, e);
                    break;
                }

                let mut cursor = Cursor::new(&packet_buf[..]);
                match RecvHeader::read_from(&mut cursor) {
                    Ok(header) => {
                        trace!("SimConnect packet received: {:?} (size {})", header.id, header.size);
                        let msg = match header.id {
                            SimConnectRecvId::Open => {
                                match RecvOpen::read_from(header, &mut cursor) {
                                    Ok(open) => {
                                        info!("Connected to SimConnect: {} v{}.{} (build {}.{})",
                                            open.app_name, open.app_version_major, open.app_version_minor,
                                            open.app_build_major, open.app_build_minor);
                                        SimConnectMessage::Open(open)
                                    }
                                    Err(e) => {
                                        warn!("Failed to parse RecvOpen: {}", e);
                                        continue;
                                    }
                                }
                            }
                            SimConnectRecvId::Quit => {
                                info!("SimConnect sent Quit message");
                                SimConnectMessage::Quit
                            }
                            SimConnectRecvId::ClientData => {
                                match RecvClientData::read_from(header, &mut cursor) {
                                    Ok(cd) => {
                                        pmdg_store_reader.apply(&cd);
                                        SimConnectMessage::ClientData(cd)
                                    }
                                    Err(e) => {
                                        warn!("Failed to parse RecvClientData: {}", e);
                                        continue;
                                    }
                                }
                            }
                            SimConnectRecvId::SimObjectData | SimConnectRecvId::SimObjectDataByType => {
                                match RecvSimObjectData::read_from(header, &mut cursor) {
                                    Ok(d) => {
                                        simvar_store_reader.apply(&d);
                                        SimConnectMessage::SimObjectData(d)
                                    }
                                    Err(e) => {
                                        warn!("Failed to parse RecvSimObjectData: {}", e);
                                        continue;
                                    }
                                }
                            }
                            SimConnectRecvId::Event => {
                                match RecvEvent::read_from(header, &mut cursor) {
                                    Ok(ev) => SimConnectMessage::Event(ev),
                                    Err(e) => {
                                        warn!("Failed to parse RecvEvent: {}", e);
                                        continue;
                                    }
                                }
                            }
                            SimConnectRecvId::SystemState => {
                                match RecvSystemState::read_from(header, &mut cursor) {
                                    Ok(ss) => SimConnectMessage::SystemState(ss),
                                    Err(e) => {
                                        warn!("Failed to parse RecvSystemState: {}", e);
                                        continue;
                                    }
                                }
                            }
                            SimConnectRecvId::EventFilename => {
                                match RecvEventFilename::read_from(header, &mut cursor) {
                                    Ok(ef) => SimConnectMessage::EventFilename(ef),
                                    Err(e) => {
                                        warn!("Failed to parse RecvEventFilename: {}", e);
                                        continue;
                                    }
                                }
                            }
                            SimConnectRecvId::Exception => {
                                match RecvException::read_from(header, &mut cursor) {
                                    Ok(ex) => {
                                        let what = sent_log_reader
                                            .lock()
                                            .ok()
                                            .and_then(|l| l.get(&ex.send_id).cloned())
                                            .unwrap_or_else(|| "unknown request".to_string());
                                        warn!("SimConnect exception {} ({}) on packet #{} [{}] param index={}",
                                            ex.exception, exception_name(ex.exception), ex.send_id, what, ex.index);
                                        SimConnectMessage::Exception(ex)
                                    }
                                    Err(e) => {
                                        warn!("Failed to parse RecvException: {}", e);
                                        continue;
                                    }
                                }
                            }
                            _ => {
                                SimConnectMessage::Raw(header, packet_buf[12..].to_vec())
                            }
                        };
                        let _ = tx_clone.send(msg);
                    }
                    Err(e) => {
                        error!("Failed to parse packet header: {}", e);
                    }
                }
            }
        });

        Ok(Self {
            write_stream: Arc::new(Mutex::new(write_half)),
            send_seq: AtomicU32::new(1),
            msg_tx,
            event_ids: Mutex::new(HashMap::new()),
            next_event_id: AtomicU32::new(FIRST_DYNAMIC_EVENT_ID),
            sent_log,
            simvar_store,
            simvar_subscribe_lock: Mutex::new(()),
            pmdg_store,
        })
    }

    pub(super) fn record_sent(&self, seq: u32, what: String) {
        if let Ok(mut log) = self.sent_log.lock() {
            if log.len() >= SENT_LOG_CAPACITY {
                let cutoff = seq.saturating_sub(SENT_LOG_CAPACITY as u32);
                log.retain(|k, _| *k > cutoff);
            }
            log.insert(seq, what);
        }
    }

    /// Returns the client event ID mapped to `sim_event_name`, mapping it on first use.
    ///
    /// SimConnect requires each client event ID to be mapped exactly once per connection;
    /// re-mapping an ID to a different name is rejected, so IDs are allocated per name.
    pub async fn ensure_event_mapped(&self, sim_event_name: &str) -> Result<u32> {
        let key = sim_event_name.trim().to_ascii_uppercase();
        if key.is_empty() {
            return Err(anyhow!("empty SimConnect event name"));
        }
        let mut ids = self.event_ids.lock().await;
        if let Some(id) = ids.get(&key) {
            return Ok(*id);
        }
        let id = self.next_event_id.fetch_add(1, Ordering::SeqCst);
        self.map_client_event_to_sim_event(id, &key).await?;
        ids.insert(key, id);
        Ok(id)
    }

    /// Maps (if needed) and fires a sim event on the user aircraft.
    pub async fn transmit_event(&self, sim_event_name: &str, data: u32) -> Result<()> {
        let id = self.ensure_event_mapped(sim_event_name).await?;
        trace!("TransmitClientEvent {} (id {}) data={}", sim_event_name, id, data as i32);
        self.transmit_client_event(SIMCONNECT_OBJECT_ID_USER, id, data).await
    }

    pub fn subscribe(&self) -> broadcast::Receiver<SimConnectMessage> {
        self.msg_tx.subscribe()
    }

    pub async fn send_raw(&self, data: &[u8]) -> Result<()> {
        let mut stream = self.write_stream.lock().await;
        stream.write_all(data).await?;
        stream.flush().await?;
        Ok(())
    }

    pub(super) fn next_seq(&self) -> u32 {
        self.send_seq.fetch_add(1, Ordering::SeqCst)
    }

    pub async fn open(&self, app_name: &str) -> Result<()> {
        let seq = self.next_seq();
        self.record_sent(seq, format!("Open '{}'", app_name));
        let mut builder = SendPacketBuilder::new(SimConnectSendId::Open, seq);
        builder.write_fixed_string(app_name, 256);
        builder.write_u32(0); // dwReserved
        builder.write_u32(0x4653_5800); // dwMagic: "FSX\0"
        builder.write_u32(10); // dwApplicationVersionMajor
        builder.write_u32(0); // dwApplicationVersionMinor
        builder.write_u32(61259); // dwApplicationBuildMajor (SIMCONNECT_BUILD_SP2_XPACK)
        builder.write_u32(0); // dwApplicationBuildMinor
        self.send_raw(&builder.build()).await
    }

    pub async fn map_client_data_name_to_id(&self, name: &str, client_data_id: u32) -> Result<()> {
        let seq = self.next_seq();
        self.record_sent(seq, format!("MapClientDataNameToId '{}' -> 0x{:08X}", name, client_data_id));
        let mut builder = SendPacketBuilder::new(SimConnectSendId::MapClientDataNameToId, seq);
        builder.write_fixed_string(name, 256);
        builder.write_u32(client_data_id);
        self.send_raw(&builder.build()).await
    }

    pub async fn create_client_data(&self, client_data_id: u32, size: u32, read_only: bool) -> Result<()> {
        let seq = self.next_seq();
        self.record_sent(seq, format!("CreateClientData 0x{:08X} ({} bytes)", client_data_id, size));
        let mut builder = SendPacketBuilder::new(SimConnectSendId::CreateClientData, seq);
        builder.write_u32(client_data_id);
        builder.write_u32(size);
        builder.write_u32(if read_only { 1 } else { 0 });
        self.send_raw(&builder.build()).await
    }

    pub async fn add_to_client_data_definition(&self, def_id: u32, offset: u32, size: u32) -> Result<()> {
        let seq = self.next_seq();
        self.record_sent(seq, format!("AddToClientDataDefinition define 0x{:08X} offset {} size {}", def_id, offset, size));
        let mut builder = SendPacketBuilder::new(SimConnectSendId::AddToClientDataDefinition, seq);
        builder.write_u32(def_id);
        builder.write_u32(offset);
        builder.write_u32(size);
        builder.write_f32(0.0); // fEpsilon (4-byte float)
        builder.write_u32(0xFFFF_FFFF); // datum_id: SIMCONNECT_UNUSED
        self.send_raw(&builder.build()).await
    }

    pub async fn add_to_data_definition(
        &self,
        define_id: u32,
        datum_name: &str,
        units_name: &str,
        datum_type: u32,
        epsilon: f32,
    ) -> Result<()> {
        let seq = self.next_seq();
        self.record_sent(seq, format!("AddToDataDefinition '{}' [{}] -> define {}", datum_name, units_name, define_id));
        let mut builder = SendPacketBuilder::new(SimConnectSendId::AddToDataDefinition, seq);
        builder.write_u32(define_id);
        builder.write_fixed_string(datum_name, 256);
        builder.write_fixed_string(units_name, 256);
        builder.write_u32(datum_type);
        builder.write_f32(epsilon);
        builder.write_u32(0xFFFF_FFFF); // DatumID: SIMCONNECT_UNUSED
        self.send_raw(&builder.build()).await
    }

    pub async fn request_data_on_sim_object(
        &self,
        request_id: u32,
        define_id: u32,
        object_id: u32,
        period: u32,
        flags: u32,
    ) -> Result<()> {
        let seq = self.next_seq();
        self.record_sent(seq, format!("RequestDataOnSimObject define {} period {}", define_id, period));
        let mut builder = SendPacketBuilder::new(SimConnectSendId::RequestDataOnSimObject, seq);
        builder.write_u32(request_id);
        builder.write_u32(define_id);
        builder.write_u32(object_id);
        builder.write_u32(period);
        builder.write_u32(flags);
        builder.write_u32(0); // origin
        builder.write_u32(0); // interval
        builder.write_u32(0); // limit
        self.send_raw(&builder.build()).await
    }

    pub async fn set_data_on_sim_object(&self, define_id: u32, object_id: u32, data: &[u8]) -> Result<()> {
        let seq = self.next_seq();
        self.record_sent(seq, format!("SetDataOnSimObject define {}", define_id));
        let mut builder = SendPacketBuilder::new(SimConnectSendId::SetDataOnSimObject, seq);
        builder.write_u32(define_id);
        builder.write_u32(object_id);
        builder.write_u32(0); // flags (untagged)
        builder.write_u32(1); // array count
        builder.write_u32(data.len() as u32); // unit size
        builder.write_bytes(data);
        self.send_raw(&builder.build()).await
    }

    pub async fn request_client_data(&self, client_data_id: u32, req_id: u32, def_id: u32, period: u32) -> Result<()> {
        let seq = self.next_seq();
        self.record_sent(seq, format!("RequestClientData area 0x{:08X} define 0x{:08X}", client_data_id, def_id));
        let mut builder = SendPacketBuilder::new(SimConnectSendId::RequestClientData, seq);
        builder.write_u32(client_data_id);
        builder.write_u32(req_id);
        builder.write_u32(def_id);
        builder.write_u32(period); // SIMCONNECT_CLIENT_DATA_PERIOD
        builder.write_u32(0); // flags
        builder.write_u32(0); // origin
        builder.write_u32(0); // interval
        builder.write_u32(0); // limit
        self.send_raw(&builder.build()).await
    }

    pub async fn request_client_data_with_flags(
        &self,
        client_data_id: u32,
        req_id: u32,
        def_id: u32,
        period: u32,
        flags: u32,
    ) -> Result<()> {
        let seq = self.next_seq();
        self.record_sent(seq, format!("RequestClientData area 0x{:08X} define 0x{:08X}", client_data_id, def_id));
        let mut builder = SendPacketBuilder::new(SimConnectSendId::RequestClientData, seq);
        builder.write_u32(client_data_id);
        builder.write_u32(req_id);
        builder.write_u32(def_id);
        builder.write_u32(period);
        builder.write_u32(flags);
        builder.write_u32(0); // origin
        builder.write_u32(0); // interval
        builder.write_u32(0); // limit
        self.send_raw(&builder.build()).await
    }

    pub async fn set_client_data(&self, client_data_id: u32, def_id: u32, data: &[u8]) -> Result<()> {
        let seq = self.next_seq();
        self.record_sent(seq, format!("SetClientData area 0x{:08X} ({} bytes)", client_data_id, data.len()));
        let mut builder = SendPacketBuilder::new(SimConnectSendId::SetClientData, seq);
        builder.write_u32(client_data_id);
        builder.write_u32(def_id);
        builder.write_u32(0); // flags
        builder.write_u32(0); // reserved
        builder.write_u32(data.len() as u32);
        builder.write_bytes(data);
        self.send_raw(&builder.build()).await
    }

    pub async fn subscribe_to_system_event(&self, event_id: u32, event_name: &str) -> Result<()> {
        let seq = self.next_seq();
        self.record_sent(seq, format!("SubscribeToSystemEvent '{}'", event_name));
        let mut builder = SendPacketBuilder::new(SimConnectSendId::SubscribeToSystemEvent, seq);
        builder.write_u32(event_id);
        builder.write_fixed_string(event_name, 256);
        self.send_raw(&builder.build()).await
    }

    pub async fn request_system_state(&self, request_id: u32, state_name: &str) -> Result<()> {
        let seq = self.next_seq();
        self.record_sent(seq, format!("RequestSystemState '{}'", state_name));
        let mut builder = SendPacketBuilder::new(SimConnectSendId::RequestSystemState, seq);
        builder.write_u32(request_id);
        builder.write_fixed_string(state_name, 256);
        self.send_raw(&builder.build()).await
    }

    pub async fn map_client_event_to_sim_event(&self, event_id: u32, sim_event_name: &str) -> Result<()> {
        let seq = self.next_seq();
        self.record_sent(seq, format!("MapClientEventToSimEvent '{}' -> id {}", sim_event_name, event_id));
        let mut builder = SendPacketBuilder::new(SimConnectSendId::MapClientEventToSimEvent, seq);
        builder.write_u32(event_id);
        builder.write_fixed_string(sim_event_name, 256);
        self.send_raw(&builder.build()).await
    }

    pub async fn transmit_client_event(&self, obj_id: u32, event_id: u32, data: u32) -> Result<()> {
        let seq = self.next_seq();
        self.record_sent(seq, format!("TransmitClientEvent id {} data={}", event_id, data as i32));
        let mut builder = SendPacketBuilder::new(SimConnectSendId::TransmitClientEvent, seq);
        builder.write_u32(obj_id);
        builder.write_u32(event_id);
        builder.write_u32(data);
        // No notification group is defined, so the group field carries a priority instead.
        builder.write_u32(SIMCONNECT_GROUP_PRIORITY_HIGHEST);
        builder.write_u32(SIMCONNECT_EVENT_FLAG_GROUPID_IS_PRIORITY);
        self.send_raw(&builder.build()).await
    }
}


/// Human-readable name for a `SIMCONNECT_EXCEPTION` code.
pub fn exception_name(code: u32) -> &'static str {
    match code {
        0 => "NONE",
        1 => "ERROR",
        2 => "SIZE_MISMATCH",
        3 => "UNRECOGNIZED_ID",
        4 => "UNOPENED",
        5 => "VERSION_MISMATCH",
        6 => "TOO_MANY_GROUPS",
        7 => "NAME_UNRECOGNIZED",
        8 => "TOO_MANY_EVENT_NAMES",
        9 => "EVENT_ID_DUPLICATE",
        10 => "TOO_MANY_MAPS",
        11 => "TOO_MANY_OBJECTS",
        12 => "TOO_MANY_REQUESTS",
        13 => "WEATHER_INVALID_PORT",
        14 => "WEATHER_INVALID_METAR",
        15 => "WEATHER_UNABLE_TO_GET_OBSERVATION",
        16 => "WEATHER_UNABLE_TO_CREATE_STATION",
        17 => "WEATHER_UNABLE_TO_REMOVE_STATION",
        18 => "INVALID_DATA_TYPE",
        19 => "INVALID_DATA_SIZE",
        20 => "DATA_ERROR",
        21 => "INVALID_ARRAY",
        22 => "CREATE_OBJECT_FAILED",
        23 => "LOAD_FLIGHTPLAN_FAILED",
        24 => "OPERATION_INVALID_FOR_OBJECT_TYPE",
        25 => "ILLEGAL_OPERATION",
        26 => "ALREADY_SUBSCRIBED",
        27 => "INVALID_ENUM",
        28 => "DEFINITION_ERROR",
        29 => "DUPLICATE_ID",
        30 => "DATUM_ID",
        31 => "OUT_OF_BOUNDS",
        32 => "ALREADY_CREATED",
        33 => "OBJECT_OUTSIDE_REALITY_BUBBLE",
        34 => "OBJECT_CONTAINER",
        35 => "OBJECT_AI",
        36 => "OBJECT_ATC",
        37 => "OBJECT_SCHEDULE",
        _ => "UNKNOWN",
    }
}
