mod field;
mod text_input;
mod toggle;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Padding, Paragraph};
use ratatui::Frame;

use field::{FieldValue, FormField};
use text_input::TextInput;
use toggle::Toggle;

use super::{RunSpec, View, ViewAction};
use crate::data::env_params::{resolve, EnvSource};
use crate::data::memory::{FieldStat, FormMemory};
use crate::data::node::{FlagKind, Node};

enum FieldMeta {
    Arg,
    BoolFlag { long: Option<String>, short: Option<char> },
    ValueFlag { long: Option<String>, short: Option<char> },
}

struct FormSection {
    label: String,
    fields: Vec<(FieldMeta, Box<dyn FormField>)>,
}

pub struct FormView {
    title: String,
    bin: Vec<String>,
    command_path: Vec<String>,
    path_separator: String,
    description: String,
    sections: Vec<FormSection>,
    total_fields: usize,
    focused: usize,
    scroll_offset: u16,
    /// Identity under which fills are remembered: the tool and the leaf command.
    tool_id: String,
    command_key: String,
    memory: Arc<dyn FormMemory>,
}

impl FormView {
    /// `ancestors` is the command chain from the topmost group down to the leaf
    /// being run (leaf last). Each level contributes a labelled section of its
    /// own flags/args — so flags defined on a parent command accumulate alongside
    /// the leaf's, the way they did before lazy loading.
    pub fn new(
        ancestors: &[&Node],
        bin: &[String],
        path_separator: &str,
        tool_id: &str,
        env: &dyn EnvSource,
        memory: Arc<dyn FormMemory>,
    ) -> Self {
        let leaf = ancestors.last();
        let command_names = leaf.map(|n| n.command_path.clone()).unwrap_or_default();
        let display_bin = bin.join(" ");
        let display_path = command_names.join(path_separator);
        let title = if display_path.is_empty() {
            display_bin
        } else {
            format!("{display_bin} {display_path}")
        };

        let description = leaf.map(|n| n.description.clone()).unwrap_or_default();

        // Build leaf-first so the command's own parameters lead; the leaf section
        // is unlabelled, parent levels are labelled by command name. The detail
        // card in the browser shares `param_levels`, so what you preview matches
        // what you can fill in here.
        // Walk leaf → root so the command's own parameters win: a flag the leaf
        // declares locally (kubectl repeats inherited flags inline under every
        // command's `Options:`) is shown once, under the leaf, and skipped where
        // an ancestor re-declares it. Sections emptied by dedup are dropped.
        let command_key = command_names.join(path_separator);
        let stats = memory.stats(tool_id, &command_key);

        // Env params prefix-match the full command being run (tool first), so a
        // value set on an ancestor applies to every field regardless of the
        // level it's defined at.
        let full_path: Vec<String> =
            std::iter::once(tool_id.to_string()).chain(command_names.iter().cloned()).collect();

        let mut seen = HashSet::new();
        let sections: Vec<FormSection> = param_levels(ancestors)
            .into_iter()
            .map(|(label, node)| FormSection {
                label,
                fields: build_fields(node, &mut seen, &stats, env, &full_path),
            })
            .filter(|section| !section.fields.is_empty())
            .collect();
        let total_fields = sections.iter().map(|s| s.fields.len()).sum();

        Self {
            title,
            bin: bin.to_vec(),
            command_path: command_names,
            path_separator: path_separator.to_string(),
            description,
            sections,
            total_fields,
            focused: 0,
            scroll_offset: 0,
            tool_id: tool_id.to_string(),
            command_key,
            memory,
        }
    }

    fn focus_next(&mut self) {
        if self.total_fields > 0 {
            self.focused = (self.focused + 1) % self.total_fields;
        }
    }

    fn focus_prev(&mut self) {
        if self.total_fields > 0 {
            self.focused = (self.focused + self.total_fields - 1) % self.total_fields;
        }
    }

    fn focused_field_mut(&mut self) -> Option<&mut Box<dyn FormField>> {
        let mut remaining = self.focused;
        for section in &mut self.sections {
            if remaining < section.fields.len() {
                return Some(&mut section.fields[remaining].1);
            }
            remaining -= section.fields.len();
        }
        None
    }

