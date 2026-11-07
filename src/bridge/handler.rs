use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex, oneshot};
use tokio::time::{sleep, Duration, timeout};
use tracing::{debug, error, info, trace, warn};

use crate::simconnect::{SimConnectClient, SimConnectMessage};
use super::protocol::*;

pub struct SpadLvarBridge {
    client: Arc<SimConnectClient>,
    known_lvars: Arc<Mutex<Vec<String>>>,
    lvar_values: Arc<Mutex<HashMap<String, f64>>>,
    connected: Arc<Mutex<bool>>,
    aircraft_name: Arc<Mutex<String>>,
    pending_calc: Arc<Mutex<Option<oneshot::Sender<Result<HostEvent>>>>>,
}

impl SpadLvarBridge {
    pub fn new(client: Arc<SimConnectClient>) -> Self {
        Self {
            client,
            known_lvars: Arc::new(Mutex::new(Vec::new())),
            lvar_values: Arc::new(Mutex::new(HashMap::new())),
            connected: Arc::new(Mutex::new(false)),
            aircraft_name: Arc::new(Mutex::new(String::new())),
            pending_calc: Arc::new(Mutex::new(None)),
        }
    }

    /// Initializes client data areas and starts listening for WASM bridge packets
    pub async fn start(&self) -> Result<()> {
        info!("Registering SPAD.neXt LVAR Bridge ClientData areas with SimConnect...");

        // 1. Map names to ClientData IDs
        self.client.map_client_data_name_to_id("SPAD_SC_DYN_VARDATA_ID", SPAD_SC_DYN_VARDATA_ID).await?;
        self.client.map_client_data_name_to_id("SPAD_SC_DYN_VARDATA_DATA", SPAD_SC_DYN_VARDATA_DATA).await?;
        self.client.map_client_data_name_to_id("SPAD_SC_DYN_VARDUMP_ID", SPAD_SC_DYN_VARDUMP_ID).await?;
        self.client.map_client_data_name_to_id("SPAD_SC_DYN_VARDUMP_DATA", SPAD_SC_DYN_VARDUMP_DATA).await?;
        self.client.map_client_data_name_to_id("SPAD_SC_DYN_HOSTRESQUEST_ID", SPAD_SC_DYN_HOSTREQUEST_ID).await?;
        self.client.map_client_data_name_to_id("SPAD_SC_DYN_HOSTRESQUEST_DATA", SPAD_SC_DYN_HOSTREQUEST_DATA).await?;
        self.client.map_client_data_name_to_id("SPAD_SC_DYN_HOSTRESQUEST2_ID", SPAD_SC_DYN_HOSTREQUEST2_ID).await?;
        self.client.map_client_data_name_to_id("SPAD_SC_DYN_HOSTRESQUEST2_DATA", SPAD_SC_DYN_HOSTREQUEST2_DATA).await?;
        self.client.map_client_data_name_to_id("SPAD_SC_DYN_HOSTEVENT_ID", SPAD_SC_DYN_HOSTEVENT_ID).await?;
        self.client.map_client_data_name_to_id("SPAD_SC_DYN_HOSTEVENT_DATA", SPAD_SC_DYN_HOSTEVENT_DATA).await?;

        // 2. Add definitions
        // HostRequest (276 bytes)
        self.client.add_to_client_data_definition(SPAD_SC_DYN_HOSTREQUEST_DATA, 0, 276).await?;
        // HostRequest2 (1044 bytes)
        self.client.add_to_client_data_definition(SPAD_SC_DYN_HOSTREQUEST2_DATA, 0, 1044).await?;
        // HostEvent (1044 bytes)
        self.client.add_to_client_data_definition(SPAD_SC_DYN_HOSTEVENT_DATA, 0, 1044).await?;
        // VarDump (4012 bytes)
        self.client.add_to_client_data_definition(SPAD_SC_DYN_VARDUMP_DATA, 0, 4012).await?;
        // DynamicData (268 bytes)
        self.client.add_to_client_data_definition(SPAD_SC_DYN_VARDATA_DATA, 0, 268).await?;

        // 3. Request client data streams
        // SIMCONNECT_CLIENT_DATA_PERIOD_ON_SET = 1
        self.client.request_client_data(SPAD_SC_DYN_HOSTEVENT_ID, 101, SPAD_SC_DYN_HOSTEVENT_DATA, 1).await?;
        self.client.request_client_data(SPAD_SC_DYN_VARDUMP_ID, 102, SPAD_SC_DYN_VARDUMP_DATA, 1).await?;
        self.client.request_client_data(SPAD_SC_DYN_VARDATA_ID, 103, SPAD_SC_DYN_VARDATA_DATA, 1).await?;

        // Spawn packet processing loop
        let mut rx = self.client.subscribe();
        let known_lvars = self.known_lvars.clone();
        let lvar_values = self.lvar_values.clone();
        let connected = self.connected.clone();
        let aircraft_name = self.aircraft_name.clone();
        let pending_calc = self.pending_calc.clone();

        tokio::spawn(async move {
            while let Ok(msg) = rx.recv().await {
                if let SimConnectMessage::ClientData(cd) = msg {
                    if cd.define_id == SPAD_SC_DYN_HOSTEVENT_DATA {
                        if let Ok(event) = HostEvent::from_bytes(&cd.data) {
                            debug!("Bridge HostEvent received: op={} val={} msg='{}'", event.operation, event.value, event.message);
                            match event.operation {
                                OP_CONNECT => {
                                    info!("LVAR Bridge WASM connected! Bridge Version: {} (API val: {})", event.message, event.value);
                                    let mut c = connected.lock().await;
                                    *c = true;
                                }
                                OP_AIRCRAFT_NAME => {
                                    info!("LVAR Bridge: Active Aircraft is '{}'", event.message);
                                    let mut ac = aircraft_name.lock().await;
                                    *ac = event.message.clone();
                                }
                                EVENT_STATUS_SUCCESS | EVENT_STATUS_ERROR => {
                                    let mut p = pending_calc.lock().await;
                                    if let Some(tx) = p.take() {
                                        let _ = tx.send(Ok(event));
                                    }
                                }
                                _ => {}
                            }
                        }
                    } else if cd.define_id == SPAD_SC_DYN_VARDUMP_DATA {
                        if let Ok(dump) = VarDump::from_bytes(&cd.data) {
                            let mut lvars = known_lvars.lock().await;
                            for name in dump.variable_names {
                                if !lvars.contains(&name) {
                                    lvars.push(name);
                                }
                            }
                            info!("LVAR Bridge: received variable batch (Total LVARs discovered: {})", lvars.len());
                        }
                    } else if cd.define_id == SPAD_SC_DYN_VARDATA_DATA {
                        if let Ok(dyn_data) = DynamicData::from_bytes(&cd.data) {
                            trace!("LVAR update: {} = {}", dyn_data.name, dyn_data.value);
                            let mut vals = lvar_values.lock().await;
                            vals.insert(dyn_data.name, dyn_data.value);
                        }
                    }
                }
            }
        });

        // 4. Send Connect Handshake
        self.send_connect_handshake().await?;

        Ok(())
    }

