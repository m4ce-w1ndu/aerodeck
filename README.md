# AeroDeck: Advanced Flight Simulation Hardware & Telemetry Suite

A high-performance, cross-platform flight simulation hardware engine and controller bridge for **Linux across Steam Proton** and Windows, written in **Rust** with full **SPAD.neXt profile compatibility**.

---

## 1. Executive Summary & Architecture

Modern flight simulation enthusiasts utilize specialized flight controls, joysticks, throttle quadrants, rudder pedals, and hardware panels (such as Logitech/Saitek Radio, Multi, Switch, and BIP panels). However, running flight controller mapping suites natively on Linux under Steam Proton has historically been a significant barrier due to Windows-specific driver dependencies, WCF/WPF UI bindings, and named pipe restrictions.

**AeroDeck** solves this by providing:
1. **Zero-Overhead Linux-to-Proton SimConnect TCP Engine**: Connects natively to Microsoft Flight Simulator 2020/2024 running inside Steam Proton with sub-millisecond latency.
2. **SPAD.neXt Profile Compatibility**: Full deserialization, editing, and execution of SPAD.neXt `.xml` profiles (`xmlns="http://www.fsgs.com/SPAD"`), allowing pilots to reuse and import existing aircraft profiles without modification.
3. **Full Analog Axes Engine**: Real-time calibration, deadband jitter filtration, invert options, and graphical position bars for flight sticks, yokes, throttles, rudder pedals, and toe brakes.
4. **100% Protocol-Compatible MSFS WASM LVAR Bridge Client**: Speaks the binary ClientDataArea protocol implemented by `SPAD-bridge-module.wasm`. Supports LVAR discovery, RPN calculator code execution, HVARs, BVARs, and continuous variable streaming.
5. **Native Linux Hardware Discovery**: Automatic enumeration of Linux `evdev` devices (`/dev/input/event*`) and direct `hidraw` drivers for Saitek Multi, Radio, Switch, and BIP panels.

---

## 2. Protocol Specification & WASM Evidence

The bridge protocol enables communication with the flight simulator:

### 2.1 MSFS WASM LVAR Bridge Protocol

The WASM bridge is an in-process MSFS gauge module that communicates with external SimConnect clients through **SimConnect Client Data Areas**.

#### Client Data Areas & Identifiers
The WASM module maps 10 Client Data Areas using the signature prefix `0x534458` (`"SDX"` = SPAD Dynamics):

| Client Data Name | Data Area ID | Size | Direction | Purpose |
| :--- | :--- | :--- | :--- | :--- |
| `SPAD_SC_DYN_VARDATA_ID` | `0x53445801` (1396987905) | 268 B | WASM -> Host | Real-time LVAR updates (`DynamicData`) |
| `SPAD_SC_DYN_VARDATA_DATA` | `0x53445802` (1396987906) | - | - | Definition ID for VarData |
| `SPAD_SC_DYN_VARDUMP_ID` | `0x53445803` (1396987907) | 4012 B | WASM -> Host | Batch discovery variable name dumps (`VarDump`) |
| `SPAD_SC_DYN_VARDUMP_DATA` | `0x53445804` (1396987908) | - | - | Definition ID for VarDump |
| `SPAD_SC_DYN_HOSTRESQUEST_ID` | `0x53445805` (1396987909) | 276 B | Host -> WASM | Control requests & standard commands (`HostRequest`) |
| `SPAD_SC_DYN_HOSTRESQUEST_DATA` | `0x53445806` (1396987910) | - | - | Definition ID for HostRequest |
| `SPAD_SC_DYN_HOSTEVENT_ID` | `0x53445807` (1396987911) | 1044 B | WASM -> Host | Status, ACKs, and Calc return values (`HostEvent`) |
| `SPAD_SC_DYN_HOSTEVENT_DATA` | `0x53445808` (1396987912) | - | - | Definition ID for HostEvent |
| `SPAD_SC_DYN_HOSTRESQUEST2_ID` | `0x53445809` (1396987913) | 1044 B | Host -> WASM | Long script / RPN Calculator requests (`HostRequest2`) |
| `SPAD_SC_DYN_HOSTRESQUEST2_DATA` | `0x5344580A` (1396987914) | - | - | Definition ID for HostRequest2 |