    pub fn set_field(&mut self, name: &str, value: &str) -> bool {
        for section in &mut self.sections {
            for (_meta, field) in &mut section.fields {
                if field.name() == name {
                    return field.set_text(value);
                }
            }
        }
        false
    }

    pub fn toggle_field(&mut self, name: &str) -> bool {
        for section in &mut self.sections {
            for (_meta, field) in &mut section.fields {
                if field.name() == name {
                    return field.toggle();
                }
            }
        }
        false
    }

    /// Section labels in render order (`""` is the leaf's own parameters); used by
    /// tests to confirm parent-level flags are accumulated into their own groups.
    pub fn section_labels(&self) -> Vec<String> {
        self.sections.iter().map(|s| s.label.clone()).collect()
    }

    /// Field names across every section, in render order; used by tests to
    /// confirm a flag inherited at multiple levels is shown exactly once.
    pub fn field_names(&self) -> Vec<String> {
        self.sections
            .iter()
            .flat_map(|s| s.fields.iter().map(|(_, f)| f.name().to_string()))
            .collect()
    }

    pub fn run_spec(&self) -> RunSpec {
        // mise joins task segments into one token (`app:check`); cobra keeps them
        // as separate argv tokens (`list projects`). Joining on the tool's separator
        // and re-splitting on whitespace yields the right argv for both.
        let mut args: Vec<String> = self
            .command_path
            .join(&self.path_separator)
            .split_whitespace()
            .map(String::from)
            .collect();
        let mut positional = Vec::new();

        for section in &self.sections {
            for (meta, field) in &section.fields {
                match (meta, field.value()) {
                    (FieldMeta::Arg, FieldValue::Text(v)) if !v.is_empty() => {
                        positional.push(v);
                    }
                    (FieldMeta::BoolFlag { long, short }, FieldValue::Bool(true)) => {
                        args.push(flag_token(long, short));
                    }
                    (FieldMeta::ValueFlag { long, short }, FieldValue::Text(v)) if !v.is_empty() => {
                        // `--flag=value`, not `--flag value`: an optional-value flag
                        // (cobra's NoOptDefVal, e.g. kubectl `--dry-run`) treats a
                        // space-separated token as a positional, not its value. The
                        // `=` form binds the value across cobra/clap/click alike.
                        if let Some(l) = long {
                            args.push(format!("--{l}={v}"));
                        } else if let Some(s) = short {
                            args.push(format!("-{s}"));
                            args.push(v);
                        }
                    }
                    _ => {}
                }
            }
        }

        args.extend(positional);

        RunSpec {
            bin: self.bin.clone(),
            args,
        }
    }

    /// Persist what the user filled, so next time these fields sort higher and
    /// offer their value for recall. Called when the command is actually run.
    fn record_fills(&self) {
        for section in &self.sections {
            for (meta, field) in &section.fields {
                // Env-provided values are deliberately kept off disk.
                if field.is_env_sourced() {
                    continue;
                }
                match (meta, field.value()) {
                    (FieldMeta::Arg | FieldMeta::ValueFlag { .. }, FieldValue::Text(v))
                        if !v.is_empty() =>
                    {
                        self.memory.record(&self.tool_id, &self.command_key, field.name(), &v);
                    }
                    (FieldMeta::BoolFlag { .. }, FieldValue::Bool(true)) => {
                        self.memory.record(&self.tool_id, &self.command_key, field.name(), "true");
                    }
                    _ => {}
                }
            }
        }
    }
}

