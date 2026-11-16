//! The sidebar's places. Drives are listed at once; their free space is asked of every drive in
//! parallel, and a drive that hasn't answered within a moment (a network drive whose server is
//! off) is shown without it rather than holding up the list.

use fileboat_core::places::{self, Drive, Place};
use serde::Serialize;
use std::sync::mpsc;
use std::time::{Duration, Instant};

const SPACE_WAIT: Duration = Duration::from_millis(800);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Places {
    pub home: Vec<Place>,
    pub drives: Vec<Drive>,
}

fn with_space(mut drives: Vec<Drive>) -> Vec<Drive> {
    let (tx, rx) = mpsc::channel();
    for (i, d) in drives.iter().enumerate() {
        let (tx, path) = (tx.clone(), d.path.clone());
        // A drive that never answers keeps its thread until the OS gives up; nothing waits for it.
        std::thread::spawn(move || {
            let _ = tx.send((i, places::space(&path)));
        });
    }
    drop(tx);
    let deadline = Instant::now() + SPACE_WAIT;
    while let Ok((i, space)) = rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
        if let Some((free, total)) = space {
            drives[i].free = Some(free);
            drives[i].total = Some(total);
        }
    }
    drives
}

/// Free and total bytes of the drive holding `path` (for the status bar), or None if the drive
/// doesn't answer within a moment.
#[tauri::command]
pub async fn drive_space(path: String) -> Option<(u64, u64)> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(places::space(&path));
    });
    tauri::async_runtime::spawn_blocking(move || rx.recv_timeout(SPACE_WAIT).ok().flatten()).await.ok().flatten()
}

#[tauri::command]
pub async fn places_list() -> Places {
    tauri::async_runtime::spawn_blocking(|| Places { home: places::home_places(), drives: with_space(places::drives()) })
        .await
        .unwrap_or(Places { home: Vec::new(), drives: Vec::new() })
}
