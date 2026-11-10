use aerodeck::profile::executor::{axis_position, scale_axis, sim_axis_range, sim_event_name};
use aerodeck::profile::schema::bound_to_matches;
use aerodeck::simconnect::SimConnectClient;
use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;

/// Reads one SimConnect packet and returns (type id without 0xF0000000, body after the 16-byte header).
async fn read_packet(stream: &mut tokio::net::TcpStream) -> (u32, Vec<u8>) {
    let mut len = [0u8; 4];
    stream.read_exact(&mut len).await.unwrap();
    let size = u32::from_le_bytes(len) as usize;
    let mut rest = vec![0u8; size - 4];
    stream.read_exact(&mut rest).await.unwrap();
    let ty = u32::from_le_bytes(rest[4..8].try_into().unwrap()) & 0x0FFF_FFFF;
    (ty, rest[12..].to_vec())
}

fn u32_at(b: &[u8], idx: usize) -> u32 {
    u32::from_le_bytes(b[idx * 4..idx * 4 + 4].try_into().unwrap())
}

#[tokio::test]
async fn transmit_event_maps_once_and_uses_priority_group() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    let server = tokio::spawn(async move {
        let (mut s, _) = listener.accept().await.unwrap();
        let mut packets = Vec::new();
        for _ in 0..4 {
            packets.push(read_packet(&mut s).await);
        }
        packets
    });

    let sc = SimConnectClient::connect(&addr).await.unwrap();
    sc.transmit_event("AP_MASTER", 0).await.unwrap();
    sc.transmit_event("ap_master", 0).await.unwrap(); // same event, no re-map
    sc.transmit_event("AXIS_ELEVATOR_SET", (-16383i32) as u32).await.unwrap();

    let p = server.await.unwrap();
    // Map AP_MASTER, Transmit, Transmit, Map AXIS_ELEVATOR_SET (then its Transmit, not read)
    assert_eq!(p[0].0, 4, "first packet must be MapClientEventToSimEvent");
    let ap_id = u32_at(&p[0].1, 0);
    assert!(p[0].1[4..].starts_with(b"AP_MASTER\0"));

    for t in &p[1..3] {
        assert_eq!(t.0, 5, "TransmitClientEvent");
        assert_eq!(u32_at(&t.1, 0), 0, "user object");
        assert_eq!(u32_at(&t.1, 1), ap_id);
        assert_eq!(u32_at(&t.1, 3), 1, "SIMCONNECT_GROUP_PRIORITY_HIGHEST");
        assert_eq!(u32_at(&t.1, 4), 0x10, "GROUPID_IS_PRIORITY");
    }

    assert_eq!(p[3].0, 4);
    assert_ne!(u32_at(&p[3].1, 0), ap_id, "distinct events get distinct client IDs");
}

#[test]
fn axis_scaling_matches_sim_event_ranges() {
    assert_eq!(sim_axis_range("AXIS_ELEVATOR_SET"), (-16383, 16383));
    assert_eq!(sim_axis_range("THROTTLE1_AXIS_SET_EX1"), (-16383, 16383));
    assert_eq!(sim_axis_range("THROTTLE_SET"), (0, 16383));

    // Unsigned evdev device (0..65535) must cover the full bipolar range.
    let r = sim_axis_range("AXIS_AILERONS_SET");
    assert_eq!(scale_axis(axis_position(0, 0, 65535), r), -16383);
    assert_eq!(scale_axis(axis_position(65535, 0, 65535), r), 16383);
    // Signed device centred at 0.
    assert_eq!(scale_axis(axis_position(0, -512, 512), r), 0);
    assert_eq!(scale_axis(axis_position(1023, 0, 1023), sim_axis_range("THROTTLE_SET")), 16383);
}

#[test]
fn targets_and_axis_names() {
    assert_eq!(sim_event_name("SIMCONNECT:AP_MASTER"), Some("AP_MASTER"));
    assert_eq!(sim_event_name("GEAR_TOGGLE"), Some("GEAR_TOGGLE"));
    assert_eq!(sim_event_name("LVAR:A32NX_FOO"), None);

    assert!(bound_to_matches("X_AXIS", "ABS_X"));
    assert!(bound_to_matches("RZ_AXIS", "ABS_RZ"));
    assert!(!bound_to_matches("X_AXIS", "ABS_Y"));
}

