//! Controller between the Slint UI and the backend: owns the interface state, turns backend
//! events into models and UI callbacks into commands.

slint::include_modules!();

mod logs;
mod mapping;
mod profiles;
mod sim;
mod vars;

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use anyhow::Result;
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use tokio::sync::mpsc;

use crate::gui::backend::{start_backend, GuiCommand, GuiEvent, PmdgUiStatus};
use crate::hardware::enumerator::{ConnectedDevice, DeviceEnumerator};
use crate::profile::schema::SpadProfile;
use crate::profile::{AircraftAssignmentManager, AircraftScanner, DiscoveredAircraft, ProfileParser, ProfileRepository};

pub(crate) const NO_AIRCRAFT: &str = "No Aircraft Detected";

/// Last known position of an axis.
#[derive(Debug, Clone)]
pub(crate) struct LiveAxis {
    pub raw: i32,
    pub normalized: f64,
    pub min: i32,
    pub max: i32,
}

pub(crate) struct LogEntry {
    pub time: String,
    pub level: String,
    pub text: String,
}

/// Work finished on a helper thread, handed back to the UI thread.
pub(crate) enum Task {
    SnippetDownloaded { profile_path: String, result: std::result::Result<PathBuf, String> },
}

pub(crate) struct State {
    pub host: String,
    pub port: u16,
    pub connected: bool,
    pub sim_message: String,
    pub bridge_active: bool,
    pub lvar_count: usize,
    pub aircraft: String,

    pub profile: Option<SpadProfile>,
    pub profile_path: String,
    pub profile_name: String,
    pub dirty: bool,
    pub auto_matched: bool,
    pub local_profiles: Vec<String>,

    pub devices: Vec<ConnectedDevice>,
    pub aircrafts: Vec<DiscoveredAircraft>,
    pub assignments: AircraftAssignmentManager,

    pub last_input: Option<(String, String, String, Instant)>,
    pub feed: VecDeque<(String, String, String, String, bool)>,
    pub device_activity: HashMap<String, Instant>,
    pub input_activity: HashMap<String, Instant>,
    pub live_axes: HashMap<String, LiveAxis>,
    /// Where each axis' current movement started and when it last moved, to tell a
    /// deliberate move from jitter
    pub axis_gesture: HashMap<String, (i32, Instant)>,

    pub lvars: Vec<String>,
    pub lvar_values: HashMap<String, f64>,
    pub simvars: Vec<(String, Option<f64>)>,
    pub pmdg: Vec<PmdgUiStatus>,

    pub logs: Vec<LogEntry>,

    pub m: mapping::MappingUi,
    pub p: profiles::ProfilesUi,
    pub s: sim::SimUi,
}

pub(crate) struct App {
    pub ui: slint::Weak<AppWindow>,
    pub cmd: mpsc::UnboundedSender<GuiCommand>,
    pub st: RefCell<State>,
    pub task_tx: std::sync::mpsc::Sender<Task>,
    task_rx: std::sync::mpsc::Receiver<Task>,
    events: RefCell<mpsc::UnboundedReceiver<GuiEvent>>,
    toast_serial: std::cell::Cell<i32>,
    /// Persistent models (re-creating them would reset list scroll positions)
    pub inputs_model: Rc<VecModel<InputItem>>,
    pub devices_model: Rc<VecModel<DeviceItem>>,
    pub mapping_devices_model: Rc<VecModel<DeviceItem>>,
    pub logs_model: Rc<VecModel<LogItem>>,
}

pub(crate) fn ss(s: impl AsRef<str>) -> SharedString {
    SharedString::from(s.as_ref())
}

pub(crate) fn strings(v: impl IntoIterator<Item = impl AsRef<str>>) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(v.into_iter().map(|s| ss(s)).collect::<Vec<_>>()))
}

pub(crate) fn model<T: Clone + 'static>(v: Vec<T>) -> ModelRc<T> {
    ModelRc::new(VecModel::from(v))
}

