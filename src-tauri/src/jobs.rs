//! Copies and moves run as jobs on their own threads. Every window hears about them (`job`
//! events: progress at most ten times a second, a name clash waiting for an answer, the outcome),
//! and a finished job goes into the undo log.

use fileboat_core::ops::transfer::{self, Choice, Conflict, Control, Mode, Progress};
use fileboat_core::ops::undo::{Op, UndoLog};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressView {
    pub done_bytes: u64,
    pub total_bytes: u64,
    pub done_items: u64,
    pub total_items: u64,
    pub current: String,
    pub counting: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictView {
    pub name: String,
    pub folder: String,
    pub is_folder: bool,
    pub source_size: u64,
    pub target_size: u64,
    pub source_modified: f64,
    pub target_modified: f64,
}

/// What the windows see of a job.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobView {
    pub id: String,
    /// "copy" or "move".
    pub kind: &'static str,
    /// What is being copied ("“a.txt”", "3 items") and where to ("src").
    pub what: String,
    pub dest: String,
    /// "running", "conflict", "done", "failed" or "cancelled".
    pub state: &'static str,
    pub progress: ProgressView,
    pub conflict: Option<ConflictView>,
    /// The outcome in words, when finished ("Copied 3 items to “src”").
    pub message: Option<String>,
}

/// The user's answer to a clash: the choice and "do this for the rest", or None to cancel.
type Answer = Option<(Choice, bool)>;

struct Job {
    cancel: AtomicBool,
    /// While a clash waits for the user: where the answer goes.
    answer: Mutex<Option<mpsc::Sender<Answer>>>,
    /// "Do this for all": the answer every later clash gets.
    for_all: Mutex<Option<Choice>>,
}

#[derive(Default)]
pub struct Jobs {
    next: AtomicU64,
    running: Mutex<HashMap<String, Arc<Job>>>,
}

fn name_of(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.display().to_string())
}

fn progress_view(p: &Progress) -> ProgressView {
    ProgressView {
        done_bytes: p.done_bytes,
        total_bytes: p.total_bytes,
        done_items: p.done_items,
        total_items: p.total_items,
        current: p.current.clone(),
        counting: p.counting,
    }
}

/// Runs a transfer, telling the windows how it goes.
struct Ctl {
    app: AppHandle,
    job: Arc<Job>,
    view: Mutex<JobView>,
    last: Mutex<Option<Instant>>,
}

impl Ctl {
    fn emit(&self) {
        let _ = self.app.emit("job", self.view.lock().unwrap().clone());
    }
}

impl Control for Ctl {
    fn progress(&self, p: &Progress) {
        let mut last = self.last.lock().unwrap();
        let due = last.is_none_or(|t| t.elapsed() >= Duration::from_millis(100)) || (!p.counting && p.done_items == p.total_items);
        self.view.lock().unwrap().progress = progress_view(p);
        if due {
            *last = Some(Instant::now());
            drop(last);
            self.emit();
        }
    }

    fn conflict(&self, c: &Conflict) -> Option<Choice> {
        if let Some(choice) = *self.job.for_all.lock().unwrap() {
            return Some(choice);
        }
        let (tx, rx) = mpsc::channel();
        *self.job.answer.lock().unwrap() = Some(tx);
        {
            let mut v = self.view.lock().unwrap();
            v.state = "conflict";
            v.conflict = Some(ConflictView {
                name: name_of(&c.target),
                folder: c.target.parent().map(name_of).unwrap_or_default(),
                is_folder: c.target.is_dir(),
                source_size: c.source_size,
                target_size: c.target_size,
                source_modified: c.source_modified,
                target_modified: c.target_modified,
            });
        }
        self.emit();
        let answer = rx.recv().ok().flatten();
        *self.job.answer.lock().unwrap() = None;
        {
            let mut v = self.view.lock().unwrap();
            v.state = "running";
            v.conflict = None;
        }
        self.emit();
        let (choice, all) = answer?;
        if all {
            *self.job.for_all.lock().unwrap() = Some(choice);
        }
        Some(choice)
    }

    fn cancelled(&self) -> bool {
        self.job.cancel.load(Ordering::Relaxed)
    }
}

