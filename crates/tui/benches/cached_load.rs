//! Measures the cost of the cached-render hot path: the work that happens when a
//! subtree is expanded and its `--help` is already on disk. Establishes whether
//! the spinner-on-cache-hit is compute (parse/build) or latency (event loop).
//!
//! Run: `cargo bench -p bract --bench cached_load`

use std::hint::black_box;
use std::path::PathBuf;

use criterion::{criterion_group, criterion_main, Criterion};

use bract::data::loader::{BackgroundLoader, LoadRequest, Loader, Priority};
use bract::data::source::help_cache::CachingHelpProvider;
use bract::data::source::mise_tools::HelpToolSource;
use bract::data::source::{HelpProvider, Source};
use helptext_parser::InputFormat;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(fixtures().join(rel)).expect("fixture")
}

/// Always returns the same help text — stands in for the slow `mise exec` so the
/// only thing under test is parse/build, never a subprocess.
struct StaticProvider(String);
impl HelpProvider for StaticProvider {
    fn fetch_help(&self, _b: &str, _p: &[&str]) -> Result<String, Box<dyn std::error::Error>> {
        Ok(self.0.clone())
    }
}

fn bench(c: &mut Criterion) {
    let az_root = read("knack-help/az_2.87.0_root.txt"); // 8.7 KB, the largest realistic single help
    let gh_root = read("cli-help/gh_2.92.0_root.txt");

    // (1) Pure parser cost — no I/O, no node build.
    c.bench_function("parse_only/az_root_knack", |b| {
        b.iter(|| helptext_parser::parse(InputFormat::KnackHelptext, black_box(&az_root)).unwrap())
    });
    c.bench_function("parse_only/gh_root_cobra", |b| {
        b.iter(|| helptext_parser::parse(InputFormat::CobraHelptext, black_box(&gh_root)).unwrap())
    });

    // (2) Full cached load = disk read + parse + node build, exactly what the
    // background loader runs when expanding a node whose help is cached.
    let (az_src, _g1) = warm("az", InputFormat::KnackHelptext, az_root);
    c.bench_function("cached_load/az_root", |b| {
        b.iter(|| black_box(az_src.load(black_box(&[])).unwrap()))
    });

    let (gh_src, _g2) = warm("gh", InputFormat::CobraHelptext, gh_root.clone());
    c.bench_function("cached_load/gh_root", |b| {
        b.iter(|| black_box(gh_src.load(black_box(&[])).unwrap()))
    });

    // (3) The async round-trip the *old* path paid even on a cache hit: enqueue →
    // background thread wakeup → channel → the same cached load → poll. The event
    // loop then added up to TICK (80ms) on top before integrating. The sync path
    // (case 2) removes both. This is the before/after for the spinner fix.
    let tmp = tempfile::tempdir().unwrap();
    let provider = CachingHelpProvider::new(
        Box::new(StaticProvider(gh_root)),
        tmp.path().to_path_buf(),
        "bench".to_string(),
    );
    let rt_src = HelpToolSource::new("gh".to_string(), InputFormat::CobraHelptext, Box::new(provider));
    rt_src.load(&[]).unwrap(); // prime
    let loader = BackgroundLoader::new(vec![Box::new(rt_src)]);
    c.bench_function("async_roundtrip/gh_root_cached", |b| {
        b.iter(|| {
            loader.request(LoadRequest {
                node_id: "gh".to_string(),
                tool_id: "gh".to_string(),
                command_path: vec![],
                priority: Priority::High,
            });
            loop {
                if !loader.poll().is_empty() {
                    break;
                }
                std::hint::spin_loop();
            }
        })
    });
}

/// Build a source whose cache file is already primed, so `load` is a pure hit.
/// Returns the tempdir guard too — it must outlive the source.
fn warm(name: &str, fmt: InputFormat, content: String) -> (HelpToolSource, tempfile::TempDir) {
    let tmp = tempfile::tempdir().unwrap();
    let provider = CachingHelpProvider::new(
        Box::new(StaticProvider(content)),
        tmp.path().to_path_buf(),
        "bench".to_string(),
    );
    let source = HelpToolSource::new(name.to_string(), fmt, Box::new(provider));
    source.load(&[]).unwrap();
    (source, tmp)
}

criterion_group!(benches, bench);
criterion_main!(benches);