pub(crate) fn now_hms() -> String {
    use std::time::SystemTime;
    let secs = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    // Local offset is not known without a tz crate; the backend logs use UTC too.
    format!("{:02}:{:02}:{:02}", (secs / 3600) % 24, (secs / 60) % 60, secs % 60)
}

fn config_file() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("aerodeck").join("ui.conf"))
}

fn load_dark_theme() -> bool {
    config_file()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|s| !s.lines().any(|l| l.trim() == "theme=light"))
        .unwrap_or(true)
}

fn save_dark_theme(dark: bool) {
    if let Some(p) = config_file() {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(p, format!("theme={}\n", if dark { "dark" } else { "light" }));
    }
}

pub fn run(profile_path: Option<PathBuf>, host: String, port: u16) -> Result<()> {
    slint::set_xdg_app_id("aerodeck").ok();
    let (cmd_tx, event_rx) = start_backend(profile_path.clone(), host.clone(), port);

    let ui = AppWindow::new()?;
    ui.global::<Theme>().set_dark(load_dark_theme());
    ui.global::<AppState>().set_version(ss(env!("CARGO_PKG_VERSION")));

    let sim_ui = sim::SimUi::new();
    let initial_port = if port == 45454 && sim_ui.cfg_port != 45454 { sim_ui.cfg_port } else { port };

    let (task_tx, task_rx) = std::sync::mpsc::channel();
    let mut state = State {
        host,
        port: initial_port,
        connected: false,
        sim_message: "Waiting for Microsoft Flight Simulator".to_string(),
        bridge_active: false,
        lvar_count: 0,
        aircraft: NO_AIRCRAFT.to_string(),
        profile: None,
        profile_path: String::new(),
        profile_name: "No profile".to_string(),
        dirty: false,
        auto_matched: false,
        local_profiles: ProfileRepository::list_local_profiles(),
        devices: DeviceEnumerator::enumerate_all(),
        aircrafts: AircraftScanner::discover_installed_aircraft(None),
        assignments: AircraftAssignmentManager::load_or_default(),
        last_input: None,
        feed: VecDeque::new(),
        device_activity: HashMap::new(),
        input_activity: HashMap::new(),
        live_axes: HashMap::new(),
        axis_gesture: HashMap::new(),
        lvars: Vec::new(),
        lvar_values: HashMap::new(),
        simvars: Vec::new(),
        pmdg: Vec::new(),
        logs: Vec::new(),
        m: mapping::MappingUi::default(),
        p: profiles::ProfilesUi::new(),
        s: sim_ui,
    };

    // Initial profile, as the backend loads it
    let initial = profile_path.or_else(|| Path::new("default_c172.xml").exists().then(|| PathBuf::from("default_c172.xml")));
    if let Some(p) = initial.filter(|p| p.exists()) {
        if let Ok(prof) = ProfileParser::parse_file(&p) {
            state.profile_name = prof.name.clone().unwrap_or_else(|| file_stem(&p.to_string_lossy()));
            state.profile_path = p.to_string_lossy().to_string();
            state.profile = Some(prof);
        }
    }

    let app = Rc::new(App {
        ui: ui.as_weak(),
        cmd: cmd_tx,
        st: RefCell::new(state),
        task_tx,
        task_rx,
        events: RefCell::new(event_rx),
        toast_serial: std::cell::Cell::new(0),
        inputs_model: Rc::new(VecModel::default()),
        devices_model: Rc::new(VecModel::default()),
        mapping_devices_model: Rc::new(VecModel::default()),
        logs_model: Rc::new(VecModel::default()),
    });

    ui.global::<MappingState>().set_inputs(ModelRc::from(app.inputs_model.clone()));
    ui.global::<MappingState>().set_devices(ModelRc::from(app.mapping_devices_model.clone()));
    ui.global::<AppState>().set_devices(ModelRc::from(app.devices_model.clone()));
    ui.global::<LogsState>().set_logs(ModelRc::from(app.logs_model.clone()));

    app.wire_shell(&ui);
    mapping::wire(&app, &ui);
    profiles::wire(&app, &ui);
    vars::wire(&app, &ui);
    sim::wire(&app, &ui);
    logs::wire(&app, &ui);

    app.refresh_shell();
    app.refresh_devices();
    app.refresh_mapping_all();
    app.refresh_sim();

    // Pump backend events and finished tasks on the UI thread.
    let timer = slint::Timer::default();
    {
        let app = app.clone();
        timer.start(slint::TimerMode::Repeated, Duration::from_millis(33), move || app.tick());
    }

    ui.run()?;
    drop(timer);
    Ok(())
}

