//! Lazy child loading. The UI requests a node's details by id; results arrive
//! later via [`Loader::poll`]. The production [`BackgroundLoader`] runs fetches
//! on a worker thread so a slow `--help` (e.g. `az`, ~1–2s) never blocks the
//! event loop; the [`SyncLoader`] resolves inline for deterministic tests.
//!
//! Requests carry a [`Priority`]: the item the user is focused on is `High`,
//! while speculative one-level-deeper "peeks" (used to discover Cobra
//! expandability) are `Low`. The worker always drains High before Low and
//! re-checks for new High work between items, so a fresh focus is never stuck
//! behind a backlog of peeks.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;

use super::source::{Loaded, Source};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    High,
    Low,
}

pub struct LoadRequest {
    pub node_id: String,
    pub tool_id: String,
    pub command_path: Vec<String>,
    pub priority: Priority,
}

pub struct LoadOutcome {
    pub node_id: String,
    pub loaded: Result<Loaded, String>,
}

pub trait Loader {
    fn request(&self, req: LoadRequest);
    fn poll(&self) -> Vec<LoadOutcome>;
}

fn index(sources: Vec<Box<dyn Source>>) -> HashMap<String, Arc<dyn Source>> {
    sources
        .into_iter()
        .map(|s| (s.tool_id().to_string(), Arc::from(s)))
        .collect()
}

fn run(sources: &HashMap<String, Arc<dyn Source>>, req: &LoadRequest) -> Result<Loaded, String> {
    match sources.get(&req.tool_id) {
        Some(src) => src.load(&req.command_path).map_err(|e| e.to_string()),
        None => Err(format!("no source for tool '{}'", req.tool_id)),
    }
}

pub struct BackgroundLoader {
    tx: Sender<LoadRequest>,
    rx: Receiver<LoadOutcome>,
}

impl BackgroundLoader {
    pub fn new(sources: Vec<Box<dyn Source>>) -> Self {
        let sources = index(sources);
        let (tx_req, rx_req) = mpsc::channel::<LoadRequest>();
        let (tx_out, rx_out) = mpsc::channel::<LoadOutcome>();

        thread::spawn(move || worker(sources, rx_req, tx_out));

        Self { tx: tx_req, rx: rx_out }
    }
}

fn worker(
    sources: HashMap<String, Arc<dyn Source>>,
    rx: Receiver<LoadRequest>,
    tx: Sender<LoadOutcome>,
) {
    let mut high: VecDeque<LoadRequest> = VecDeque::new();
    let mut low: VecDeque<LoadRequest> = VecDeque::new();

    loop {
        // Block only when there is nothing queued; otherwise keep draining so
        // newly-arrived High work can jump ahead of pending Low peeks.
        if high.is_empty() && low.is_empty() {
            match rx.recv() {
                Ok(req) => enqueue(&mut high, &mut low, req),
                Err(_) => break,
            }
        }
        while let Ok(req) = rx.try_recv() {
            enqueue(&mut high, &mut low, req);
        }

        let Some(req) = high.pop_front().or_else(|| low.pop_front()) else { continue };
        let loaded = run(&sources, &req);
        if tx.send(LoadOutcome { node_id: req.node_id, loaded }).is_err() {
            break;
        }
    }
}

fn enqueue(high: &mut VecDeque<LoadRequest>, low: &mut VecDeque<LoadRequest>, req: LoadRequest) {
    match req.priority {
        Priority::High => high.push_back(req),
        Priority::Low => low.push_back(req),
    }
}

impl Loader for BackgroundLoader {
    fn request(&self, req: LoadRequest) {
        let _ = self.tx.send(req);
    }

    fn poll(&self) -> Vec<LoadOutcome> {
        self.rx.try_iter().collect()
    }
}

/// Resolves requests inline and queues the outcomes for the next `poll`. Lets
/// tests drive loading deterministically without threads or timing.
pub struct SyncLoader {
    sources: HashMap<String, Arc<dyn Source>>,
    pending: RefCell<Vec<LoadOutcome>>,
}

impl SyncLoader {
    pub fn new(sources: Vec<Box<dyn Source>>) -> Self {
        Self { sources: index(sources), pending: RefCell::new(Vec::new()) }
    }
}

impl Loader for SyncLoader {
    fn request(&self, req: LoadRequest) {
        let loaded = run(&self.sources, &req);
        self.pending.borrow_mut().push(LoadOutcome { node_id: req.node_id, loaded });
    }

    fn poll(&self) -> Vec<LoadOutcome> {
        std::mem::take(&mut *self.pending.borrow_mut())
    }
}