/// Copy or move `sources` into `dest` in the background. Returns the job's id at once.
#[tauri::command]
pub fn job_transfer(app: AppHandle, jobs: State<'_, Jobs>, sources: Vec<String>, dest: String, mode: String) -> Result<String, String> {
    let mode = match mode.as_str() {
        "copy" => Mode::Copy,
        "move" => Mode::Move,
        _ => return Err("Unknown transfer.".into()),
    };
    let sources: Vec<PathBuf> = sources.into_iter().map(PathBuf::from).collect();
    if sources.is_empty() {
        return Err("Nothing to copy.".into());
    }
    let dest = PathBuf::from(dest);
    let id = format!("j{}", jobs.next.fetch_add(1, Ordering::Relaxed));
    let job = Arc::new(Job { cancel: AtomicBool::new(false), answer: Mutex::new(None), for_all: Mutex::new(None) });
    jobs.running.lock().unwrap().insert(id.clone(), job.clone());
    let what = match sources.as_slice() {
        [one] => format!("“{}”", name_of(one)),
        many => format!("{} items", many.len()),
    };
    let kind = if mode == Mode::Copy { "copy" } else { "move" };
    let view = JobView {
        id: id.clone(),
        kind,
        what: what.clone(),
        dest: name_of(&dest),
        state: "running",
        progress: ProgressView { done_bytes: 0, total_bytes: 0, done_items: 0, total_items: 0, current: String::new(), counting: true },
        conflict: None,
        message: None,
    };
    let ctl = Ctl { app: app.clone(), job, view: Mutex::new(view), last: Mutex::new(None) };
    ctl.emit();

    let job_id = id.clone();
    std::thread::Builder::new()
        .name(format!("fileboat-{job_id}"))
        .spawn(move || {
            let result = transfer::transfer(&sources, &dest, mode, &ctl);
            let undo = app.state::<UndoLog>();
            let verb = if mode == Mode::Copy { "Copied" } else { "Moved" };
            let (state, message) = match result {
                Ok(out) => {
                    let skipped = if out.skipped > 0 { format!(" ({} skipped)", out.skipped) } else { String::new() };
                    let to = format!("“{}”", name_of(&dest));
                    if mode == Mode::Copy && !out.created.is_empty() {
                        undo.push(Op::Copied(out.created), format!("Copy {what} to {to}"));
                    }
                    if mode == Mode::Move && !out.moved.is_empty() {
                        undo.push(Op::Moved(out.moved), format!("Move {what} to {to}"));
                    }
                    ("done", format!("{verb} {what} to {to}{skipped}"))
                }
                Err(e) if e == "Cancelled." => ("cancelled", format!("{} cancelled.", if mode == Mode::Copy { "Copy" } else { "Move" })),
                Err(e) => ("failed", e),
            };
            {
                let mut v = ctl.view.lock().unwrap();
                v.state = state;
                v.message = Some(message);
            }
            ctl.emit();
            app.state::<Jobs>().running.lock().unwrap().remove(&job_id);
        })
        .map_err(|e| e.to_string())?;
    Ok(id)
}

#[tauri::command]
pub fn job_cancel(jobs: State<'_, Jobs>, id: String) {
    if let Some(job) = jobs.running.lock().unwrap().get(&id) {
        job.cancel.store(true, Ordering::Relaxed);
        // A job waiting on a clash is cancelled by answering "cancel".
        if let Some(tx) = job.answer.lock().unwrap().take() {
            let _ = tx.send(None);
        }
    }
}

/// Answer a name clash: "replace", "skip" or "keepBoth", for this one or for all the rest.
#[tauri::command]
pub fn job_resolve(jobs: State<'_, Jobs>, id: String, choice: String, all: bool) -> Result<(), String> {
    let choice = match choice.as_str() {
        "replace" => Choice::Replace,
        "skip" => Choice::Skip,
        "keepBoth" => Choice::KeepBoth,
        _ => return Err("Unknown choice.".into()),
    };
    let running = jobs.running.lock().unwrap();
    let job = running.get(&id).ok_or("That job has finished.")?;
    let tx = job.answer.lock().unwrap().take().ok_or("That job isn't waiting for an answer.")?;
    tx.send(Some((choice, all))).map_err(|_| "That job has finished.".to_string())
}
