use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crossterm::event::{self, Event, KeyEvent, KeyEventKind};

use crate::model::Snapshot;

/// Events consumed by the main loop. Input, tick, and the sampler's snapshots are unified
/// here so a single channel drives the render loop. Terminal `Resize` joins this enum once
/// the too-small guidance screen exists.
pub enum AppEvent {
    Key(KeyEvent),
    Tick,
    Snapshot(Box<Snapshot>),
}

const POLL_INTERVAL: Duration = Duration::from_millis(250);

/// Spawns the thread that turns crossterm input into `AppEvent`s, sending `Tick` on every
/// poll timeout. Exits within one `POLL_INTERVAL` of `running` being set to false.
pub fn spawn_input_thread(tx: Sender<AppEvent>, running: Arc<AtomicBool>) -> JoinHandle<()> {
    thread::spawn(move || {
        while running.load(Ordering::Relaxed) {
            let outgoing = match event::poll(POLL_INTERVAL) {
                Ok(true) => match event::read() {
                    Ok(Event::Key(key)) if key.kind == KeyEventKind::Press => {
                        Some(AppEvent::Key(key))
                    }
                    _ => None,
                },
                Ok(false) => Some(AppEvent::Tick),
                Err(_) => None,
            };

            if let Some(event) = outgoing
                && tx.send(event).is_err()
            {
                return;
            }
        }
    })
}
