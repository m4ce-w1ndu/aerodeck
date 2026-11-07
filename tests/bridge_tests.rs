use spad_linx::bridge::*;

#[test]
fn test_host_request_binary_layout() {
    let req = HostRequest::new(OP_CONNECT, "TEST_VAR", 123.45);
    let bytes = req.to_bytes();

    assert_eq!(bytes.len(), 276);
    // Secret (4 bytes)
    assert_eq!(&bytes[0..4], &HOST_SECRET.to_le_bytes());
    // Operation (4 bytes)
    assert_eq!(&bytes[4..8], &OP_CONNECT.to_le_bytes());
    // Variable name starts at byte 12
    assert!(bytes[12..].starts_with(b"TEST_VAR"));
}

#[test]
fn test_host_request2_binary_layout() {
    let req = HostRequest2::new(OP_EXECUTE_CALCULATOR_CODE, "(L:A32NX_AUTOPILOT_1_ACTIVE, bool)", CALC_RET_DOUBLE);
    let bytes = req.to_bytes();

    assert_eq!(bytes.len(), 1044);
    assert_eq!(&bytes[0..4], &HOST_SECRET.to_le_bytes());
    assert_eq!(&bytes[4..8], &OP_EXECUTE_CALCULATOR_CODE.to_le_bytes());
    assert_eq!(&bytes[8..12], &CALC_RET_DOUBLE.to_le_bytes());
    assert!(bytes[12..].starts_with(b"(L:A32NX_AUTOPILOT_1_ACTIVE, bool)"));
}

#[test]
fn test_host_event_parsing() {
    let mut raw = Vec::new();
    raw.extend_from_slice(&HOST_SECRET.to_le_bytes());
    raw.extend_from_slice(&OP_AIRCRAFT_NAME.to_le_bytes());
    raw.extend_from_slice(&1i32.to_le_bytes()); // has_message = 1
    raw.extend_from_slice(&5.0f64.to_le_bytes()); // value = 5.0
    let mut msg = [0u8; 1024];
    msg[..20].copy_from_slice(b"Cessna 172 Skyhawk\0\0");
    raw.extend_from_slice(&msg);

    let event = HostEvent::from_bytes(&raw).expect("Should parse HostEvent");
    assert_eq!(event.secret, HOST_SECRET);
    assert_eq!(event.operation, OP_AIRCRAFT_NAME);
    assert_eq!(event.value, 5.0);
    assert_eq!(event.message, "Cessna 172 Skyhawk");
}

#[test]
fn test_vardump_parsing() {
    let mut raw = Vec::new();
    raw.extend_from_slice(&HOST_SECRET.to_le_bytes());
    raw.extend_from_slice(&0i32.to_le_bytes()); // start_index = 0
    raw.extend_from_slice(&3i32.to_le_bytes()); // count = 3
    let mut names = [0u8; 4000];
    let payload = b"A32NX_AUTOPILOT_1_ACTIVE;A32NX_FMA_EXPEDITE_MODE;A32NX_EFIS_L_ND_RANGE\0";
    names[..payload.len()].copy_from_slice(payload);
    raw.extend_from_slice(&names);

    let dump = VarDump::from_bytes(&raw).expect("Should parse VarDump");
    assert_eq!(dump.variable_names.len(), 3);
    assert_eq!(dump.variable_names[0], "A32NX_AUTOPILOT_1_ACTIVE");
    assert_eq!(dump.variable_names[1], "A32NX_FMA_EXPEDITE_MODE");
    assert_eq!(dump.variable_names[2], "A32NX_EFIS_L_ND_RANGE");
}
