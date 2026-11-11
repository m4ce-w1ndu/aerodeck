//! Variables page: PMDG SDK status, LVAR bridge tools, SimVars and live value tables.

use std::rc::Rc;

use slint::ComponentHandle;

use super::*;
use crate::gui::editing::format_value;

impl App {
    pub(crate) fn refresh_vars(&self) {
        let Some(ui) = self.ui.upgrade() else { return };
        let vs = ui.global::<VarsState>();
        let st = self.st.borrow();
        vs.set_pmdg(model(
            st.pmdg
                .iter()
                .map(|r| {
                    let (status, tone) = if r.receiving {
                        ("receiving data", 2)
                    } else if r.broadcast_enabled {
                        ("waiting for aircraft", 3)
                    } else {
                        ("broadcast off", 4)
                    };
                    PmdgRow { label: ss(&r.label), package: ss(&r.package), status: ss(status), tone }
                })
                .collect(),
        ));
        vs.set_pmdg_any_off(st.pmdg.iter().any(|r| !r.broadcast_enabled));
        vs.set_bridge_active(st.bridge_active);
        vs.set_lvar_count(st.lvar_count as i32);
        let q = vs.get_filter().to_lowercase();
        let hit = |n: &str| q.is_empty() || n.to_lowercase().contains(&q);
        vs.set_simvars(model(
            st.simvars
                .iter()
                .filter(|(n, _)| hit(n))
                .map(|(n, v)| VarRow { name: ss(n), value: ss(v.map(format_value).unwrap_or_default()), waiting: v.is_none() })
                .collect(),
        ));
        vs.set_lvars(model(
            st.lvars
                .iter()
                .filter(|n| hit(n))
                .map(|n| {
                    let v = st.lvar_values.get(n).copied();
                    VarRow { name: ss(n), value: ss(v.map(format_value).unwrap_or_default()), waiting: v.is_none() }
                })
                .collect(),
        ));
    }
}

pub(crate) fn wire(app: &Rc<App>, ui: &AppWindow) {
    let vs = ui.global::<VarsState>();
    let a = app.clone();
    vs.on_enable_broadcast(move || {
        a.send(GuiCommand::EnablePmdgBroadcast);
        a.toast("Data broadcast enabled: restart the aircraft to apply", 2);
    });
    let a = app.clone();
    vs.on_rescan_lvars(move || {
        a.send(GuiCommand::RequestLvarScan);
        a.toast("Scanning LVARs…", 2);
    });
    let a = app.clone();
    vs.on_set_lvar(move || {
        let Some(ui) = a.ui.upgrade() else { return };
        let vs = ui.global::<VarsState>();
        match vs.get_lvar_value().trim().parse::<f64>() {
            Ok(value) => {
                let name = vs.get_lvar_name().trim().trim_start_matches("LVAR:").to_string();
                a.send(GuiCommand::SetLvar { name: name.clone(), value });
                a.toast(format!("{} = {}", name, value), 2);
            }
            Err(_) => a.toast("The value is not a number", 3),
        }
    });
    let a = app.clone();
    vs.on_run_rpn(move || {
        let Some(ui) = a.ui.upgrade() else { return };
        let code = ui.global::<VarsState>().get_rpn().to_string();
        a.send(GuiCommand::ExecCalculatorCode { code });
        a.toast("RPN code sent", 2);
    });
    let a = app.clone();
    vs.on_watch_simvar(move || {
        let Some(ui) = a.ui.upgrade() else { return };
        let name = ui.global::<VarsState>().get_simvar_name().trim().to_string();
        if !name.is_empty() {
            a.send(GuiCommand::WatchSimVar { name: name.clone() });
            a.toast(format!("Watching {}", name), 2);
        }
    });
    let a = app.clone();
    vs.on_set_simvar(move || {
        let Some(ui) = a.ui.upgrade() else { return };
        let vs = ui.global::<VarsState>();
        match vs.get_simvar_value().trim().parse::<f64>() {
            Ok(value) => {
                let name = vs.get_simvar_name().trim().to_string();
                a.send(GuiCommand::SetSimVar { name: name.clone(), value });
                a.toast(format!("{} = {}", name, value), 2);
            }
            Err(_) => a.toast("The value is not a number", 3),
        }
    });
    let a = app.clone();
    vs.on_filter_changed(move || a.refresh_vars());
    let a = app.clone();
    vs.on_pick_simvar(move |n| {
        if let Some(ui) = a.ui.upgrade() {
            ui.global::<VarsState>().set_simvar_name(n);
        }
    });
    let a = app.clone();
    vs.on_pick_lvar(move |n| {
        if let Some(ui) = a.ui.upgrade() {
            ui.global::<VarsState>().set_lvar_name(n);
        }
    });
}
