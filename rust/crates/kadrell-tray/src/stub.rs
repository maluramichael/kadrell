//! Andere Plattformen (Linux): kein Tray, alles ein No-op, damit der Workspace dort baut.

use std::sync::mpsc::Sender;

use crate::model::{Notification, Sound, StatusSummary, TrayItem};
use crate::{TrayConfig, TrayEvent};

pub struct Backend;

impl Backend {
    pub fn new(_config: TrayConfig, _events: Sender<TrayEvent>) -> Self {
        Backend
    }
    pub fn set_status(&self, _s: StatusSummary) {}
    pub fn set_menu(&self, _items: Vec<TrayItem>) {}
    pub fn notify(&self, _n: Notification) {}
    pub fn set_badge(&self, _count: usize) {}
    pub fn set_window_handle(&self, _hwnd: isize) {}
}

pub fn play(_sound: Sound) {}

pub fn beep() {}

pub fn reduce_motion() -> bool {
    false
}
