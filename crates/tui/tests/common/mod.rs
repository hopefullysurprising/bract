#![allow(dead_code)]

use std::fs;
use std::path::PathBuf;

use bract::app::{App, AppResult};
use bract::data::loader::SyncLoader;
use bract::data::node::Node;
use bract::data::source::mise_tasks::nodes_from_spec;
use bract::data::source::mise_tools::HelpToolSource;
use bract::data::source::{HelpProvider, Loaded, Source};
use bract::ui::form::FormView;
use bract::ui::miller::MillerView;
use bract::ui::RunSpec;
use helptext_parser::InputFormat;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// Serves `--help` text from on-disk fixtures, keyed by binary + subcommand path,
/// so tool sources can be exercised without spawning real processes.
pub struct FixtureHelpProvider {
    dir: PathBuf,
    prefix: String,
}

impl FixtureHelpProvider {
    pub fn new(dir: PathBuf, prefix: &str) -> Self {
        Self { dir, prefix: prefix.to_string() }
    }
}

impl HelpProvider for FixtureHelpProvider {
    fn fetch_help(
        &self,
        _binary: &str,
        subcommand_path: &[&str],
    ) -> Result<String, Box<dyn std::error::Error>> {
        let suffix = if subcommand_path.is_empty() {
            "root".to_string()
        } else {
            subcommand_path.join("_")
        };
        let path = self.dir.join(format!("{}_{suffix}.txt", self.prefix));
        fs::read_to_string(&path).map_err(|e| format!("read fixture {}: {e}", path.display()).into())
    }
}

/// A source whose entire (already-built) node tree is handed over at the root —
/// used to exercise navigation/run over fixture-derived mise tasks.
pub struct StaticSource {
    id: String,
    name: String,
    bin: Vec<String>,
    separator: String,
    roots: Vec<Node>,
}

impl Source for StaticSource {
    fn tool_id(&self) -> &str {
        &self.id
    }
    fn tool_name(&self) -> &str {
        &self.name
    }
    fn tool_bin(&self) -> Vec<String> {
        self.bin.clone()
    }
    fn tool_path_separator(&self) -> &str {
        &self.separator
    }
    fn load(&self, command_path: &[String]) -> Result<Loaded, Box<dyn std::error::Error>> {
        let children = if command_path.is_empty() { self.roots.clone() } else { vec![] };
        Ok(Loaded { description: String::new(), runnable: false, flags: vec![], args: vec![], children })
    }
}

pub fn mani_source() -> Box<dyn Source> {
    let provider = FixtureHelpProvider::new(fixtures_dir().join("cli-help"), "mani_0.32.0");
    Box::new(HelpToolSource::new(
        "mani".to_string(),
        InputFormat::CobraHelptext,
        Box::new(provider),
    ))
}

pub fn cli_source() -> Box<dyn Source> {
    let provider = FixtureHelpProvider::new(fixtures_dir().join("cli-help"), "gh_2.92.0");
    Box::new(HelpToolSource::new(
        "gh".to_string(),
        InputFormat::CobraHelptext,
        Box::new(provider),
    ))
}

pub fn kubectl_source() -> Box<dyn Source> {
    let provider = FixtureHelpProvider::new(fixtures_dir().join("cli-help"), "kubectl_1.36.2");
    Box::new(HelpToolSource::new(
        "kubectl".to_string(),
        InputFormat::CobraHelptext,
        Box::new(provider),
    ))
}

pub fn az_source() -> Box<dyn Source> {
    let provider = FixtureHelpProvider::new(fixtures_dir().join("knack-help"), "az_2.87.0");
    Box::new(HelpToolSource::new("az".to_string(), InputFormat::KnackHelptext, Box::new(provider)))
}

pub fn task_source() -> Box<dyn Source> {
    let content =
        fs::read_to_string(fixtures_dir().join("mise-usage/collisions.kdl")).unwrap();
    let spec = helptext_parser::parse(InputFormat::UsageKdl, &content).unwrap();
    Box::new(StaticSource {
        id: "mise_tasks".to_string(),
        name: "Mise Tasks".to_string(),
        bin: vec!["mise".to_string(), "run".to_string()],
        separator: ":".to_string(),
        roots: nodes_from_spec(&spec, "mise_tasks"),
    })
}