pub(crate) fn file_stem(path: &str) -> String {
    Path::new(path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| path.to_string())
}

pub(crate) fn same_profile_path(a: &str, b: &str) -> bool {
    !a.is_empty() && !b.is_empty() && (a == b || a.ends_with(b) || b.ends_with(a))
}

impl App {
    pub fn send(&self, cmd: GuiCommand) {
        let _ = self.cmd.send(cmd);
    }

    pub fn log(&self, level: &str, text: impl Into<String>) {
        let entry = LogEntry { time: now_hms(), level: level.to_string(), text: text.into() };
        self.push_log(entry);
    }

    /// Short message in the corner. Tone: 2 success, 3 warning, 4 error.
    pub fn toast(&self, text: impl Into<String>, tone: i32) {
        let Some(ui) = self.ui.upgrade() else { return };
        let serial = self.toast_serial.get() + 1;
        self.toast_serial.set(serial);
        let g = ui.global::<AppState>();
        g.set_toast_text(ss(text.into()));
        g.set_toast_tone(tone);
        let weak = self.ui.clone();
        let app_serial = serial;
        slint::Timer::single_shot(Duration::from_millis(3800), move || {
            // Only the latest toast clears itself.
            if let Some(ui) = weak.upgrade() {
                let g = ui.global::<AppState>();
                if g.get_toast_serial() == app_serial {
                    g.set_toast_text(SharedString::new());
                }
            }
        });
        g.set_toast_serial(serial);
    }

    /// Saves the in-memory active profile (the backend writes it and reloads the executor).
    pub fn save_active_profile(&self) {
        let st = self.st.borrow();
        if let Some(prof) = st.profile.clone() {
            if st.profile_path.is_empty() {
                return;
            }
            self.send(GuiCommand::UpdateAndSaveProfile { profile: prof, path: PathBuf::from(&st.profile_path), bind_to_aircraft: None });
        }
    }

    fn wire_shell(self: &Rc<Self>, ui: &AppWindow) {
        let g = ui.global::<AppState>();
        let app = self.clone();
        g.on_navigate(move |page| app.navigate(page));
        let app = self.clone();
        g.on_select_profile(move |i| {
            let path = app.st.borrow().local_profiles.get(i as usize).cloned();
            if let Some(p) = path {
                app.st.borrow_mut().auto_matched = false;
                app.send(GuiCommand::LoadProfile(PathBuf::from(p)));
            }
        });
        let app = self.clone();
        g.on_save_profile(move || {
            app.save_active_profile();
            app.st.borrow_mut().dirty = false;
            app.refresh_shell();
        });
        let app = self.clone();
        g.on_toggle_theme(move || {
            if let Some(ui) = app.ui.upgrade() {
                let t = ui.global::<Theme>();
                let dark = !t.get_dark();
                t.set_dark(dark);
                save_dark_theme(dark);
            }
        });
        let app = self.clone();
        g.on_rescan_devices(move || {
            app.send(GuiCommand::RescanDevices);
            app.st.borrow_mut().devices = DeviceEnumerator::enumerate_all();
            app.refresh_devices();
            app.refresh_mapping_all();
            app.toast("Hardware rescanned", 2);
        });
        let app = self.clone();
        g.on_open_device(move |i| {
            app.st.borrow_mut().m.device_idx = i.max(0) as usize;
            app.st.borrow_mut().m.selected_input.clear();
            app.navigate(1);
            app.refresh_mapping_all();
        });
        let app = self.clone();
        g.on_quick_connect(move || {
            let (host, port) = {
                let st = app.st.borrow();
                (st.host.clone(), st.port)
            };
            app.send(GuiCommand::ConnectSimConnect { host, port });
            app.toast(format!("Connecting to {}:{}…", app.st.borrow().host, port), 2);
        });
        g.on_open_url(|url| {
            let _ = std::process::Command::new("xdg-open").arg(url.as_str()).spawn();
        });
    }