impl View for FormView {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn render(&mut self, frame: &mut Frame) {
        let area = centered_area(frame.area(), 72);

        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(Color::DarkGray))
            .title(format!(" {} ", self.title))
            .title_style(Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            .padding(Padding::new(2, 2, 1, 1));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let [content_area, footer_area] =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(2)]).areas(inner);

        let mut lines: Vec<Line> = Vec::new();
        let mut focused_line_start: u16 = 0;
        let mut focused_line_end: u16 = 0;

        if !self.description.is_empty() {
            lines.push(Line::from(Span::styled(
                self.description.clone(),
                Style::new().fg(Color::Gray).add_modifier(Modifier::ITALIC),
            )));
            lines.push(Line::raw(""));
        }

        let mut global_field_index = 0;

        for (section_idx, section) in self.sections.iter().enumerate() {
            if section_idx > 0 {
                lines.push(Line::raw(""));
            }
            if !section.label.is_empty() {
                lines.push(section_header(&section.label, content_area.width));
                lines.push(Line::raw(""));
            }

            for (_meta, field) in &section.fields {
                let focused = global_field_index == self.focused;
                if focused {
                    focused_line_start = lines.len() as u16;
                }
                lines.extend(field.render_lines(focused, content_area.width));
                if focused {
                    focused_line_end = lines.len() as u16;
                }
                lines.push(Line::raw(""));
                global_field_index += 1;
            }
        }

        if self.total_fields == 0 {
            lines.push(Line::from(Span::styled(
                "No parameters",
                Style::new()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::ITALIC),
            )));
        }

        let visible_height = content_area.height;
        if focused_line_end > self.scroll_offset + visible_height {
            self.scroll_offset = focused_line_end.saturating_sub(visible_height);
        }
        if focused_line_start < self.scroll_offset {
            self.scroll_offset = focused_line_start;
        }

        frame.render_widget(
            Paragraph::new(Text::from(lines)).scroll((self.scroll_offset, 0)),
            content_area,
        );
        frame.render_widget(footer(footer_area.width), footer_area);
    }

    fn handle_key(&mut self, key: KeyEvent) -> Option<ViewAction> {
        if key.code == KeyCode::Char('r') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.record_fills();
            return Some(ViewAction::Run(self.run_spec()));
        }

        match key.code {
            KeyCode::Esc => None,
            KeyCode::Tab => {
                self.focus_next();
                Some(ViewAction::Consumed)
            }
            KeyCode::BackTab => {
                self.focus_prev();
                Some(ViewAction::Consumed)
            }
            _ => {
                if let Some(field) = self.focused_field_mut() {
                    field.handle_key(key);
                }
                Some(ViewAction::Consumed)
            }
        }
    }
}

fn centered_area(area: Rect, max_width: u16) -> Rect {
    if area.width <= max_width {
        area
    } else {
        let x = area.x + (area.width - max_width) / 2;
        Rect::new(x, area.y, max_width, area.height)
    }
}

/// The command levels that contribute parameters, leaf-first: each is a section
/// label (empty for the leaf) and the node whose flags/args fill it. Shared by
/// the run form and the browser's detail card so the two never diverge on which
/// parameters a command exposes.
pub fn param_levels<'a>(ancestors: &[&'a Node]) -> Vec<(String, &'a Node)> {
    let leaf_index = ancestors.len().saturating_sub(1);
    ancestors
        .iter()
        .enumerate()
        .rev()
        .filter(|(_, node)| !node.args.is_empty() || !node.flags.is_empty())
        .map(|(i, node)| {
            let label = if i == leaf_index { String::new() } else { node.name.clone() };
            (label, *node)
        })
        .collect()
}

/// A field plus the keys it's ordered by: required-first (you can't run without
/// them), then most-frequently-filled, then original parser order as the stable
/// tiebreak.
struct RankedField {
    required: bool,
    count: u64,
    original: usize,
    meta: FieldMeta,
    field: Box<dyn FormField>,
}

