use anyhow::{anyhow, Result};
use std::io::Cursor;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{broadcast, Mutex};
use tracing::{error, info, trace, warn};

use super::protocol::*;

pub struct SimConnectClient {
    write_stream: Arc<Mutex<tokio::net::tcp::OwnedWriteHalf>>,
    send_seq: AtomicU32,
    msg_tx: broadcast::Sender<SimConnectMessage>,
}

impl SimConnectClient {
    pub async fn connect(addr: &str) -> Result<Self> {
        info!("Connecting to SimConnect server at {}", addr);
        let stream = TcpStream::connect(addr).await
            .map_err(|e| anyhow!("Failed to connect to SimConnect at {}: {}. Ensure MSFS is running with network SimConnect enabled.", addr, e))?;

        let (mut read_half, write_half) = stream.into_split();
        let (msg_tx, _) = broadcast::channel(512);
        let tx_clone = msg_tx.clone();

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
                                    Ok(cd) => SimConnectMessage::ClientData(cd),
                                    Err(e) => {
                                        warn!("Failed to parse RecvClientData: {}", e);
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
                            SimConnectRecvId::Exception => {
                                match RecvException::read_from(header, &mut cursor) {
                                    Ok(ex) => {
                                        warn!("SimConnect exception: code={} send_id={} index={}",
                                            ex.exception, ex.send_id, ex.index);
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
        })
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

    fn next_seq(&self) -> u32 {
        self.send_seq.fetch_add(1, Ordering::SeqCst)
    }

    pub async fn open(&self, app_name: &str) -> Result<()> {
        let seq = self.next_seq();
        let mut builder = SendPacketBuilder::new(SimConnectSendId::Open, seq);
        builder.write_fixed_string(app_name, 256);
        self.send_raw(&builder.build()).await
    }

    pub async fn map_client_data_name_to_id(&self, name: &str, client_data_id: u32) -> Result<()> {
        let seq = self.next_seq();
        let mut builder = SendPacketBuilder::new(SimConnectSendId::MapClientDataNameToId, seq);
        builder.write_fixed_string(name, 256);
        builder.write_u32(client_data_id);
        self.send_raw(&builder.build()).await
    }

    pub async fn create_client_data(&self, client_data_id: u32, size: u32, read_only: bool) -> Result<()> {
        let seq = self.next_seq();
        let mut builder = SendPacketBuilder::new(SimConnectSendId::CreateClientData, seq);
        builder.write_u32(client_data_id);
        builder.write_u32(size);
        builder.write_u32(if read_only { 1 } else { 0 });
        self.send_raw(&builder.build()).await
    }

    pub async fn add_to_client_data_definition(&self, def_id: u32, offset: u32, size: u32) -> Result<()> {
        let seq = self.next_seq();
        let mut builder = SendPacketBuilder::new(SimConnectSendId::AddToClientDataDefinition, seq);
        builder.write_u32(def_id);
        builder.write_u32(offset);
        builder.write_u32(size);
        builder.write_f64(0.0); // epsilon
        builder.write_u32(0); // datum_id
        self.send_raw(&builder.build()).await
    }

    pub async fn request_client_data(&self, client_data_id: u32, req_id: u32, def_id: u32, period: u32) -> Result<()> {
        let seq = self.next_seq();
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

    pub async fn set_client_data(&self, client_data_id: u32, def_id: u32, data: &[u8]) -> Result<()> {
        let seq = self.next_seq();
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
        let mut builder = SendPacketBuilder::new(SimConnectSendId::SubscribeToSystemEvent, seq);
        builder.write_u32(event_id);
        builder.write_fixed_string(event_name, 256);
        self.send_raw(&builder.build()).await
    }

    pub async fn map_client_event_to_sim_event(&self, event_id: u32, sim_event_name: &str) -> Result<()> {
        let seq = self.next_seq();
        let mut builder = SendPacketBuilder::new(SimConnectSendId::MapClientEventToSimEvent, seq);
        builder.write_u32(event_id);
        builder.write_fixed_string(sim_event_name, 256);
        self.send_raw(&builder.build()).await
    }

    pub async fn transmit_client_event(&self, obj_id: u32, event_id: u32, data: u32) -> Result<()> {
        let seq = self.next_seq();
        let mut builder = SendPacketBuilder::new(SimConnectSendId::TransmitClientEvent, seq);
        builder.write_u32(obj_id);
        builder.write_u32(event_id);
        builder.write_u32(data);
        builder.write_u32(0); // group_id
        builder.write_u32(0); // flags
        self.send_raw(&builder.build()).await
    }
}