#[tokio::test]
async fn simvar_subscription_round_trip() {
    use tokio::io::AsyncWriteExt;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    let server = tokio::spawn(async move {
        let (mut s, _) = listener.accept().await.unwrap();
        let (add_ty, add) = read_packet(&mut s).await;
        let (req_ty, req) = read_packet(&mut s).await;
        assert_eq!(add_ty, 12, "AddToDataDefinition");
        assert_eq!(&add[4..27], b"NAV STANDBY FREQUENCY:2");
        assert!(add[260..].starts_with(b"MHz\0"), "unit");
        assert_eq!(u32_at(&add[516..], 0), 4, "FLOAT64");
        assert_eq!(req_ty, 14, "RequestDataOnSimObject");
        let define = u32_at(&add, 0);
        assert_eq!(u32_at(&req, 1), define);

        // Reply: SIMCONNECT_RECV_SIMOBJECT_DATA with one FLOAT64
        let mut pkt = Vec::new();
        for v in [0u32, 4, 8, define, 0, define, 0, 0, 1, 1] {
            pkt.extend_from_slice(&v.to_le_bytes());
        }
        pkt.extend_from_slice(&110.55f64.to_le_bytes());
        let len = pkt.len() as u32;
        pkt[0..4].copy_from_slice(&len.to_le_bytes());
        s.write_all(&pkt).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    });

    let sc = SimConnectClient::connect(&addr).await.unwrap();
    let mut changes = sc.simvar_changes();
    let key = sc.subscribe_simvar("A:nav standby frequency:2").await.unwrap();
    assert_eq!(key, "NAV STANDBY FREQUENCY:2");

    let (k, v) = tokio::time::timeout(std::time::Duration::from_secs(2), changes.recv()).await.unwrap().unwrap();
    assert_eq!(k, key);
    assert_eq!(v, 110.55);
    assert_eq!(sc.simvar("NAV STANDBY FREQUENCY:2"), Some(110.55));
    server.await.unwrap();
}

#[test]
fn change_value_operations() {
    use aerodeck::profile::executor::apply_operation;
    assert_eq!(apply_operation(5.0, "", 1.0, "PRESS"), 1.0);
    assert!((apply_operation(110.50, "", 0.05, "TUNER_INNER_CLOCKWISE") - 110.55).abs() < 1e-9);
    assert!((apply_operation(110.50, "", 0.05, "TUNER_INNER_COUNTERCLOCKWISE") - 110.45).abs() < 1e-9);
    assert_eq!(apply_operation(2.0, "Increment", 1.0, "PRESS"), 3.0);
    assert_eq!(apply_operation(1.0, "Toggle", 0.0, "PRESS"), 0.0);
    assert_eq!(apply_operation(0.0, "Toggle", 0.0, "PRESS"), 1.0);
    assert_eq!(apply_operation(0b100.into(), "ToggleBit", 2.0, "PRESS"), 0.0);
}

#[test]
fn real_profiles_expose_simvars_and_parameters() {
    use aerodeck::profile::executor::collect_simvar_refs;
    use aerodeck::profile::{EventActionItem, ProfileParser};

    let p = ProfileParser::parse_file("tests/fixtures/profiles/spad_online_11909_PMDG_737_MSFS.xml").unwrap();
    let refs = collect_simvar_refs(&p);
    assert!(refs.iter().any(|r| r == "NAV STANDBY FREQUENCY:2"), "{:?}", refs);

    let p = ProfileParser::parse_file("tests/fixtures/profiles/PMDG 777.xml").unwrap();
    let mut found_param = false;
    for dev in p.devices.iter().flat_map(|d| &d.items) {
        for page in &dev.pages {
            for ev in page.events.iter().flat_map(|e| &e.items) {
                for def in ev.event_definitions.iter().flat_map(|d| &d.items) {
                    for a in def.actions.iter().flat_map(|a| &a.items) {
                        if let EventActionItem::Control(c) = a {
                            if c.value.is_none() && !c.parameters.is_empty() {
                                assert_eq!(c.effective_value(), c.parameters[0].value.as_deref());
                                found_param = true;
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(found_param, "<Parameter Value> children must be read");
}

#[test]
fn conditions_read_simvars() {
    use aerodeck::profile::{ConditionEvaluator, EventConditionSimple};
    use std::collections::HashMap;
    let lvars = HashMap::new();
    let locals = HashMap::new();
    let mut simvars = HashMap::new();
    simvars.insert("ENG N2 RPM:1".to_string(), 22.0);
    let ev = ConditionEvaluator::new(&lvars, &locals).with_simvars(&simvars);
    let cond = |c: &str, v: &str| EventConditionSimple {
        target: "SIMCONNECT:ENG N2 RPM:1".into(),
        comparator: Some(c.into()),
        value: Some(v.into()),
    };
    assert!(ev.evaluate_simple(&cond("GreaterOrEqual", "5.0")));
    assert!(!ev.evaluate_simple(&cond("Less", "5.0")));
    // Unknown providers must not compare their own name as a literal.
    let pmdg = EventConditionSimple { target: "PMDGNG3:MCP_Course_0".into(), comparator: None, value: Some("PMDGNG3:MCP_Course_0".into()) };
    assert!(!ev.evaluate_simple(&pmdg));
}