    /// Sends the OP_CONNECT handshake packet to the WASM bridge
    pub async fn send_connect_handshake(&self) -> Result<()> {
        info!("Sending SPAD LVAR Bridge handshake (OP_CONNECT)...");
        let req = HostRequest::new(OP_CONNECT, "", 0.0);
        self.client.set_client_data(SPAD_SC_DYN_HOSTREQUEST_ID, SPAD_SC_DYN_HOSTREQUEST_DATA, &req.to_bytes()).await
    }

    /// Request aircraft name from bridge
    pub async fn request_aircraft_name(&self) -> Result<()> {
        let req = HostRequest::new(OP_REQ_AIRCRAFT_NAME, "", 0.0);
        self.client.set_client_data(SPAD_SC_DYN_HOSTREQUEST_ID, SPAD_SC_DYN_HOSTREQUEST_DATA, &req.to_bytes()).await
    }

    /// Start discovery of LVARs (digging through gauge named variables)
    pub async fn start_lvar_discovery(&self) -> Result<()> {
        info!("Triggering LVAR discovery in MSFS WASM module...");
        let req = HostRequest::new(OP_START_LVAR_SEARCH, "", 0.0);
        self.client.set_client_data(SPAD_SC_DYN_HOSTREQUEST_ID, SPAD_SC_DYN_HOSTREQUEST_DATA, &req.to_bytes()).await?;

        // Request list transmission
        let req2 = HostRequest::new(OP_SEND_LVARS, "", 0.0);
        self.client.set_client_data(SPAD_SC_DYN_HOSTREQUEST_ID, SPAD_SC_DYN_HOSTREQUEST_DATA, &req2.to_bytes()).await
    }

    /// Execute RPN Calculator code in MSFS Gauge context
    pub async fn execute_calculator_code(&self, code: &str, ret_type: i32) -> Result<HostEvent> {
        let (tx, rx) = oneshot::channel();
        {
            let mut p = self.pending_calc.lock().await;
            *p = Some(tx);
        }

        let req = HostRequest2::new(OP_EXECUTE_CALCULATOR_CODE, code, ret_type);
        self.client.set_client_data(SPAD_SC_DYN_HOSTREQUEST2_ID, SPAD_SC_DYN_HOSTREQUEST2_DATA, &req.to_bytes()).await?;

        match timeout(Duration::from_secs(3), rx).await {
            Ok(Ok(res)) => res,
            Ok(Err(_)) => Err(anyhow!("Calculator code execution response dropped")),
            Err(_) => {
                let mut p = self.pending_calc.lock().await;
                p.take();
                Err(anyhow!("Calculator code execution timed out after 3 seconds for code: {}", code))
            }
        }
    }

    /// Set an LVAR value by executing the RPN assignment `(>L:name, unit)`
    pub async fn set_lvar(&self, name: &str, value: f64) -> Result<()> {
        let clean_name = name.trim_start_matches("L:").trim();
        let rpn = format!("{} (>L:{})", value, clean_name);
        self.execute_calculator_code(&rpn, CALC_RET_VOID).await?;
        Ok(())
    }

    /// Read an LVAR value by executing `(L:name, number)`
    pub async fn get_lvar(&self, name: &str) -> Result<f64> {
        let clean_name = name.trim_start_matches("L:").trim();
        let rpn = format!("(L:{}, number)", clean_name);
        let event = self.execute_calculator_code(&rpn, CALC_RET_DOUBLE).await?;
        Ok(event.value)
    }

    pub async fn get_known_lvars(&self) -> Vec<String> {
        self.known_lvars.lock().await.clone()
    }

    pub async fn get_active_aircraft(&self) -> String {
        self.aircraft_name.lock().await.clone()
    }

    pub async fn is_connected(&self) -> bool {
        *self.connected.lock().await
    }
}
