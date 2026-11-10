use aerodeck::bridge::SpadLvarBridge;
use aerodeck::hardware::enumerator::{joystick_button_names, pov_direction};
use aerodeck::profile::{ProfileExecutor, ProfileParser};
use aerodeck::simconnect::SimConnectClient;
use std::sync::Arc;
use std::time::Duration;

const TRIGGERS: [&str; 5] = ["PRESS", "RELEASE", "PRESSSHORT", "PRESSLONG", "HELD"];

fn profile_xml() -> String {
    let defs = |prefix: &str| {
        TRIGGERS
            .iter()
            .map(|t| {
                format!(
                    r#"<EventDefinition Trigger="{p}{t}"><EventActions><EventActionChangeValue Target="LOCAL:{p}{t}" Operation="Increment" Value="1"/></EventActions></EventDefinition>"#,
                    p = prefix,
                    t = t
                )
            })
            .collect::<String>()
    };
    format!(
        r#"<Profile Version="2"><Name>timing</Name><Devices><Device VendorID="0x044F" ProductID="0x0407" Name="TCA" ID="">
<DevicePage ID="p" IsDefaultPage="true" Name="Default"><Events>
<Event BoundTo="BUTTON_1"><EventDefinitions>{}</EventDefinitions></Event>
<Event BoundTo="HATSWITCH_0"><EventDefinitions>{}</EventDefinitions></Event>
</Events></DevicePage></Device></Devices></Profile>"#,
        defs(""),
        defs("POV_NORTH_")
    )
}

async fn executor() -> (Arc<ProfileExecutor>, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    // Accept and drain; LOCAL actions never touch SimConnect.
    let server = tokio::spawn(async move {
        let (mut s, _) = listener.accept().await.unwrap();
        let mut buf = [0u8; 4096];
        use tokio::io::AsyncReadExt;
        while let Ok(n) = s.read(&mut buf).await {
            if n == 0 {
                break;
            }
        }
    });
    let sc = Arc::new(SimConnectClient::connect(&addr).await.unwrap());
    let bridge = Arc::new(SpadLvarBridge::new(sc.clone()));
    let profile = ProfileParser::parse_str(&profile_xml()).unwrap();
    (Arc::new(ProfileExecutor::new(profile, sc, bridge)), server)
}

async fn count(exec: &ProfileExecutor, name: &str) -> u32 {
    exec.local_var(name).await.and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0) as u32
}

const DEV: &str = "evdev_044f_0407_event7";

#[tokio::test]
async fn short_press() {
    let (exec, _srv) = executor().await;
    exec.handle_button(DEV, "BUTTON_1", "", true).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    exec.handle_button(DEV, "BUTTON_1", "", false).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    assert_eq!(count(&exec, "PRESS").await, 1);
    assert_eq!(count(&exec, "RELEASE").await, 1);
    assert_eq!(count(&exec, "PRESSSHORT").await, 1);
    assert_eq!(count(&exec, "PRESSLONG").await, 0);
    assert_eq!(count(&exec, "HELD").await, 1, "HELD fires once on press");
}

#[tokio::test]
async fn long_press_and_held_repeat() {
    let (exec, _srv) = executor().await;
    exec.handle_button(DEV, "BUTTON_1", "", true).await;
    tokio::time::sleep(Duration::from_millis(850)).await;
    exec.handle_button(DEV, "BUTTON_1", "", false).await;
    tokio::time::sleep(Duration::from_millis(250)).await; // timer must stop after release

    assert_eq!(count(&exec, "PRESSLONG").await, 1);
    assert_eq!(count(&exec, "PRESSSHORT").await, 0, "no short press after a long one");
    assert_eq!(count(&exec, "RELEASE").await, 1);
    let held = count(&exec, "HELD").await;
    // 1 on press + repeats every 100 ms from 300 ms to 850 ms
    assert!((5..=8).contains(&held), "HELD fired {} times", held);
}

#[tokio::test]
async fn pov_directions_use_prefixed_triggers() {
    let (exec, _srv) = executor().await;
    exec.handle_button(DEV, "HATSWITCH_0", "POV_NORTH_", true).await;
    exec.handle_button(DEV, "HATSWITCH_0", "POV_NORTH_", false).await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(count(&exec, "POV_NORTH_PRESS").await, 1);
    assert_eq!(count(&exec, "POV_NORTH_PRESSSHORT").await, 1);
    assert_eq!(count(&exec, "PRESS").await, 0, "plain button triggers stay untouched");
}

#[test]
fn spad_button_and_pov_names() {
    let names = joystick_button_names([0x2c0u16, 0x120, 0x121, 0x30 /* KEY_B, ignored */].into_iter());
    assert_eq!(names[&0x120], "BUTTON_1");
    assert_eq!(names[&0x121], "BUTTON_2");
    assert_eq!(names[&0x2c0], "BUTTON_3", "BTN_TRIGGER_HAPPY1 follows the first block");
    assert!(!names.contains_key(&0x30));

    assert_eq!(pov_direction(0, -1), Some("NORTH"));
    assert_eq!(pov_direction(1, 1), Some("SOUTHEAST"));
    assert_eq!(pov_direction(0, 0), None);
}

#[tokio::test]
async fn hat_directions_split_across_device_entries_both_fire() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let _srv = tokio::spawn(async move {
        let (mut s, _) = listener.accept().await.unwrap();
        let mut buf = [0u8; 4096];
        use tokio::io::AsyncReadExt;
        while s.read(&mut buf).await.unwrap_or(0) > 0 {}
    });
    let def = |dir: &str| {
        format!(
            r#"<Event BoundTo="HATSWITCH_0"><EventDefinitions><EventDefinition Trigger="POV_{d}_PRESS" IsFinal="true"><EventActions><EventActionChangeValue Target="LOCAL:{d}" Operation="Increment" Value="1"/></EventActions></EventDefinition></EventDefinitions></Event>"#,
            d = dir
        )
    };
    // Same stick listed twice (as left behind by imports/merges), north in one, south in the other.
    let xml = format!(
        r#"<Profile Version="2"><Name>split</Name><Devices>
<Device VendorID="0x4098" ProductID="0xBC27" Name="URSA"><DevicePage ID="a" Name="Default"><Events>{}</Events></DevicePage></Device>
<Device VendorID="0x4098" ProductID="0xBC27" ID="DEV_4098_BC27"><DevicePage ID="b" Name="Default"><Events>{}</Events></DevicePage></Device>
</Devices></Profile>"#,
        def("NORTH"),
        def("SOUTH")
    );
    let sc = Arc::new(SimConnectClient::connect(&addr).await.unwrap());
    let exec = Arc::new(ProfileExecutor::new(
        ProfileParser::parse_str(&xml).unwrap(),
        sc.clone(),
        Arc::new(SpadLvarBridge::new(sc)),
    ));
    let dev = "evdev_4098_bc27_event3";
    for prefix in ["POV_NORTH_", "POV_SOUTH_"] {
        exec.handle_button(dev, "HATSWITCH_0", prefix, true).await;
        exec.handle_button(dev, "HATSWITCH_0", prefix, false).await;
    }
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(count(&exec, "NORTH").await, 1);
    assert_eq!(count(&exec, "SOUTH").await, 1);
}
