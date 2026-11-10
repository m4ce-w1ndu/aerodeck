//! Checks against the PMDG SDK headers installed in the local MSFS prefix.
//! Skipped (passes trivially) on machines without PMDG aircraft.

use aerodeck::pmdg::{self, PmdgFamily};
use std::collections::BTreeSet;

fn profile_pmdg_targets() -> BTreeSet<(String, String)> {
    let mut out = BTreeSet::new();
    for entry in std::fs::read_dir("tests/fixtures/profiles").unwrap().flatten() {
        let text = std::fs::read_to_string(entry.path()).unwrap_or_default();
        for cap in text.split('"').filter(|s| s.starts_with("PMDG") && s.contains(':')) {
            let (p, n) = cap.split_once(':').unwrap();
            out.insert((p.to_string(), n.to_string()));
        }
    }
    out
}

#[test]
fn installed_headers_resolve_every_profile_reference() {
    let sdks = pmdg::installed_sdks();
    if sdks.is_empty() {
        eprintln!("no PMDG SDK installed; skipping");
        return;
    }
    for sdk in sdks {
        eprintln!(
            "{:?}: {} events, {} fields, data struct {} bytes, area '{}'",
            sdk.family, sdk.events.len(), sdk.fields.len(), sdk.data_size, sdk.data_name
        );
        assert!(sdk.events.len() > 500, "suspiciously few events");
        assert!(sdk.data_size > 500 && sdk.data_size < 8192);
    }

    let mut unresolved = Vec::new();
    let mut checked = 0;
    for (prefix, name) in profile_pmdg_targets() {
        let Some(family) = PmdgFamily::from_profile_prefix(&prefix) else {
            unresolved.push(format!("{}:{} (unknown prefix)", prefix, name));
            continue;
        };
        let Some(sdk) = sdks.iter().find(|s| s.family == family) else { continue };
        // Raw-offset addressing (`PMDGNG3:0000:0118`): semantics unknown; only used for a
        // Saitek panel LED in one profile, so it is deliberately not supported.
        if name.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            continue;
        }
        checked += 1;
        let ok = if name.to_ascii_uppercase().starts_with("EVT") {
            sdk.event_id(&name).is_some()
        } else {
            sdk.find_field(&name).is_some()
        };
        if !ok {
            unresolved.push(format!("{}:{}", prefix, name));
        }
    }
    eprintln!("checked {} PMDG references", checked);
    assert!(unresolved.is_empty(), "unresolved PMDG references:\n{}", unresolved.join("\n"));
}

#[test]
fn ng3_known_values() {
    let Some(sdk) = pmdg::installed_sdks().iter().find(|s| s.family == PmdgFamily::Ng3) else { return };
    assert_eq!(sdk.event_id("EVT_OH_ELEC_BATTERY_SWITCH"), Some(69633));
    assert_eq!(sdk.data_id, 0x4E473331);
    // First fields: unsigned char then bool[...]: offsets 0 and 1.
    assert_eq!(sdk.fields[0].offset, 0);
    assert_eq!(sdk.fields[1].offset, 1);
}

async fn read_packet(s: &mut tokio::net::TcpStream) -> (u32, Vec<u8>) {
    use tokio::io::AsyncReadExt;
    let mut len = [0u8; 4];
    s.read_exact(&mut len).await.unwrap();
    let mut rest = vec![0u8; u32::from_le_bytes(len) as usize - 4];
    s.read_exact(&mut rest).await.unwrap();
    (u32::from_le_bytes(rest[4..8].try_into().unwrap()) & 0x0FFF_FFFF, rest[12..].to_vec())
}

fn u32_at(b: &[u8], i: usize) -> u32 {
    u32::from_le_bytes(b[i * 4..i * 4 + 4].try_into().unwrap())
}

#[tokio::test]
async fn ng3_wire_round_trip() {
    use tokio::io::AsyncWriteExt;
    let sdks = pmdg::installed_sdks();
    let Some(ng3) = sdks.iter().find(|s| s.family == PmdgFamily::Ng3).cloned() else { return };
    let n_sdks = sdks.len();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let ng3_srv = ng3.clone();
    let server = tokio::spawn(async move {
        let (mut s, _) = listener.accept().await.unwrap();
        let mut ng3_request = None;
        for _ in 0..n_sdks {
            let (t1, map) = read_packet(&mut s).await;
            let (t2, def) = read_packet(&mut s).await;
            let (t3, req) = read_packet(&mut s).await;
            assert_eq!((t1, t2, t3), (55, 57, 59));
            // MapClientDataNameToId body: szClientDataName[256], then the ID
            if map.starts_with(ng3_srv.data_name.as_bytes()) {
                assert_eq!(u32_at(&map[256..], 0), ng3_srv.data_id);
                assert_eq!(u32_at(&def, 2), ng3_srv.data_size as u32);
                assert_eq!(u32_at(&req, 3), 3, "PERIOD_ON_SET");
                assert_eq!(u32_at(&req, 4), 1, "FLAG_CHANGED");
                ng3_request = Some(u32_at(&req, 1));
            }
        }
        // Fake broadcast: MCP_Course[0] = 123
        let (field, _) = ng3_srv.find_field("MCP_Course_0").unwrap();
        let mut data = vec![0u8; ng3_srv.data_size];
        data[field.offset..field.offset + 2].copy_from_slice(&123u16.to_le_bytes());
        let mut pkt = Vec::new();
        // header (size, version, SIMCONNECT_RECV_ID_CLIENT_DATA=16), then
        // request, object, define, flags, entry, out_of, define_count
        for v in [0u32, 4, 16, ng3_request.unwrap(), 0, ng3_srv.data_definition, 0, 0, 1, 1] {
            pkt.extend_from_slice(&v.to_le_bytes());
        }
        pkt.extend_from_slice(&data);
        let len = pkt.len() as u32;
        pkt[0..4].copy_from_slice(&len.to_le_bytes());
        s.write_all(&pkt).await.unwrap();

        // Then the event: map "#69633" + transmit with wheel-up flag
        let (t, map) = read_packet(&mut s).await;
        assert_eq!(t, 4);
        assert!(map[4..].starts_with(b"#69633\0"));
        let (t, tx) = read_packet(&mut s).await;
        assert_eq!(t, 5);
        assert_eq!(u32_at(&tx, 2), 0x4000);
    });

    let sc = aerodeck::simconnect::SimConnectClient::connect(&addr).await.unwrap();
    sc.ensure_pmdg().await.unwrap();
    let mut value = None;
    for _ in 0..50 {
        value = sc.pmdg_value("PMDGNG3", "MCP_Course_0");
        if value.is_some() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(value.as_deref(), Some("123"));
    assert_eq!(sc.pmdg_snapshot().get("PMDGNG3:MCP_COURSE_0").map(String::as_str), Some("123"));
    sc.transmit_pmdg_event("PMDGNG3", "EVT OH ELEC BATTERY SWITCH", pmdg::MOUSE_FLAG_WHEEL_UP).await.unwrap();
    server.await.unwrap();
}