    pub fn navigate(self: &Rc<Self>, page: i32) {
        let Some(ui) = self.ui.upgrade() else { return };
        ui.global::<AppState>().set_page(page);
        match page {
            1 => self.refresh_mapping_all(),
            2 => self.refresh_profiles(),
            3 => self.refresh_vars(),
            4 => self.refresh_sim(),
            5 => self.refresh_logs_full(),
            _ => {}
        }
    }

    /// Header, sidebar and overview tiles.
    pub fn refresh_shell(&self) {
        let Some(ui) = self.ui.upgrade() else { return };
        let st = self.st.borrow();
        let g = ui.global::<AppState>();
        g.set_connected(st.connected);
        g.set_sim_endpoint(ss(format!("{}:{}", st.host, st.port)));
        let detected = st.aircraft != NO_AIRCRAFT && !st.aircraft.is_empty();
        g.set_aircraft_detected(detected);
        g.set_aircraft(ss(if detected { st.aircraft.as_str() } else { "No aircraft detected" }));
        g.set_auto_matched(st.auto_matched);
        g.set_profile_name(ss(&st.profile_name));
        g.set_dirty(st.dirty);
        let (devs, inputs) = st
            .profile
            .as_ref()
            .and_then(|p| p.devices.as_ref())
            .map(|d| (d.items.len(), d.items.iter().map(|x| x.mapped_input_count()).sum::<usize>()))
            .unwrap_or((0, 0));
        g.set_profile_meta(ss(if st.profile.is_some() {
            format!("{} controllers · {} inputs mapped", devs, inputs)
        } else {
            "Pick or create a profile".to_string()
        }));
        g.set_profile_names(strings(st.local_profiles.iter().map(|p| file_stem(p))));
        g.set_profile_index(st.local_profiles.iter().position(|p| same_profile_path(p, &st.profile_path)).map(|i| i as i32).unwrap_or(-1));
        g.set_bridge_active(st.bridge_active);
        g.set_lvar_count(st.lvar_count as i32);
        g.set_proton_ready(st.s.roaming.is_some());
        let (pmdg_text, pmdg_tone) = if st.pmdg.is_empty() {
            ("Not installed".to_string(), 0)
        } else if st.pmdg.iter().any(|r| r.receiving) {
            ("Receiving data".to_string(), 2)
        } else if st.pmdg.iter().all(|r| r.broadcast_enabled) {
            ("Waiting for aircraft".to_string(), 3)
        } else {
            ("Data broadcast off".to_string(), 4)
        };
        g.set_pmdg_status(ss(pmdg_text));
        g.set_pmdg_tone(pmdg_tone);
    }

    /// Device lists (overview + mapping) with live activity.
    pub fn refresh_devices(&self) {
        let st = self.st.borrow();
        let items: Vec<DeviceItem> = st
            .devices
            .iter()
            .map(|d| {
                let mapped = st
                    .profile
                    .as_ref()
                    .and_then(|p| p.find_device_for_connected(d.vendor_id, d.product_id, &d.id))
                    .map(|x| x.mapped_input_count())
                    .unwrap_or(0);
                DeviceItem {
                    name: ss(&d.name),
                    vidpid: ss(format!("{:04X}:{:04X}", d.vendor_id, d.product_id)),
                    category: ss(d.category.badge()),
                    mapped: mapped as i32,
                    buttons: d.buttons.len() as i32,
                    axes: d.axes.len() as i32,
                    live: st.device_activity.get(&d.name).is_some_and(|t| t.elapsed() < Duration::from_millis(700)),
                }
            })
            .collect();
        self.devices_model.set_vec(items.clone());
        self.mapping_devices_model.set_vec(items);
    }

