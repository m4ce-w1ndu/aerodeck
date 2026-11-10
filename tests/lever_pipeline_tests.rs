//! End-to-end: SPAD-style TCA profile + evdev axis movement -> SimConnect packets.

use aerodeck::bridge::SpadLvarBridge;
use aerodeck::profile::{ProfileExecutor, ProfileParser};
use aerodeck::simconnect::SimConnectClient;
use std::sync::Arc;
use tokio::io::AsyncReadExt;

const TCA_PROFILE: &str = r#"<Profile Version="2"><Name>777 test</Name><Devices>
<Device VendorID="0x044F" ProductID="0x0407" DevicePath="\\?\hid#vid_044f&amp;pid_0407" Name="" ID="">
<DevicePage ID="p" IsDefaultPage="true" Name="Default"><Events>
<Event BoundTo="X_AXIS"><EventDefinitions><EventDefinition Trigger="VALUE"><EventActions>
<EventActionRangedAxis ConfigID="LEVER_NOREVERSE_THROTTLE"><Options>
<Option Key="USERAWVALUE" Value="True" /><Option Key="MAX" Value="0" /><Option Key="MAXTO" Value="0" />
<Option Key="OPTION1" Value="True" /><Option Key="IDLE" Value="50934" /><Option Key="IDLETO" Value="48647" />
</Options></EventActionRangedAxis></EventActions></EventDefinition></EventDefinitions></Event>
</Events></DevicePage></Device></Devices></Profile>"#;

/// Reads packets until the client goes quiet; returns (type, body) pairs.
async fn collect(listener: tokio::net::TcpListener) -> Vec<(u32, Vec<u8>)> {
    let (mut s, _) = listener.accept().await.unwrap();
    let mut out = Vec::new();
    loop {
        let mut len = [0u8; 4];
        match tokio::time::timeout(std::time::Duration::from_millis(500), s.read_exact(&mut len)).await {
            Ok(Ok(_)) => {}
            _ => return out,
        }
        let mut rest = vec![0u8; u32::from_le_bytes(len) as usize - 4];
        s.read_exact(&mut rest).await.unwrap();
        out.push((u32::from_le_bytes(rest[4..8].try_into().unwrap()) & 0x0FFF_FFFF, rest[12..].to_vec()));
    }
}

#[tokio::test]
async fn tca_lever_reaches_simconnect_and_unmapped_axis_is_reported() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let server = tokio::spawn(collect(listener));

    let sc = Arc::new(SimConnectClient::connect(&addr).await.unwrap());
    let br = Arc::new(SpadLvarBridge::new(sc.clone()));
    let exec = ProfileExecutor::new(ProfileParser::parse_str(TCA_PROFILE).unwrap(), sc, br);
    let dev = "evdev_044f_0407_event7";

    assert!(exec.dispatch_axis_event(dev, "ABS_X", 0, 0, 65535).await.unwrap());
    assert!(exec.dispatch_axis_event(dev, "ABS_X", 60000, 0, 65535).await.unwrap());
    assert!(!exec.dispatch_axis_event(dev, "ABS_Y", 100, 0, 65535).await.unwrap());

    let notices = exec.take_notices();
    assert!(notices.iter().any(|n| n.contains("ABS_X") && n.contains("THROTTLE1_AXIS_SET_EX1")), "{:?}", notices);
    assert!(notices.iter().any(|n| n.contains("ABS_Y") && n.contains("not mapped") && n.contains("777 test")), "{:?}", notices);
    exec.dispatch_axis_event(dev, "ABS_Y", 200, 0, 65535).await.unwrap();
    assert!(exec.take_notices().is_empty(), "notices are reported once");

    let packets = server.await.unwrap();
    let maps: Vec<String> = packets
        .iter()
        .filter(|(t, _)| *t == 4)
        .map(|(_, b)| String::from_utf8_lossy(&b[4..]).trim_end_matches('\0').to_string())
        .collect();
    assert_eq!(maps, ["THROTTLE1_AXIS_SET_EX1"]);
    let values: Vec<i32> = packets
        .iter()
        .filter(|(t, _)| *t == 5)
        .map(|(_, b)| i32::from_le_bytes(b[8..12].try_into().unwrap()))
        .collect();
    assert_eq!(values, [16383, -16383], "full forward, then past idle (reverse side clamps to idle)");
}
