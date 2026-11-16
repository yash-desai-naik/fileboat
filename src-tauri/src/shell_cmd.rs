//! `!` in the command bar: a shell command run in the folder you're in, its output streamed back.
//! `cmd /C` on Windows (the command passed as typed), the login shell on macOS; no console
//! window; cancelled when the command bar closes. Output is capped at 1 MB.

use serde::Serialize;
use std::collections::HashMap;
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::ipc::Channel;
use tauri::State;

#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ShellEvent {
    Output { text: String },
    Exit { code: Option<i32>, ms: u64 },
}

#[derive(Default)]
pub struct Shells {
    next: AtomicU64,
    running: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

const MAX_OUTPUT: usize = 1 << 20;

fn command(line: &str) -> Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let mut c = Command::new(std::env::var_os("ComSpec").unwrap_or_else(|| "cmd.exe".into()));
        c.arg("/S").arg("/C").raw_arg(format!("\"{line}\""));
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        c
    }
    #[cfg(not(windows))]
    {
        let shell = std::env::var_os("SHELL").unwrap_or_else(|| "/bin/sh".into());
        let mut c = Command::new(shell);
        c.arg("-lc").arg(line);
        c
    }
}

#[tauri::command]
pub fn shell_run(shells: State<'_, Arc<Shells>>, cwd: String, line: String, on_event: Channel<ShellEvent>) -> Result<String, String> {
    if line.trim().is_empty() {
        return Err("Type a command.".into());
    }
    let mut child = command(&line)
        .current_dir(&cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Couldn't start the shell: {e}"))?;
    let id = format!("s{}", shells.next.fetch_add(1, Ordering::Relaxed));
    let stop = Arc::new(AtomicBool::new(false));
    shells.running.lock().unwrap().insert(id.clone(), stop.clone());
    let shells = shells.inner().clone();
    let key = id.clone();
    std::thread::spawn(move || {
        let started = Instant::now();
        let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
        let mut readers = Vec::new();
        for pipe in [child.stdout.take().map(|p| Box::new(p) as Box<dyn Read + Send>), child.stderr.take().map(|p| Box::new(p) as Box<dyn Read + Send>)].into_iter().flatten() {
            let tx = tx.clone();
            readers.push(std::thread::spawn(move || {
                let mut pipe = pipe;
                let mut buf = [0u8; 8192];
                while let Ok(n) = pipe.read(&mut buf) {
                    if n == 0 || tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
            }));
        }
        drop(tx);
        let mut sent = 0usize;
        let mut pending = Vec::new();
        let flush = |pending: &mut Vec<u8>, sent: &mut usize| {
            if pending.is_empty() || *sent >= MAX_OUTPUT {
                pending.clear();
                return;
            }
            let take = pending.len().min(MAX_OUTPUT - *sent);
            *sent += take;
            let mut text = String::from_utf8_lossy(&pending[..take]).into_owned();
            if *sent >= MAX_OUTPUT {
                text.push_str("\n… (output cut at 1 MB)\n");
            }
            let _ = on_event.send(ShellEvent::Output { text });
            pending.clear();
        };
        let code = loop {
            match rx.recv_timeout(Duration::from_millis(60)) {
                Ok(chunk) => pending.extend(chunk),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    flush(&mut pending, &mut sent);
                    break child.wait().ok().and_then(|s| s.code());
                }
            }
            flush(&mut pending, &mut sent);
            if stop.load(Ordering::Relaxed) {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        };
        for r in readers {
            let _ = r.join();
        }
        if !stop.load(Ordering::Relaxed) {
            let _ = on_event.send(ShellEvent::Exit { code, ms: started.elapsed().as_millis() as u64 });
        }
        shells.running.lock().unwrap().remove(&key);
    });
    Ok(id)
}

#[tauri::command]
pub fn shell_cancel(shells: State<'_, Arc<Shells>>, id: String) {
    if let Some(stop) = shells.running.lock().unwrap().remove(&id) {
        stop.store(true, Ordering::Relaxed);
    }
}