    /// Updates only the `live` flags of the device rows.
    fn refresh_device_activity(&self) {
        let st = self.st.borrow();
        for (i, d) in st.devices.iter().enumerate() {
            let live = st.device_activity.get(&d.name).is_some_and(|t| t.elapsed() < Duration::from_millis(700));
            for m in [&self.devices_model, &self.mapping_devices_model] {
                if let Some(mut row) = m.row_data(i) {
                    if row.live != live {
                        row.live = live;
                        m.set_row_data(i, row);
                    }
                }
            }
        }
    }

    fn refresh_feed(&self) {
        let Some(ui) = self.ui.upgrade() else { return };
        let st = self.st.borrow();
        let items: Vec<FeedItem> = st
            .feed
            .iter()
            .map(|(time, device, input, value, axis)| FeedItem { time: ss(time), device: ss(device), input: ss(input), value: ss(value), axis: *axis })
            .collect();
        let g = ui.global::<AppState>();
        g.set_feed(model(items));
    }

    fn refresh_live_strip(&self) {
        let Some(ui) = self.ui.upgrade() else { return };
        let st = self.st.borrow();
        let g = ui.global::<AppState>();
        match &st.last_input {
            Some((dev, bound, val, t)) => {
                g.set_last_input(ss(format!("{} · {} {}", short_device(dev), crate::gui::editing::input_label(bound), val)));
                g.set_input_live(t.elapsed() < Duration::from_millis(900));
            }
            None => {
                g.set_last_input(ss("Waiting for hardware input"));
                g.set_input_live(false);
            }
        }
    }