#### Binary Packet Layouts
- **Host Secret**: `0x01020304` (`16909060`)
- **HostRequest (276 bytes)**:
  - `[0..4]`: `Secret` (int32, little-endian)
  - `[4..8]`: `Operation` (int32)
  - `[8..12]`: `dwQ400Index` / Sub-operation (int32)
  - `[12..268]`: `VariableName` (256 bytes null-terminated ASCII)
  - `[268..276]`: `VariableValue` (f64, 8 bytes)
- **HostRequest2 (1044 bytes)**:
  - `[0..4]`: `Secret` (int32)
  - `[4..8]`: `Operation` (int32)
  - `[8..12]`: `dwQ400Index` / Return Type: `0`=Void, `1`=Double, `2`=Int32, `3`=String
  - `[12..1036]`: `VariableName` / Calculator Script (1024 bytes ASCII)
  - `[1036..1044]`: `VariableValue` (f64)
- **HostEvent (1044 bytes)**:
  - `[0..4]`: `Secret` (int32)
  - `[4..8]`: `Operation` (int32: `7`=ACK, `8`=AircraftName, `17`=Calc Success, `13`=Calc Error)
  - `[8..12]`: `HasMessage` (int32)
  - `[12..20]`: `Value` (f64)
  - `[20..1044]`: `Message` (1024 bytes ASCII)
- **VarDump (4012 bytes)**:
  - `[0..4]`: `Secret` (int32)
  - `[4..8]`: `StartIndex` (int32)
  - `[8..12]`: `VariableCount` (int32)
  - `[12..4012]`: `VariableNames` (4000 bytes, semicolon `;` separated list)

#### Operation Codes
- `0`: `OP_START_LVAR_SEARCH` - Triggers gauge loop iterating `get_name_of_named_variable(0..4096)`
- `1`: `OP_SEND_LVARS` - Flushes discovered variables in 4000-byte `VarDump` packets
- `7`: `OP_CONNECT` - Handshake request. Responds with `HostEvent(op=7, value=5.0, message="5.0.0.0")`
- `8`: `OP_AIRCRAFT_NAME` - Dispatched when aircraft changes (contains aircraft title)
- `9`: `OP_REQ_AIRCRAFT_NAME` - Explicit aircraft name request
- `14`: `OP_HEARTBEAT` - Connection keepalive
- `16`: `OP_EXECUTE_CALCULATOR_CODE` - Executes MSFS RPN script via `execute_calculator_code()`
- `18`: `OP_SETTING_UPDATE` - Updates dig depth, search batch size, and search interval
- `10009`: `OP_RESET_LVARS` - Reindexes and resets LVAR table

---

## 3. Steam Proton & Linux Interop

When MSFS runs under Steam Proton, Wine's internal Named Pipe system does not bridge cleanly to native Linux processes. However, **SimConnect natively supports TCP sockets**.

By adding a `SimConnect.xml` to the Proton wineprefix (`<pfx>/drive_c/users/steamuser/AppData/Roaming/Microsoft Flight Simulator/`), MSFS opens a local IPv4 TCP listener on `127.0.0.1:45454`.

Because both Steam Proton and native Linux share the Linux network namespace (`localhost`), **AeroDeck** connects directly to this TCP socket, achieving **zero-overhead, sub-millisecond communication**.

---

## 4. Hardware Panel USB HID Protocol

| Device | VID | PID | Input Report | Output Report |
| :--- | :--- | :--- | :--- | :--- |
| **Radio Panel** | `0x06A3` | `0x0D05` | 3 bytes: top/bottom selector bitmasks & concentric encoders | 20 bytes: 4x 5-digit BCD with bit 7 for decimal points |
| **Multi Panel** | `0x06A3` | `0x0D06` | 3 bytes: AP buttons, mode rotary, trim wheel, flaps, throttle | 12 bytes: 5 upper digits, 5 lower digits, LED bitmask |
| **Switch Panel** | `0x06A3` | `0x0D67` | 3 bytes: 5-pos magneto, lights, switches, gear lever | 1 byte: 6 LEDs (Left, Nose, Right; Green & Red) |
| **BIP Panel** | `0x06A3` | `0x0B4E` | None | 10 bytes: 24 annunciators (2 bits each: Off/Grn/Amb/Red) |