fn build_fields(
    node: &Node,
    seen: &mut HashSet<String>,
    stats: &HashMap<String, FieldStat>,
    env: &dyn EnvSource,
    full_path: &[String],
) -> Vec<(FieldMeta, Box<dyn FormField>)> {
    let mut ranked: Vec<RankedField> = Vec::new();
    let stat = |name: &str| stats.get(name);

    // A text field starts empty unless an env var provides a value, in which case
    // it's pre-filled and tagged with the source variable.
    let prefill = |name: &str| -> (Vec<char>, usize, Option<String>) {
        match resolve(env, full_path, name) {
            Some(ev) => (ev.value.chars().collect(), ev.value.chars().count(), Some(ev.var_name)),
            None => (Vec::new(), 0, None),
        }
    };

    for arg in &node.args {
        if !seen.insert(format!("arg:{}", arg.name)) {
            continue;
        }
        let name = if arg.required {
            format!("<{}>", arg.name)
        } else {
            format!("[{}]", arg.name)
        };
        let st = stat(&name);
        let (chars, cursor, env_source) = prefill(&name);
        ranked.push(RankedField {
            required: arg.required,
            count: st.map(|s| s.count).unwrap_or(0),
            original: ranked.len(),
            meta: FieldMeta::Arg,
            field: Box::new(TextInput {
                name,
                help: arg.description.clone(),
                default: arg.default.clone(),
                remembered: st.and_then(|s| s.last_value.clone()),
                env_source,
                chars,
                cursor,
            }),
        });
    }

    for flag in &node.flags {
        let key = match (&flag.long, &flag.short) {
            (Some(long), _) => format!("flag:--{long}"),
            (None, Some(short)) => format!("flag:-{short}"),
            (None, None) => continue,
        };
        if !seen.insert(key) {
            continue;
        }
        let st = stat(&flag.name);
        let count = st.map(|s| s.count).unwrap_or(0);
        let original = ranked.len();
        match &flag.kind {
            FlagKind::Boolean => {
                // An env var with a truthy value pre-enables the toggle; either way
                // its presence marks the field env-controlled (and thus not saved).
                let env_hit = resolve(env, full_path, &flag.name);
                let (value, env_source) = match env_hit {
                    Some(ev) => (crate::data::env_params::is_truthy(&ev.value), Some(ev.var_name)),
                    None => (false, None),
                };
                ranked.push(RankedField {
                    required: flag.required,
                    count,
                    original,
                    meta: FieldMeta::BoolFlag { long: flag.long.clone(), short: flag.short },
                    field: Box::new(Toggle {
                        name: flag.name.clone(),
                        help: flag.description.clone(),
                        value,
                        env_source,
                    }),
                })
            }
            FlagKind::Value { default, .. } => {
                let (chars, cursor, env_source) = prefill(&flag.name);
                ranked.push(RankedField {
                    required: flag.required,
                    count,
                    original,
                    meta: FieldMeta::ValueFlag { long: flag.long.clone(), short: flag.short },
                    field: Box::new(TextInput {
                        name: flag.name.clone(),
                        help: flag.description.clone(),
                        default: default.clone(),
                        remembered: st.and_then(|s| s.last_value.clone()),
                        env_source,
                        chars,
                        cursor,
                    }),
                })
            }
        }
    }

    ranked.sort_by(|a, b| {
        b.required
            .cmp(&a.required)
            .then(b.count.cmp(&a.count))
            .then(a.original.cmp(&b.original))
    });
    ranked.into_iter().map(|r| (r.meta, r.field)).collect()
}

fn flag_token(long: &Option<String>, short: &Option<char>) -> String {
    if let Some(l) = long {
        format!("--{l}")
    } else if let Some(s) = short {
        format!("-{s}")
    } else {
        String::new()
    }
}

fn section_header(label: &str, width: u16) -> Line<'static> {
    let prefix = format!("── {} ", label);
    let remaining = (width as usize).saturating_sub(prefix.chars().count());
    Line::from(Span::styled(
        format!("{}{}", prefix, "─".repeat(remaining)),
        Style::new().fg(Color::DarkGray),
    ))
}

fn separator_line(width: u16) -> Line<'static> {
    Line::from(Span::styled(
        "─".repeat(width as usize),
        Style::new().fg(Color::DarkGray),
    ))
}