    fn tick(self: &Rc<Self>) {
        let mut shell = false;
        let mut devices = false;
        let mut feed = false;
        let mut axis = false;
        let mut mapping_lists = false;
        let mut vars = false;
        let mut profiles = false;
        // (device, input, is_axis): a button press wins over axis movement in the same tick
        let mut follow: Option<(String, String, bool)> = None;

        // Drain backend events (bounded per tick so the UI stays responsive)
        let mut events = Vec::new();
        {
            let mut rx = self.events.borrow_mut();
            while events.len() < 4000 {
                match rx.try_recv() {
                    Ok(e) => events.push(e),
                    Err(_) => break,
                }
            }
        }
        for ev in events {
            match ev {
                GuiEvent::SimConnectStatus { connected, message } => {
                    {
                        let mut st = self.st.borrow_mut();
                        st.connected = connected;
                        st.sim_message = message.clone();
                    }
                    self.log(if connected { "INFO" } else { "WARN" }, format!("SimConnect: {}", message));
                    shell = true;
                }
                GuiEvent::BridgeStatus { active, lvar_count } => {
                    let mut st = self.st.borrow_mut();
                    st.bridge_active = active;
                    st.lvar_count = lvar_count;
                    shell = true;
                    vars = true;
                }
                GuiEvent::AircraftChanged(name) => {
                    self.on_aircraft_changed(name);
                    shell = true;
                }
                GuiEvent::AircraftCatalogUpdated { aircrafts, assignments } => {
                    let mut st = self.st.borrow_mut();
                    st.aircrafts = aircrafts;
                    st.assignments = assignments;
                    profiles = true;
                }
                GuiEvent::RepositoryProfileInstalled { name, path, .. } => {
                    self.log("INFO", format!("Installed repository profile '{}' ({})", name, path));
                    profiles = true;
                }
                GuiEvent::OnlineProfilesQueried(_) => {}
                GuiEvent::OnlineSpadCatalogLoaded(entries) => {
                    let mut st = self.st.borrow_mut();
                    st.p.fetching = false;
                    st.p.catalog = entries;
                    profiles = true;
                }
                GuiEvent::WindowsSpadProfilesUpdated(_) => {}
                GuiEvent::DevicesUpdated(devs) => {
                    self.st.borrow_mut().devices = devs;
                    devices = true;
                    mapping_lists = true;
                }
                GuiEvent::InputTriggered { device, bound_to, action, time } => {
                    let is_axis = action.ends_with('%');
                    let mut st = self.st.borrow_mut();
                    st.device_activity.insert(device.clone(), time);
                    st.input_activity.insert(format!("{}|{}", device, bound_to), time);
                    let label = crate::gui::editing::input_label(&bound_to);
                    let entry = (now_hms(), device.clone(), label.clone(), action.clone(), is_axis);
                    let coalesce = st.feed.front().is_some_and(|f| f.4 && is_axis && f.1 == device && f.2 == label);
                    if coalesce {
                        st.feed[0] = entry;
                    } else {
                        st.feed.push_front(entry);
                        st.feed.truncate(9);
                    }
                    if !is_axis {
                        follow = Some((device.clone(), bound_to.clone(), false));
                    }
                    st.last_input = Some((device, bound_to, action, time));
                    feed = true;
                }
                GuiEvent::AxisMoved { device, bound_to, raw_value, normalized, min, max, time } => {
                    let mut st = self.st.borrow_mut();
                    let key = format!("{}:{}", device, bound_to);
                    let live = LiveAxis { raw: raw_value, normalized, min, max };
                    st.live_axes.insert(key.clone(), live.clone());
                    st.live_axes.insert(bound_to.clone(), live);
                    st.device_activity.insert(device.clone(), time);
                    // A movement starts when the axis moves after a pause. It lights the input
                    // past 3 % of travel, and is followed only past AXIS_FOLLOW_PERCENT: jitter
                    // and small nudges never move the selection.
                    let span = (max - min).abs().max(1) as i64;
                    let gesture = st.axis_gesture.entry(key).or_insert((raw_value, time));
                    if time.saturating_duration_since(gesture.1) > AXIS_GESTURE_PAUSE {
                        gesture.0 = raw_value;
                    }
                    gesture.1 = time;
                    let travel = (raw_value as i64 - gesture.0 as i64).abs() * 100 / span;
                    if travel >= 3 {
                        st.input_activity.insert(format!("{}|{}", device, bound_to), time);
                    }
                    if travel >= AXIS_FOLLOW_PERCENT && !follow.as_ref().is_some_and(|f| !f.2) {
                        follow = Some((device, bound_to, true));
                    }
                    axis = true;
                }
                GuiEvent::LvarsDiscovered(list) => {
                    let mut st = self.st.borrow_mut();
                    st.lvar_count = list.len();
                    st.lvars = list;
                    vars = true;
                    shell = true;
                }
                GuiEvent::SimVarsUpdated(list) => {
                    self.st.borrow_mut().simvars = list;
                    vars = true;
                }
                GuiEvent::PmdgStatus(rows) => {
                    self.st.borrow_mut().pmdg = rows;
                    vars = true;
                    shell = true;
                }
                GuiEvent::LvarUpdated { name, value } => {
                    self.st.borrow_mut().lvar_values.insert(name, value);
                    vars = true;
                }
                GuiEvent::Log { timestamp, level, text } => {
                    self.push_log(LogEntry { time: timestamp, level, text });
                }
                GuiEvent::ProfileLoaded { name, path } => {
                    {
                        let mut st = self.st.borrow_mut();
                        st.profile_name = name.clone();
                        st.profile_path = path.clone();
                        if let Ok(prof) = ProfileParser::parse_file(&path) {
                            st.profile = Some(prof);
                        }
                        st.dirty = false;
                        st.local_profiles = ProfileRepository::list_local_profiles();
                    }
                    self.log("INFO", format!("Loaded profile '{}' from {}", name, path));
                    shell = true;
                    devices = true;
                    mapping_lists = true;
                    profiles = true;
                }
                GuiEvent::ProfileSaved { path } => {
                    self.st.borrow_mut().dirty = false;
                    self.log("INFO", format!("Saved profile to {}", path));
                    shell = true;
                }
                GuiEvent::Error(err) => {
                    self.log("ERROR", err.clone());
                    self.toast(err, 4);
                }
            }
        }

        while let Ok(task) = self.task_rx.try_recv() {
            match task {
                Task::SnippetDownloaded { profile_path, result } => self.on_snippet_downloaded(profile_path, result),
            }
        }

        // Throttle quadrants close a switch at some detents: a button that fires while an
        // axis of the same device is moving is part of the lever movement, not a press.
        if let Some((device, _, false)) = &follow {
            let st = self.st.borrow();
            let prefix = format!("{}:", device);
            let lever_moving = st.axis_gesture.iter().any(|(k, (_, t))| k.starts_with(&prefix) && t.elapsed() < DETENT_WINDOW);
            if lever_moving {
                follow = None;
            }
        }
        if let Some((device, bound_to, is_axis)) = follow {
            if self.follow_input(&device, &bound_to, is_axis) {
                mapping_lists = false;
            }
        }

        let page = self.ui.upgrade().map(|u| u.global::<AppState>().get_page()).unwrap_or(0);
        if shell {
            self.refresh_shell();
        }
        if devices {
            self.refresh_devices();
        } else {
            self.refresh_device_activity();
        }
        if feed {
            self.refresh_feed();
        }
        self.refresh_live_strip();
        if page == 1 {
            if mapping_lists {
                self.refresh_mapping_all();
            } else {
                if axis {
                    self.refresh_mapping_live();
                }
                self.refresh_input_activity();
            }
        }
        if vars && page == 3 {
            self.refresh_vars();
        }
        if profiles && page == 2 {
            self.refresh_profiles();
        }
    }

