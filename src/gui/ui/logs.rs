//! Logs page.

use std::rc::Rc;

use slint::{ComponentHandle, Model};

use super::*;

const MAX_LOGS: usize = 5000;

fn tone(level: &str) -> i32 {
    match level {
        "ERROR" => 4,
        "WARN" => 3,
        "ACTION" => 2,
        "INPUT" => 1,
        _ => 0,
    }
}

fn item(e: &LogEntry) -> LogItem {
    LogItem { time: ss(&e.time), level: ss(&e.level), text: ss(&e.text), tone: tone(&e.level) }
}

impl App {
    fn log_matches(&self, e: &LogEntry) -> bool {
        let Some(ui) = self.ui.upgrade() else { return true };
        let g = ui.global::<LogsState>();
        let level_ok = match g.get_level_filter() {
            1 => e.level == "ERROR",
            2 => e.level == "WARN",
            3 => e.level == "ACTION" || e.level == "INPUT",
            4 => e.level == "INFO",
            _ => true,
        };
        let q = g.get_query().to_lowercase();
        level_ok && (q.is_empty() || e.text.to_lowercase().contains(&q))
    }

    pub(crate) fn push_log(&self, entry: LogEntry) {
        let visible = self.log_matches(&entry);
        {
            let mut st = self.st.borrow_mut();
            st.logs.push(entry);
            if st.logs.len() > MAX_LOGS {
                st.logs.drain(0..MAX_LOGS / 10);
                drop(st);
                self.refresh_logs_full();
                return;
            }
        }
        if visible {
            let st = self.st.borrow();
            if let Some(e) = st.logs.last() {
                self.logs_model.push(item(e));
            }
        }
        self.refresh_log_counts();
    }

    fn refresh_log_counts(&self) {
        let Some(ui) = self.ui.upgrade() else { return };
        let g = ui.global::<LogsState>();
        let st = self.st.borrow();
        g.set_total(st.logs.len() as i32);
        g.set_errors(st.logs.iter().filter(|e| e.level == "ERROR").count() as i32);
        g.set_warnings(st.logs.iter().filter(|e| e.level == "WARN").count() as i32);
        g.set_scroll_serial(g.get_scroll_serial().wrapping_add(1));
    }

    pub(crate) fn refresh_logs_full(&self) {
        let items: Vec<LogItem> = {
            let st = self.st.borrow();
            st.logs.iter().filter(|e| self.log_matches(e)).map(item).collect()
        };
        self.logs_model.set_vec(items);
        let _ = self.logs_model.row_count();
        self.refresh_log_counts();
    }
}

pub(crate) fn wire(app: &Rc<App>, ui: &AppWindow) {
    let g = ui.global::<LogsState>();
    let a = app.clone();
    g.on_filter_changed(move || a.refresh_logs_full());
    let a = app.clone();
    g.on_clear(move || {
        a.st.borrow_mut().logs.clear();
        a.refresh_logs_full();
    });
}
