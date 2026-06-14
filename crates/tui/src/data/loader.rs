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
    /// Resolve `req` synchronously iff it can be served cheaply (its `--help` is
    /// already cached on disk). Returns `None` when a real fetch is needed — the
    /// caller then falls back to [`request`]. This lets the UI skip the
    /// background round-trip (and the spinner) for the common cache-hit case,
    /// where the whole load is tens of microseconds.
    fn load_cached(&self, _req: &LoadRequest) -> Option<Result<Loaded, String>> {
        None
    }
}

fn index(sources: Vec<Box<dyn Source>>) -> HashMap<String, Arc<dyn Source>> {
    sources
        .into_iter()
        .map(|s| (s.tool_id().to_string(), Arc::from(s)))
        .collect()
}

type SourceIndex = HashMap<String, Arc<dyn Source>>;

fn run(sources: &SourceIndex, req: &LoadRequest) -> Result<Loaded, String> {
    match sources.get(&req.tool_id) {
        Some(src) => src.load(&req.command_path).map_err(|e| e.to_string()),
        None => Err(format!("no source for tool '{}'", req.tool_id)),
    }
}

/// Resolve `req` now iff the source reports its help is already cached. Shared by
/// both loaders so the synchronous fast-path behaves identically in tests and in
/// production.
fn run_if_cached(sources: &SourceIndex, req: &LoadRequest) -> Option<Result<Loaded, String>> {
    let src = sources.get(&req.tool_id)?;
    if src.cached(&req.command_path) {
        Some(src.load(&req.command_path).map_err(|e| e.to_string()))
    } else {
        None
    }
}

pub struct BackgroundLoader {
    tx: Sender<LoadRequest>,
    rx: Receiver<LoadOutcome>,
    /// Shared with the worker so cache hits can be resolved on the calling thread
    /// without a channel round-trip.
    sources: Arc<SourceIndex>,
}

impl BackgroundLoader {
    pub fn new(sources: Vec<Box<dyn Source>>) -> Self {
        let sources = Arc::new(index(sources));
        let (tx_req, rx_req) = mpsc::channel::<LoadRequest>();
        let (tx_out, rx_out) = mpsc::channel::<LoadOutcome>();

        let worker_sources = Arc::clone(&sources);
        thread::spawn(move || worker(&worker_sources, rx_req, tx_out));

        Self { tx: tx_req, rx: rx_out, sources }
    }
}

fn worker(sources: &SourceIndex, rx: Receiver<LoadRequest>, tx: Sender<LoadOutcome>) {
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
        let loaded = run(sources, &req);
        if tx.send(LoadOutcome { node_id: req.node_id, loaded }).is_err() {
            break;
        }
    }
}

fn enqueue(high: &mut VecDeque<LoadRequest>, low: &mut VecDeque<LoadRequest>, req: LoadRequest) {
    // Dedup by node id, and promote: a High request for a node already queued at
    // Low jumps it to the front queue rather than fetching it twice. This is how a
    // focus (High) overtakes the speculative-peek (Low) backlog for the same node.
    if high.iter().any(|r| r.node_id == req.node_id) {
        return;
    }
    if let Some(pos) = low.iter().position(|r| r.node_id == req.node_id) {
        if matches!(req.priority, Priority::High) {
            low.remove(pos);
            high.push_back(req);
        }
        return;
    }
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

    fn load_cached(&self, req: &LoadRequest) -> Option<Result<Loaded, String>> {
        run_if_cached(&self.sources, req)
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

    fn load_cached(&self, req: &LoadRequest) -> Option<Result<Loaded, String>> {
        run_if_cached(&self.sources, req)
    }
}
