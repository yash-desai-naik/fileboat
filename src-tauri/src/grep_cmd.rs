//! Content search for the UI: one search at a time per caller, its matches streamed in batches
//! on a channel, cancelled when a newer one starts or the caller goes away.

use crate::settings::SettingsStore;
use fileboat_core::grep::{self, GrepOptions, LineMatch};
use fileboat_core::index::Rules;
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::ipc::Channel;
use tauri::State;

#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum GrepEvent {
    Matches { items: Vec<LineMatch> },
    Done { files: usize, lines: usize, limited: bool, ms: u64 },
    Error { message: String },
}

#[derive(Default)]
pub struct Greps {
    next: AtomicU64,
    running: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

/// Lines a search stops at.
const LIMIT: usize = 2000;

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn grep_start(
    greps: State<'_, Arc<Greps>>,
    store: State<'_, SettingsStore>,
    path: String,
    pattern: String,
    regex: bool,
    case: bool,
    whole_word: bool,
    on_event: Channel<GrepEvent>,
) -> String {
    let id = format!("g{}", greps.next.fetch_add(1, Ordering::Relaxed));
    let stop = Arc::new(AtomicBool::new(false));
    greps.running.lock().unwrap().insert(id.clone(), stop.clone());
    let s = store.current();
    let rules = Rules::new(&s.index_rules_off, &s.index_excludes, &s.index_includes, true, true);
    let greps = greps.inner().clone();
    let key = id.clone();
    std::thread::spawn(move || {
        let started = Instant::now();
        let pending = Mutex::new((Vec::<LineMatch>::new(), Instant::now()));
        let lines = AtomicU64::new(0);
        let send = |items: Vec<LineMatch>| {
            if !items.is_empty() {
                let _ = on_event.send(GrepEvent::Matches { items });
            }
        };
        let opts = GrepOptions { regex, case, whole_word };
        let result = grep::grep(&PathBuf::from(&path), &pattern, &opts, &rules, LIMIT, &|| stop.load(Ordering::Relaxed), &|found| {
            lines.fetch_add(found.len() as u64, Ordering::Relaxed);
            let mut p = pending.lock().unwrap();
            p.0.extend(found);
            // Batches at most every 80 ms (the first at once).
            if p.1.elapsed() >= Duration::from_millis(80) || p.0.len() >= 500 {
                let items = std::mem::take(&mut p.0);
                p.1 = Instant::now();
                drop(p);
                send(items);
            }
        });
        send(std::mem::take(&mut pending.lock().unwrap().0));
        if !stop.load(Ordering::Relaxed) {
            let _ = on_event.send(match result {
                Ok(files) => {
                    let n = lines.load(Ordering::Relaxed) as usize;
                    GrepEvent::Done { files, lines: n, limited: n >= LIMIT, ms: started.elapsed().as_millis() as u64 }
                }
                Err(message) => GrepEvent::Error { message },
            });
        }
        greps.running.lock().unwrap().remove(&key);
    });
    id
}

#[tauri::command]
pub fn grep_cancel(greps: State<'_, Arc<Greps>>, id: String) {
    if let Some(stop) = greps.running.lock().unwrap().remove(&id) {
        stop.store(true, Ordering::Relaxed);
    }
}