    fn on_aircraft_changed(&self, name: String) {
        self.log("INFO", format!("Active aircraft: {}", name));
        let (auto, current) = {
            let mut st = self.st.borrow_mut();
            st.aircraft = name.clone();
            (st.assignments.auto_switch_enabled, st.profile_path.clone())
        };
        if !auto || name == NO_AIRCRAFT {
            return;
        }
        // Auto-switch to the profile matching the aircraft, or the fallback profile.
        let mut matched = None;
        let mut fallback = None;
        for p in ProfileRepository::list_local_profiles() {
            if let Ok(prof) = ProfileParser::parse_file(&p) {
                if prof.matches_aircraft_title(&name) {
                    matched = Some(p);
                    break;
                }
                if prof.is_fallback_profile() && fallback.is_none() {
                    fallback = Some(p);
                }
            }
        }
        if let Some(target) = matched.or(fallback) {
            if !same_profile_path(&target, &current) {
                self.send(GuiCommand::LoadProfile(PathBuf::from(&target)));
                self.st.borrow_mut().auto_matched = true;
                self.log("INFO", format!("Auto-switched profile to '{}' for aircraft '{}'", target, name));
                self.toast(format!("{} → {}", name, file_stem(&target)), 2);
            }
        }
    }
}

/// An axis is followed once a single movement covers this much of its travel (percent).
const AXIS_FOLLOW_PERCENT: i64 = 20;
/// An axis that stays still this long starts a new movement.
const AXIS_GESTURE_PAUSE: Duration = Duration::from_millis(600);
/// Buttons firing this close to an axis movement on the same device are detent switches.
const DETENT_WINDOW: Duration = Duration::from_millis(400);

/// Device names can be long ("Winwing WINCTRL URSA MINOR 32 Joystick L"); keep the start.
pub(crate) fn short_device(name: &str) -> String {
    let words: Vec<&str> = name.split_whitespace().collect();
    if words.len() <= 3 {
        name.to_string()
    } else {
        words[..3].join(" ")
    }
}