pub struct Session {
    terminal: Terminal<TestBackend>,
    app: App,
    last_result: Option<AppResult>,
}

impl Session {
    pub fn new(sources: Vec<Box<dyn Source>>, width: u16, height: u16) -> Self {
        let miller = MillerView::with_loader(sources, |s| Box::new(SyncLoader::new(s)));
        let app = App::new(Box::new(miller));
        let terminal = Terminal::new(TestBackend::new(width, height)).expect("test terminal");
        Self { terminal, app, last_result: None }
    }

    fn miller(&mut self) -> &mut MillerView {
        self.app
            .current_view_mut()
            .expect("no current view")
            .as_any_mut()
            .downcast_mut::<MillerView>()
            .expect("expected MillerView on top of stack")
    }

    /// Navigate to a command by display-name path, loading lazily as needed.
    pub fn navigate(&mut self, path: &[&str]) {
        let resolved = self.miller().select_path(path);
        assert!(resolved, "could not resolve command path {path:?}");
    }

    pub fn focused_runnable(&mut self) -> bool {
        self.miller().focused_runnable()
    }

    pub fn focused_expandable(&mut self) -> bool {
        self.miller().focused_expandable()
    }

    /// Run an idle tick (background peeks + loads) to completion.
    pub fn pump(&mut self) {
        self.miller().pump();
    }

    pub fn unresolved_visible(&mut self) -> usize {
        self.miller().unresolved_visible()
    }

    /// Type a query into the active-column filter (opens it with `/`).
    pub fn filter(&mut self, query: &str) {
        self.tick_key(KeyCode::Char('/'));
        for c in query.chars() {
            self.tick_key(KeyCode::Char(c));
        }
    }

    pub fn form_section_labels(&mut self) -> Vec<String> {
        self.expect_form().section_labels()
    }

    pub fn form_field_names(&mut self) -> Vec<String> {
        self.expect_form().field_names()
    }

    pub fn set_field(&mut self, name: &str, value: &str) -> bool {
        self.expect_form().set_field(name, value)
    }

    pub fn focused_command(&mut self) -> Option<String> {
        self.miller().focused_command().map(str::to_string)
    }

    pub fn focused_description(&mut self) -> Option<String> {
        self.miller().focused_description()
    }

    pub fn root_names(&mut self) -> Vec<String> {
        self.miller().root_names()
    }

    pub fn pending_loads(&mut self) -> usize {
        self.miller().pending_loads()
    }

    pub fn focused_loaded(&mut self) -> bool {
        self.miller().focused_loaded()
    }

    pub fn press_down(&mut self) {
        self.tick_key(KeyCode::Down);
    }

    pub fn press_enter(&mut self) {
        self.tick_key(KeyCode::Enter);
    }

    pub fn press_run_key(&mut self) {
        self.tick_key(KeyCode::Char('r'));
    }

    pub fn active_depth(&mut self) -> usize {
        self.miller().depth()
    }

    /// Whether a run form is currently on top of the view stack.
    pub fn on_form(&mut self) -> bool {
        self.app
            .current_view_mut()
            .map(|v| v.as_any_mut().downcast_mut::<FormView>().is_some())
            .unwrap_or(false)
    }

    /// Open the run form for the focused command (the `r` key works for both
    /// leaves and runnable branches).
    pub fn open_run_form(&mut self) {
        self.tick_key(KeyCode::Char('r'));
    }

    pub fn run(&mut self) -> RunSpec {
        self.expect_form().run_spec()
    }

    pub fn run_via_form(&mut self) -> RunSpec {
        self.open_run_form();
        self.run()
    }

    pub fn render(&mut self) {
        self.app.render(&mut self.terminal).expect("render failed");
    }

    pub fn screen(&mut self) -> String {
        self.render();
        format!("{}", self.terminal.backend())
    }

    fn expect_form(&mut self) -> &mut FormView {
        self.app
            .current_view_mut()
            .expect("no current view")
            .as_any_mut()
            .downcast_mut::<FormView>()
            .expect("expected FormView on top of stack")
    }

    fn tick_key(&mut self, code: KeyCode) {
        let event = Event::Key(KeyEvent::new(code, KeyModifiers::empty()));
        if let Some(result) = self.app.tick(event) {
            self.last_result = Some(result);
        }
    }
}
