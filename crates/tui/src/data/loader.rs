//! Lazy child loading. The UI requests a node's details by id; results arrive
//! later via [`Loader::poll`]. The production [`BackgroundLoader`] runs fetches
//! on a pool of worker threads so a slow `--help` (e.g. `az`, ~1–2s) never blocks
//! the event loop; the [`SyncLoader`] resolves inline for deterministic tests.
//!
//! The pool is a **bag of tasks**: workers pull from one shared queue, and what a
//! fetch reveals — a node's children — is handed back to the caller, who may drop
//! more tasks into the same bag. Nothing waits for its own children, so a parent
//! never occupies a worker while its subtree is fetched. Callers that need to know
//! when the bag has drained count what they put in against what they took out; see
//! [`crate::data::export`].
//!
//! Requests carry a [`Priority`]: the item the user is focused on is `High`,
//! while speculative one-level-deeper "peeks" (used to discover Cobra
//! expandability) are `Low`. Workers always drain High before Low, so a fresh
//! focus is never stuck behind a backlog of peeks.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};
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

/// The shared bag. `queued` and `in_flight` together answer "is this node already
/// being dealt with?" — the queues alone cannot, because a task stops being queued
/// the moment a worker picks it up and would otherwise be fetched a second time.
struct Bag {
    state: Mutex<BagState>,
    ready: Condvar,
}

struct BagState {
    high: VecDeque<LoadRequest>,
    low: VecDeque<LoadRequest>,
    queued: HashMap<String, Priority>,
    in_flight: HashSet<String>,
    closed: bool,
}

impl Bag {
    fn put(&self, req: LoadRequest) {
        let mut state = self.state.lock().unwrap();
        if state.in_flight.contains(&req.node_id) {
            return;
        }
        // Promote: a High request for a node already queued at Low jumps it to the
        // front queue rather than fetching it twice. This is how a focus overtakes
        // the speculative-peek backlog for the same node.
        match state.queued.get(&req.node_id) {
            Some(Priority::High) => return,
            Some(Priority::Low) if matches!(req.priority, Priority::Low) => return,
            Some(Priority::Low) => {
                if let Some(pos) = state.low.iter().position(|r| r.node_id == req.node_id) {
                    state.low.remove(pos);
                }
            }
            None => {}
        }
        state.queued.insert(req.node_id.clone(), req.priority);
        match req.priority {
            Priority::High => state.high.push_back(req),
            Priority::Low => state.low.push_back(req),
        }
        self.ready.notify_one();
    }

    fn take(&self) -> Option<LoadRequest> {
        let mut state = self.state.lock().unwrap();
        loop {
            if state.closed {
                return None;
            }
            if let Some(req) = state.high.pop_front().or_else(|| state.low.pop_front()) {
                state.queued.remove(&req.node_id);
                state.in_flight.insert(req.node_id.clone());
                return Some(req);
            }
            state = self.ready.wait(state).unwrap();
        }
    }

    fn done(&self, node_id: &str) {
        self.state.lock().unwrap().in_flight.remove(node_id);
    }

    fn close(&self) {
        self.state.lock().unwrap().closed = true;
        self.ready.notify_all();
    }
}

pub struct BackgroundLoader {
    bag: Arc<Bag>,
    rx: Receiver<LoadOutcome>,
    /// Shared with the workers so cache hits can be resolved on the calling thread
    /// without a channel round-trip.
    sources: Arc<SourceIndex>,
}

impl BackgroundLoader {
    pub fn new(sources: Vec<Box<dyn Source>>) -> Self {
        let workers = thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
        Self::with_workers(sources, workers)
    }

    /// A pool of an explicit size. The fetches are subprocesses, so the useful
    /// width is the machine's, not the queue's — one worker reproduces the old
    /// single-threaded behaviour for tests that want it.
    pub fn with_workers(sources: Vec<Box<dyn Source>>, workers: usize) -> Self {
        let sources = Arc::new(index(sources));
        let (tx_out, rx_out) = mpsc::channel::<LoadOutcome>();
        let bag = Arc::new(Bag {
            state: Mutex::new(BagState {
                high: VecDeque::new(),
                low: VecDeque::new(),
                queued: HashMap::new(),
                in_flight: HashSet::new(),
                closed: false,
            }),
            ready: Condvar::new(),
        });

        for _ in 0..workers.max(1) {
            let bag = Arc::clone(&bag);
            let sources = Arc::clone(&sources);
            let tx = tx_out.clone();
            thread::spawn(move || worker(&bag, &sources, &tx));
        }

        Self { bag, rx: rx_out, sources }
    }

    /// Block until one outcome is available, for callers with no event loop of
    /// their own. `None` once every worker is gone.
    pub fn wait(&self) -> Option<LoadOutcome> {
        self.rx.recv().ok()
    }
}

impl Drop for BackgroundLoader {
    fn drop(&mut self) {
        self.bag.close();
    }
}

fn worker(bag: &Bag, sources: &SourceIndex, tx: &Sender<LoadOutcome>) {
    while let Some(req) = bag.take() {
        let loaded = run(sources, &req);
        bag.done(&req.node_id);
        if tx.send(LoadOutcome { node_id: req.node_id, loaded }).is_err() {
            break;
        }
    }
}

impl Loader for BackgroundLoader {
    fn request(&self, req: LoadRequest) {
        self.bag.put(req);
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