---

## 5. Usage & Interactive GUI

### 5.1 Interactive Graphical Interface (GUI)
**AeroDeck** has a native desktop interface built with [Slint](https://slint.dev): declarative UI files in `ui/`, GPU rendering through OpenGL (with a software fallback), no web view. It ships its own fonts (Inter and JetBrains Mono, SIL OFL 1.1), a dark glass-cockpit theme and a light theme. It follows the desktop scale factor; `SLINT_SCALE_FACTOR=1.2 aerodeck` overrides it.

To launch the GUI:
```bash
# Launch GUI directly (default action)
./target/release/aerodeck
# or explicitly with a profile
./target/release/aerodeck gui --profile ./default_c172.xml
```

#### GUI pages (Ctrl+1 … Ctrl+6)
- **Overview**: simulator, profile, LVAR bridge and PMDG status at a glance, connected controllers and a live feed of every input you touch.
- **Controls**: pick a controller and an input (or just move it: AeroDeck follows real moves and ignores axis jitter), then
  - bind an axis to a sim axis event, with invert;
  - calibrate a lever (full / idle / reverse positions, engines, reverse gate) for throttle quadrants such as the Thrustmaster TCA;
  - add trigger zones that fire events when a lever enters or leaves a range (spoilers ARM);
  - add, test and delete actions: SimConnect events (searchable catalog), PMDG events (from the installed SDK), LVARs and RPN code, with optional conditions and button triggers (press, release, short / long press, held, hat directions).
- **Profiles**: create, import, rename, duplicate, export and delete profiles; aircraft auto-switching patterns with an installed-aircraft picker; generic fallback profile; controller layouts, with SPAD.neXt community snippets and undo.
- **Variables**: PMDG SDK status and data broadcast, WASM LVAR bridge tools, SimVar watch / write, live SimVar and LVAR tables.
- **Simulator**: SimConnect connection, Steam Proton prefix detection, SimConnect.xml port and scope, pipe-bridge self-test, config previews.
- **Logs**: filterable, searchable live log.

### 5.2 Build
```bash
cargo build --release
```

### 5.3 Auto-Configure Steam Proton (CLI)
```bash
./target/release/aerodeck proton setup
```

### 5.4 Profile Inspection & Generation
```bash
# Inspect a SPAD.neXt compatible profile
./target/release/aerodeck profile inspect --path my_profile.xml

# Generate a sample XML profile
./target/release/aerodeck profile create-sample --output c172.xml
```

### 5.5 Direct LVAR Bridge Interaction
```bash
# Discover LVARs
./target/release/aerodeck bridge list-lvars --simconnect 127.0.0.1:45454

# Execute RPN calculator code
./target/release/aerodeck bridge exec "(L:A32NX_AUTOPILOT_1_ACTIVE, bool)"
```

### 5.6 Scan Hardware Controllers & Flight Panels
```bash
./target/release/aerodeck devices scan
```

### 5.7 Test in Virtual Simulator Mode
```bash
./target/release/aerodeck test-virtual --profile ./default_c172.xml
```

### 5.8 Run Profile Daemon
```bash
./target/release/aerodeck run --profile ./default_c172.xml --simconnect 127.0.0.1:45454
```

---

## 6. Test Suite
```bash
cargo test
```
All tests pass with 100% verification.

---

## 7. License & Attribution

- **Author**: Simone Rolando
- **Repository**: [https://github.com/m4ce-w1ndu/aerodeck](https://github.com/m4ce-w1ndu/aerodeck)
- **App Icon**: Original geometric flight yoke & ascending aircraft motif (`assets/icon.svg`, multi-resolution PNGs and `.ico`).
- **License**: Dual-licensed under either of:
  - Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
  - MIT license ([LICENSE-MIT](LICENSE-MIT))

### Third-Party Disclaimer
AeroDeck is an independent open-source project and is not affiliated with, authorized, maintained, sponsored, or endorsed by Microsoft Corporation, Asobo Studio, Lockheed Martin, Laminar Research, or SPAD.neXt. All product names, logos, and brands are property of their respective owners.

