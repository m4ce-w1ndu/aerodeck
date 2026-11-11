//! Simulator page: SimConnect connection and Steam Proton / SimConnect.xml setup.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use slint::ComponentHandle;

use super::*;
use crate::proton::ProtonConfigurator;

pub(crate) struct SimUi {
    pub prefix: String,
    pub cfg_port: u16,
    pub scope: String,
    pub roaming: Option<PathBuf>,
    pub message: Option<(String, bool)>,
}

impl SimUi {
    pub fn new() -> Self {
        let prefix = ProtonConfigurator::find_msfs_proton_prefix().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
        // Reuse an unprivileged IPv4 port already defined in SimConnect.xml
        let mut cfg_port = 45454;
        let mut scope = "local".to_string();
        let roaming = (!prefix.is_empty()).then(|| ProtonConfigurator::find_msfs_roaming_path(Some(PathBuf::from(&prefix)))).flatten();
        if let Some(r) = &roaming {
            let min_unpriv = ProtonConfigurator::get_linux_unprivileged_port_start();
            if let Some((sc, p)) = ProtonConfigurator::read_configured_ipv4_ports(r).into_iter().find(|(_, p)| *p >= min_unpriv) {
                cfg_port = p;
                scope = sc;
            }
        }
        Self { prefix, cfg_port, scope, roaming, message: None }
    }

    fn custom_prefix(&self) -> Option<PathBuf> {
        let t = self.prefix.trim();
        (!t.is_empty()).then(|| PathBuf::from(t))
    }
}

impl App {
    pub(crate) fn refresh_sim(&self) {
        let Some(ui) = self.ui.upgrade() else { return };
        let ss_ = ui.global::<SimState>();
        {
            let mut st = self.st.borrow_mut();
            if st.s.prefix.trim().is_empty() {
                if let Some(p) = ProtonConfigurator::find_msfs_proton_prefix() {
                    st.s.prefix = p.to_string_lossy().to_string();
                }
            }
            st.s.roaming = ProtonConfigurator::find_msfs_roaming_path(st.s.custom_prefix());
        }
        let st = self.st.borrow();
        ss_.set_connected(st.connected);
        ss_.set_status(ss(&st.sim_message));
        ss_.set_host(ss(&st.host));
        ss_.set_port(ss(st.port.to_string()));
        ss_.set_prefix(ss(&st.s.prefix));
        ss_.set_prefix_found(st.s.roaming.is_some());
        ss_.set_roaming_path(ss(st.s.roaming.as_ref().map(|p| p.to_string_lossy().to_string()).unwrap_or_default()));
        let min_unpriv = ProtonConfigurator::get_linux_unprivileged_port_start();
        let (xml_state, ports) = match &st.s.roaming {
            None => (0, Vec::new()),
            Some(r) if !r.join("SimConnect.xml").exists() => (1, Vec::new()),
            Some(r) => {
                let ports = ProtonConfigurator::read_configured_ipv4_ports(r);
                (if ports.is_empty() { 2 } else { 3 }, ports)
            }
        };
        ss_.set_xml_state(xml_state);
        ss_.set_privileged_warning(ss(if !ports.is_empty() && min_unpriv > 0 && ports.iter().all(|(_, p)| *p < min_unpriv) {
            format!(
                "Only ports below {} are defined. On Linux, Proton cannot open them without root: pick 45454 or 2048 and configure.",
                min_unpriv
            )
        } else {
            String::new()
        }));
        ss_.set_ports(model(ports.iter().map(|(sc, p)| PortItem { port: *p as i32, scope: ss(sc), active: *p == st.port }).collect()));
        ss_.set_cfg_port(ss(st.s.cfg_port.to_string()));
        ss_.set_scope(if st.s.scope == "global" { 1 } else { 0 });
        ss_.set_port_preset(match st.s.cfg_port {
            45454 => 0,
            2048 => 1,
            500 => 2,
            _ => -1,
        });
        ss_.set_port_warning(ss(if min_unpriv > 0 && st.s.cfg_port < min_unpriv {
            format!(
                "Port {} is below Linux's unprivileged port start ({}): MSFS under Proton will fail to open it unless you lowered net.ipv4.ip_unprivileged_port_start.",
                st.s.cfg_port, min_unpriv
            )
        } else {
            String::new()
        }));
        ss_.set_can_sync(st.port != st.s.cfg_port);
        let (msg, ok) = st.s.message.clone().unwrap_or_default();
        ss_.set_message(ss(msg));
        ss_.set_message_ok(ok);
        match &st.s.roaming {
            Some(r) => {
                ss_.set_helper_staged(r.join("aerodeck-pipe-bridge.exe").exists());
                ss_.set_wine_path(ss(ProtonConfigurator::find_proton_wine_binary(r).map(|p| p.to_string_lossy().to_string()).unwrap_or_default()));
            }
            None => {
                ss_.set_helper_staged(false);
                ss_.set_wine_path(ss(""));
            }
        }
        ss_.set_steam_libs(strings(ProtonConfigurator::discover_steam_libraries().iter().map(|p| p.display().to_string())));
        ss_.set_xml_preview(ss(ProtonConfigurator::generate_simconnect_xml_with_scope(st.s.cfg_port, &st.s.scope)));
        ss_.set_cfg_preview(ss(ProtonConfigurator::generate_simconnect_cfg("127.0.0.1", st.s.cfg_port)));
    }

    fn read_connection_fields(&self) -> bool {
        let Some(ui) = self.ui.upgrade() else { return false };
        let g = ui.global::<SimState>();
        let Ok(port) = g.get_port().trim().parse::<u16>() else {
            self.toast("The port must be a number between 1 and 65535", 3);
            return false;
        };
        let mut st = self.st.borrow_mut();
        st.host = g.get_host().trim().to_string();
        st.port = port;
        true
    }