fn footer(width: u16) -> Paragraph<'static> {
    Paragraph::new(vec![
        separator_line(width),
        Line::from(vec![
            Span::styled(
                "^r",
                Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" run", Style::new().fg(Color::DarkGray)),
            Span::styled("  ·  ", Style::new().fg(Color::DarkGray)),
            Span::styled(
                "↹",
                Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" next", Style::new().fg(Color::DarkGray)),
            Span::styled("  ·  ", Style::new().fg(Color::DarkGray)),
            Span::styled(
                "space",
                Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" toggle", Style::new().fg(Color::DarkGray)),
            Span::styled("  ·  ", Style::new().fg(Color::DarkGray)),
            Span::styled(
                "→",
                Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" recall", Style::new().fg(Color::DarkGray)),
            Span::styled("  ·  ", Style::new().fg(Color::DarkGray)),
            Span::styled(
                "esc",
                Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" back", Style::new().fg(Color::DarkGray)),
        ]),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::env_params::NullEnv;
    use crate::data::node::{Arg, Children, Flag, FlagKind, NodeKind};
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use std::sync::Mutex;

    /// Returns whatever stats it's seeded with, ignoring tool/command — enough to
    /// drive the form's sort and recall without a real db.
    struct FakeMemory(HashMap<String, (u64, Option<String>)>);

    impl FormMemory for FakeMemory {
        fn record(&self, _: &str, _: &str, _: &str, _: &str) {}
        fn stats(&self, _: &str, _: &str) -> HashMap<String, FieldStat> {
            self.0
                .iter()
                .map(|(k, (count, last))| {
                    (k.clone(), FieldStat { count: *count, last_value: last.clone() })
                })
                .collect()
        }
    }

    /// Captures every `record` call so a test can assert what was (and wasn't)
    /// persisted on run.
    struct RecordingMemory(Mutex<Vec<String>>);
    impl FormMemory for RecordingMemory {
        fn record(&self, _tool: &str, _cmd: &str, field: &str, _value: &str) {
            self.0.lock().unwrap().push(field.to_string());
        }
        fn stats(&self, _: &str, _: &str) -> HashMap<String, FieldStat> {
            HashMap::new()
        }
    }

    struct FakeEnv(HashMap<String, String>);
    impl EnvSource for FakeEnv {
        fn get(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }
    }
    fn fake_env(pairs: &[(&str, &str)]) -> FakeEnv {
        FakeEnv(pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect())
    }

    fn value_flag(name: &str) -> Flag {
        Flag {
            name: format!("--{name}"),
            short: None,
            long: Some(name.to_string()),
            description: String::new(),
            required: false,
            kind: FlagKind::Value {
                arg_name: name.to_string(),
                default: String::new(),
                choices: vec![],
            },
        }
    }

    fn bool_flag(name: &str) -> Flag {
        Flag {
            name: format!("--{name}"),
            short: None,
            long: Some(name.to_string()),
            description: String::new(),
            required: false,
            kind: FlagKind::Boolean,
        }
    }

    fn leaf(flags: Vec<Flag>, args: Vec<Arg>) -> Node {
        Node {
            id: "t/cmd".into(),
            name: "cmd".into(),
            description: String::new(),
            kind: NodeKind::Leaf,
            runnable: true,
            flags,
            args,
            tool_id: "t".into(),
            command_path: vec!["cmd".into()],
            children: Children::Loaded(vec![]),
        }
    }

    fn build(node: &Node, stats: &[(&str, u64, Option<&str>)]) -> FormView {
        let map = stats
            .iter()
            .map(|(k, c, v)| (k.to_string(), (*c, v.map(str::to_string))))
            .collect();
        FormView::new(&[node], &["t".to_string()], " ", "t", &NullEnv, Arc::new(FakeMemory(map)))
    }

    #[test]
    fn frequently_filled_fields_sort_higher() {
        let node = leaf(vec![value_flag("alpha"), value_flag("beta"), value_flag("gamma")], vec![]);
        let form = build(&node, &[("--beta", 5, None), ("--gamma", 2, None)]);
        assert_eq!(form.field_names(), vec!["--beta", "--gamma", "--alpha"]);
    }

    #[test]
    fn required_fields_stay_on_top_regardless_of_frequency() {
        let req = Arg {
            name: "NAME".into(),
            description: String::new(),
            required: true,
            default: String::new(),
            choices: vec![],
        };
        let node = leaf(vec![value_flag("beta")], vec![req]);
        // --beta is filled far more often, but a required arg can't be buried.
        let form = build(&node, &[("--beta", 99, None)]);
        assert_eq!(form.field_names().first().map(String::as_str), Some("<NAME>"));
    }

    #[test]
    fn previous_value_recalls_with_right_arrow() {
        let node = leaf(vec![value_flag("image")], vec![]);
        let mut form = build(&node, &[("--image", 3, Some("nginx"))]);
        // The focused field is --image; → accepts the remembered ghost.
        form.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        assert!(
            form.run_spec().args.contains(&"--image=nginx".to_string()),
            "recalled value reaches the command: {:?}",
            form.run_spec().args
        );
    }

    #[test]
    fn an_untouched_remembered_value_is_not_sent() {
        let node = leaf(vec![value_flag("image")], vec![]);
        let form = build(&node, &[("--image", 3, Some("nginx"))]);
        // No key pressed: the ghost is a suggestion, not a value. Only the command
        // path is emitted — the remembered --image is not.
        let args = form.run_spec().args;
        assert!(
            !args.iter().any(|a| a.starts_with("--image")),
            "remembered value stays a ghost until recalled: {args:?}"
        );
    }

    // tool_id "t", command_path ["cmd"] → full path ["t","cmd"], so a tool-wide
    // var is BRACT_T__<PARAM>.
    fn env_form(node: &Node, env: FakeEnv, memory: Arc<dyn FormMemory>) -> FormView {
        FormView::new(&[node], &["t".to_string()], " ", "t", &env, memory)
    }

    #[test]
    fn env_var_prefills_a_field_and_is_applied_without_a_keypress() {
        let node = leaf(vec![value_flag("org")], vec![]);
        let env = fake_env(&[("BRACT_T__ORG", "myorg")]);
        let form = env_form(&node, env, Arc::new(FakeMemory(HashMap::new())));
        // No interaction: the env value is already in the command.
        assert!(
            form.run_spec().args.contains(&"--org=myorg".to_string()),
            "env value is pre-filled and applied: {:?}",
            form.run_spec().args
        );
    }

    #[test]
    fn env_var_takes_precedence_over_a_remembered_value() {
        let node = leaf(vec![value_flag("org")], vec![]);
        let env = fake_env(&[("BRACT_T__ORG", "envorg")]);
        let mut map = HashMap::new();
        map.insert("--org".to_string(), (5u64, Some("oldorg".to_string())));
        let form = env_form(&node, env, Arc::new(FakeMemory(map)));
        // The remembered value would only be a ghost (needs →); the env value is
        // applied outright.
        assert!(
            form.run_spec().args.contains(&"--org=envorg".to_string()),
            "env wins over remembered: {:?}",
            form.run_spec().args
        );
    }

    #[test]
    fn env_sourced_field_is_never_persisted_to_memory() {
        let node = leaf(vec![value_flag("org"), value_flag("name")], vec![]);
        let env = fake_env(&[("BRACT_T__ORG", "myorg")]);
        let mem = Arc::new(RecordingMemory(Mutex::new(Vec::new())));
        let mut form = env_form(&node, env, mem.clone());
        // Fill a non-env field by hand, then run.
        assert!(form.set_field("--name", "web"));
        form.handle_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL));

        let recorded = mem.0.lock().unwrap();
        assert!(recorded.contains(&"--name".to_string()), "hand-typed field is remembered");
        assert!(
            !recorded.contains(&"--org".to_string()),
            "env-provided value must not be written to disk: {recorded:?}"
        );
    }

    #[test]
    fn truthy_env_var_enables_a_bool_flag() {
        let node = leaf(vec![bool_flag("verbose")], vec![]);
        let env = fake_env(&[("BRACT_T__VERBOSE", "true")]);
        let form = env_form(&node, env, Arc::new(FakeMemory(HashMap::new())));
        assert!(
            form.run_spec().args.contains(&"--verbose".to_string()),
            "a truthy env var enables the toggle: {:?}",
            form.run_spec().args
        );
    }

    #[test]
    fn falsy_env_var_leaves_a_bool_flag_off() {
        let node = leaf(vec![bool_flag("verbose")], vec![]);
        let env = fake_env(&[("BRACT_T__VERBOSE", "false")]);
        let form = env_form(&node, env, Arc::new(FakeMemory(HashMap::new())));
        assert!(
            !form.run_spec().args.contains(&"--verbose".to_string()),
            "a falsy env var keeps the toggle off: {:?}",
            form.run_spec().args
        );
    }

    #[test]
    fn env_sourced_bool_flag_is_not_recorded() {
        let node = leaf(vec![bool_flag("verbose")], vec![]);
        let env = fake_env(&[("BRACT_T__VERBOSE", "true")]);
        let mem = Arc::new(RecordingMemory(Mutex::new(Vec::new())));
        let mut form = env_form(&node, env, mem.clone());
        form.handle_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL));
        assert!(
            !mem.0.lock().unwrap().contains(&"--verbose".to_string()),
            "an env-enabled toggle must not be persisted"
        );
    }
}
