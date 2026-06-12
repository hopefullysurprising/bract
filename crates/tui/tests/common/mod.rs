#![allow(dead_code)]

use std::fs;
use std::path::PathBuf;

use bract::app::{App, AppResult};
use bract::data::commands::Tool;
use bract::data::source::mise_tools::MiseToolSource;
use bract::data::source::{assemble_tools, HelpProvider, Source};
use bract::ui::browse::BrowseView;
use bract::ui::form::FormView;
use bract::ui::RunSpec;
use helptext_parser::InputFormat;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;

pub struct FixtureHelpProvider {
    fixtures_dir: PathBuf,
    binary_to_prefix: Vec<(String, String)>,
}

impl FixtureHelpProvider {
    pub fn new(fixtures_dir: PathBuf, binary_to_prefix: Vec<(String, String)>) -> Self {
        Self { fixtures_dir, binary_to_prefix }
    }
}

impl HelpProvider for FixtureHelpProvider {
    fn fetch_help(
        &self,
        binary: &str,
        subcommand_path: &[&str],
    ) -> Result<String, Box<dyn std::error::Error>> {
        let prefix = self
            .binary_to_prefix
            .iter()
            .find(|(b, _)| b == binary)
            .map(|(_, p)| p.as_str())
            .ok_or_else(|| format!("no fixture prefix registered for {binary}"))?;
        let suffix = if subcommand_path.is_empty() {
            "root".to_string()
        } else {
            subcommand_path.join("_")
        };
        let path = self.fixtures_dir.join(format!("{prefix}_{suffix}.txt"));
        fs::read_to_string(&path)
            .map_err(|e| format!("read fixture {}: {e}", path.display()).into())
    }
}

pub struct Session {
    terminal: Terminal<TestBackend>,
    app: App,
    last_result: Option<AppResult>,
}

impl Session {
    pub fn new(tools: Vec<Tool>, width: u16, height: u16) -> Self {
        let browse = BrowseView::new(&tools).expect("BrowseView::new");
        let app = App::new(Box::new(browse));
        let terminal = Terminal::new(TestBackend::new(width, height)).expect("test terminal");
        Self { terminal, app, last_result: None }
    }

    pub fn select_command(&mut self, path: &[&str]) {
        {
            let view = self.app.current_view_mut().expect("no current view");
            let browse = view
                .as_any_mut()
                .downcast_mut::<BrowseView>()
                .expect("expected BrowseView on top of stack");
            assert!(
                browse.select_command(path),
                "could not resolve command path {path:?}"
            );
        }
        self.tick_key(KeyCode::Enter);
    }

    pub fn set_field(&mut self, name: &str, value: &str) {
        let form = self.expect_form();
        assert!(form.set_field(name, value), "field {name} not found");
    }

    pub fn toggle_field(&mut self, name: &str) {
        let form = self.expect_form();
        assert!(form.toggle_field(name), "field {name} not found");
    }

    pub fn run(&mut self) -> RunSpec {
        let form = self.expect_form();
        form.run_spec()
    }

    pub fn render(&mut self) {
        self.app.render(&mut self.terminal).expect("render failed");
    }

    pub fn screen(&mut self) -> String {
        self.render();
        format!("{}", self.terminal.backend())
    }

    fn expect_form(&mut self) -> &mut FormView {
        let view = self.app.current_view_mut().expect("no current view");
        view.as_any_mut()
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

pub fn build_task_tools() -> Vec<Tool> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/mise-usage/app_0.1.0_collisions.kdl");
    let content = fs::read_to_string(&path).expect("read task fixture");
    let spec = helptext_parser::parse(InputFormat::UsageKdl, &content).expect("parse usage kdl");
    let commands = bract::data::source::mise_tasks::commands_from_spec(&spec, "mise");
    vec![Tool {
        id: "mise_tasks".to_string(),
        name: "Mise Tasks".to_string(),
        bin: vec!["mise".to_string(), "run".to_string()],
        path_separator: ":".to_string(),
        description: String::new(),
        flags: vec![],
        args: vec![],
        commands,
    }]
}

pub fn build_mani_tools() -> Vec<Tool> {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cli-help");
    let provider = FixtureHelpProvider::new(
        fixtures,
        vec![("mani".to_string(), "mani_0.32.0".to_string())],
    );
    let source = MiseToolSource::new(
        "mani".to_string(),
        "mani".to_string(),
        InputFormat::CobraHelptext,
        Box::new(provider),
    );
    let sources: Vec<Box<dyn Source>> = vec![Box::new(source)];
    assemble_tools(sources).expect("assemble_tools")
}