    fn set_sim_message(&self, msg: String, ok: bool) {
        self.st.borrow_mut().s.message = Some((msg, ok));
        self.refresh_sim();
    }
}

pub(crate) fn wire(app: &Rc<App>, ui: &AppWindow) {
    let g = ui.global::<SimState>();
    let a = app.clone();
    g.on_connect(move || {
        if !a.read_connection_fields() {
            return;
        }
        let (host, port) = {
            let st = a.st.borrow();
            (st.host.clone(), st.port)
        };
        a.send(GuiCommand::ConnectSimConnect { host: host.clone(), port });
        a.toast(format!("Connecting to {}:{}…", host, port), 2);
        a.refresh_shell();
    });
    let a = app.clone();
    g.on_disconnect(move || {
        a.send(GuiCommand::DisconnectSimConnect);
    });
    let a = app.clone();
    g.on_use_port(move |p| {
        {
            let mut st = a.st.borrow_mut();
            st.port = p as u16;
            st.s.cfg_port = p as u16;
        }
        a.refresh_sim();
        a.refresh_shell();
        a.toast(format!("AeroDeck connects on port {}", p), 2);
    });
    let a = app.clone();
    g.on_browse(move || {
        let current = a.st.borrow().s.prefix.trim().to_string();
        let mut dialog = rfd::FileDialog::new().set_title("Select the MSFS Proton prefix (compatdata/1250410/pfx)");
        let cur = PathBuf::from(&current);
        if !current.is_empty() && cur.exists() {
            dialog = dialog.set_directory(&cur);
        } else if let Some(parent) = cur.parent().filter(|p| p.exists() && !current.is_empty()) {
            dialog = dialog.set_directory(parent);
        } else if Path::new("/mnt").exists() {
            dialog = dialog.set_directory("/mnt");
        }
        if let Some(folder) = dialog.pick_folder() {
            {
                let mut st = a.st.borrow_mut();
                st.s.prefix = folder.to_string_lossy().to_string();
                st.s.message = None;
            }
            a.refresh_sim();
            a.refresh_shell();
        }
    });
    let a = app.clone();
    g.on_auto_detect(move || match ProtonConfigurator::find_msfs_proton_prefix() {
        Some(p) => {
            a.st.borrow_mut().s.prefix = p.to_string_lossy().to_string();
            a.set_sim_message(format!("Found the prefix through Steam: {}", p.display()), true);
            a.refresh_shell();
        }
        None => a.set_sim_message("No prefix found automatically: use Browse to pick it.".to_string(), false),
    });
    let a = app.clone();
    g.on_prefix_edited(move || {
        let Some(ui) = a.ui.upgrade() else { return };
        a.st.borrow_mut().s.prefix = ui.global::<SimState>().get_prefix().to_string();
        a.refresh_sim();
        a.refresh_shell();
    });
    let a = app.clone();
    g.on_port_preset_picked(move |i| {
        let port = match i {
            0 => 45454,
            1 => 2048,
            _ => 500,
        };
        a.st.borrow_mut().s.cfg_port = port;
        a.refresh_sim();
    });
    let a = app.clone();
    g.on_cfg_port_edited(move || {
        let Some(ui) = a.ui.upgrade() else { return };
        if let Ok(p) = ui.global::<SimState>().get_cfg_port().trim().parse::<u16>() {
            a.st.borrow_mut().s.cfg_port = p;
            // keep the text the user is typing; refresh only derived fields
            let g = ui.global::<SimState>();
            let min_unpriv = ProtonConfigurator::get_linux_unprivileged_port_start();
            g.set_port_preset(match p {
                45454 => 0,
                2048 => 1,
                500 => 2,
                _ => -1,
            });
            g.set_port_warning(ss(if min_unpriv > 0 && p < min_unpriv {
                format!("Port {} is below Linux's unprivileged port start ({}): MSFS under Proton will fail to open it.", p, min_unpriv)
            } else {
                String::new()
            }));
            g.set_can_sync(a.st.borrow().port != p);
            g.set_xml_preview(ss(ProtonConfigurator::generate_simconnect_xml_with_scope(p, &a.st.borrow().s.scope)));
            g.set_cfg_preview(ss(ProtonConfigurator::generate_simconnect_cfg("127.0.0.1", p)));
        }
    });
    let a = app.clone();
    g.on_configure(move || {
        let Some(ui) = a.ui.upgrade() else { return };
        let scope = if ui.global::<SimState>().get_scope() == 1 { "global" } else { "local" };
        let (prefix, port) = {
            let mut st = a.st.borrow_mut();
            st.s.scope = scope.to_string();
            (st.s.custom_prefix(), st.s.cfg_port)
        };
        match ProtonConfigurator::configure_simconnect_with_scope(prefix, port, scope) {
            Ok(path) => {
                a.st.borrow_mut().port = port;
                a.set_sim_message(format!("Updated {} (port {}, scope {}). AeroDeck now connects on port {}.", path.display(), port, scope, port), true);
                a.refresh_shell();
            }
            Err(e) => a.set_sim_message(format!("Error: {}", e), false),
        }
    });
    let a = app.clone();
    g.on_sync_port(move || {
        {
            let mut st = a.st.borrow_mut();
            st.port = st.s.cfg_port;
        }
        a.refresh_sim();
        a.refresh_shell();
    });
    let a = app.clone();
    g.on_self_test(move || {
        let prefix = a.st.borrow().s.custom_prefix();
        match ProtonConfigurator::run_pipe_bridge_self_test(prefix) {
            Ok(msg) => a.set_sim_message(msg, true),
            Err(e) => a.set_sim_message(format!("Error: {}", e), false),
        }
    });
}
