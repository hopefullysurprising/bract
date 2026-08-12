//! Miller-columns navigation — the file-explorer "column view" applied to a CLI
//! command tree. The selected item in each column reveals its children in the
//! column to the right; descending slides the focus rightwards, ascending slides
//! back. Children load lazily on a background thread, and the focused item is
//! pre-loaded one level deep so its expandability — and therefore the right
//! controls — are known before you act on it.

use std::collections::HashMap;
use std::sync::Arc;

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, ListState, Padding, Paragraph, Wrap};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};

use super::form::FormView;
use super::{View, ViewAction};
use crate::data::env_params::{EnvSource, NullEnv, SystemEnv};
use crate::data::loader::{BackgroundLoader, LoadRequest, Loader, Priority};
use crate::data::memory::{default_form_memory, FormMemory, NullFormMemory};
use crate::data::node::{Children, Node, NodeKind};
use crate::data::source::{Loaded, Source};

const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const MAX_VISIBLE_COLUMNS: usize = 3;

struct ToolMeta {
    bin: Vec<String>,
    separator: String,
}

enum ColumnStyle {
    Trail,
    Active,
    Preview,
}

pub struct MillerView {
    roots: Vec<Node>,
    /// Selection index at each visited depth; `path[0]` indexes `roots`.
    path: Vec<usize>,
    tool_meta: HashMap<String, ToolMeta>,
    loader: Box<dyn Loader>,
    /// In-flight loads and the priority they were requested at, so a focus (High)
    /// can preempt an earlier speculative peek (Low) for the same node.
    pending: HashMap<String, Priority>,
    spinner: usize,
    error: Option<String>,
    /// Type-ahead filter on the active column (`None` = not filtering).
    filter: Option<String>,
    /// Remembers form fills for frequency-sorting and value recall.
    memory: Arc<dyn FormMemory>,
    /// Supplies env-var values that pre-fill matching form fields.
    env: Arc<dyn EnvSource>,
}

impl MillerView {
    pub fn new(sources: Vec<Box<dyn Source>>) -> Self {
        Self::with_loader_and_memory(
            sources,
            |s| Box::new(BackgroundLoader::new(s)),
            default_form_memory(),
            Arc::new(SystemEnv),
        )
    }

    /// Test constructor: a custom loader, no form memory, and an empty
    /// environment (so tests never touch the real db or process env).
    pub fn with_loader(
        sources: Vec<Box<dyn Source>>,
        make_loader: impl FnOnce(Vec<Box<dyn Source>>) -> Box<dyn Loader>,
    ) -> Self {
        Self::with_loader_and_memory(sources, make_loader, Arc::new(NullFormMemory), Arc::new(NullEnv))
    }

    pub fn with_loader_and_memory(
        sources: Vec<Box<dyn Source>>,
        make_loader: impl FnOnce(Vec<Box<dyn Source>>) -> Box<dyn Loader>,
        memory: Arc<dyn FormMemory>,
        env: Arc<dyn EnvSource>,
    ) -> Self {
        let mut roots = Vec::new();
        let mut tool_meta = HashMap::new();
        for source in &sources {
            tool_meta.insert(
                source.tool_id().to_string(),
                ToolMeta { bin: source.tool_bin(), separator: source.tool_path_separator().to_string() },
            );
            roots.push(Node {
                id: source.tool_id().to_string(),
                name: source.tool_name().to_string(),
                description: String::new(),
                // Nothing is known about a tool until its help is read — whether it
                // has subcommands at all, or is runnable on its own. Claiming
                // `Branch` here shows an expand arrow that flips to a leaf marker
                // the moment the data lands; `Unknown` says so honestly and still
                // descends (which triggers the load).
                kind: NodeKind::Unknown,
                runnable: false,
                flags: vec![],
                args: vec![],
                tool_id: source.tool_id().to_string(),
                command_path: vec![],
                children: Children::Unloaded,
            });
        }

        // The Mise kernel is pinned to the top — Tasks, then Mise's own CLI — and
        // the rest of the toolchain follows alphabetically (a separator divides
        // the kernel from the tools).
        roots.sort_by(|a, b| {
            let rank = |n: &Node| match n.tool_id.as_str() {
                "mise_tasks" => 0u8,
                "mise" => 1,
                _ => 2,
            };
            rank(a).cmp(&rank(b)).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });

        let loader = make_loader(sources);
        let path = if roots.is_empty() { vec![] } else { vec![0] };

        let mut view = Self {
            roots,
            path,
            tool_meta,
            loader,
            pending: HashMap::new(),
            spinner: 0,
            error: None,
            filter: None,
            memory,
            env,
        };
        view.ensure_focused_loading();
        view
    }

    // --- tree navigation by path indices -----------------------------------

    fn active_depth(&self) -> usize {
        self.path.len().saturating_sub(1)
    }

    /// Nodes shown in the column at `depth` (children of the node selected at
    /// `depth - 1`), or empty if that node's children aren't loaded yet.
    fn column_nodes(&self, depth: usize) -> &[Node] {
        let mut nodes = self.roots.as_slice();
        for d in 0..depth {
            let idx = self.path.get(d).copied().unwrap_or(0);
            match nodes.get(idx).map(|n| &n.children) {
                Some(Children::Loaded(children)) => nodes = children,
                _ => return &[],
            }
        }
        nodes
    }

    fn focused(&self) -> Option<&Node> {
        let depth = self.active_depth();
        let idx = *self.path.get(depth)?;
        self.column_nodes(depth).get(idx)
    }

    fn find_mut<'a>(nodes: &'a mut [Node], id: &str) -> Option<&'a mut Node> {
        for node in nodes {
            if node.id == id {
                return Some(node);
            }
            if let Children::Loaded(children) = &mut node.children
                && let Some(found) = Self::find_mut(children, id) {
                    return Some(found);
                }
        }
        None
    }

    // --- lazy loading ------------------------------------------------------

    fn request_load(&mut self, id: String, tool_id: String, command_path: Vec<String>, priority: Priority, show_spinner: bool) {
        // Skip only if an equal-or-higher-priority load for this node is already in
        // flight; a High request still goes through when a Low peek is pending, so
        // the loader can promote it ahead of the peek backlog.
        match self.pending.get(&id) {
            Some(Priority::High) => return,
            Some(Priority::Low) if priority == Priority::Low => return,
            _ => {}
        }
        let req = LoadRequest { node_id: id.clone(), tool_id, command_path, priority };

        // Cache hit on a focus load: resolve inline. A cached load is ~85µs —
        // far below a frame — so routing it through the background thread (and
        // flashing a spinner for a poll tick) would only add latency. Peeks
        // (Low) stay async so a wide column can't stall the main thread.
        if priority == Priority::High
            && let Some(result) = self.loader.load_cached(&req)
        {
            self.apply_outcome(&id, result);
            return;
        }

        self.pending.insert(id.clone(), priority);
        if show_spinner
            && let Some(node) = Self::find_mut(&mut self.roots, &id) {
                node.children = Children::Loading;
            }
        self.loader.request(req);
    }

    fn ensure_focused_loading(&mut self) {
        let Some(focused) = self.focused() else { return };
        if !matches!(focused.children, Children::Unloaded) {
            return;
        }
        let (id, tool_id, command_path) =
            (focused.id.clone(), focused.tool_id.clone(), focused.command_path.clone());
        self.request_load(id, tool_id, command_path, Priority::High, true);
    }

    /// Peek one level deeper than rendered, in the background (low priority), for
    /// every visible item that still hides information: `Unknown` items (to learn
    /// branch-vs-leaf, e.g. Cobra) and items missing a description (e.g. the root
    /// tools, whose `--help` headline we want to show). Knack items already carry
    /// both, so they are never peeked.
    fn peek_visible(&mut self) {
        let active = self.active_depth();
        let mut targets: Vec<(String, String, Vec<String>)> = Vec::new();
        for depth in active..=active + 1 {
            for node in self.column_nodes(depth) {
                let wants_peek =
                    matches!(node.kind, NodeKind::Unknown) || node.description.is_empty();
                if wants_peek
                    && matches!(node.children, Children::Unloaded)
                    && !self.pending.contains_key(&node.id)
                {
                    targets.push((node.id.clone(), node.tool_id.clone(), node.command_path.clone()));
                }
            }
        }
        for (id, tool_id, command_path) in targets {
            self.request_load(id, tool_id, command_path, Priority::Low, false);
        }
    }

    fn integrate_results(&mut self) {
        for outcome in self.loader.poll() {
            self.apply_outcome(&outcome.node_id, outcome.loaded);
        }
    }

    /// Fold one load result into the tree. Shared by the async poll path and the
    /// synchronous cache-hit path so both produce an identical node state.
    fn apply_outcome(&mut self, node_id: &str, loaded: Result<Loaded, String>) {
        self.pending.remove(node_id);
        let Some(node) = Self::find_mut(&mut self.roots, node_id) else { return };
        match loaded {
            Ok(loaded) => {
                if !loaded.description.is_empty() {
                    node.description = loaded.description;
                }
                node.flags = loaded.flags;
                node.args = loaded.args;
                // A node with children is a branch; without, a terminal leaf. This
                // applies to a tool's own root too: a CLI that takes only flags
                // (gomplate) is a leaf, and presenting it as a branch gives it an
                // arrow that expands nothing and no route to its run form.
                node.kind = if loaded.children.is_empty() {
                    NodeKind::Leaf
                } else {
                    NodeKind::Branch
                };
                // Now that we've parsed the node's own help, we know whether it is
                // directly runnable (a leaf or a dual command) or a pure group that
                // only dispatches to subcommands.
                node.runnable = loaded.runnable;
                node.children = Children::Loaded(loaded.children);
            }
            Err(e) => {
                self.error = Some(e);
                node.children = Children::Loaded(vec![]);
                if matches!(node.kind, NodeKind::Unknown) {
                    node.kind = NodeKind::Leaf;
                }
            }
        }
    }

    // --- key actions -------------------------------------------------------

    // Loading is intentionally *not* triggered here — it happens on the next idle
    // tick (see `on_idle`), which debounces rapid scrolling so holding ↓ through a
    // long list doesn't enqueue a `--help` fetch for every item passed over.
    fn move_selection(&mut self, delta: isize) {
        let depth = self.active_depth();
        let visible = self.filtered_indices();
        if visible.is_empty() {
            return;
        }
        let cur = self.path[depth];
        let pos = visible.iter().position(|&i| i == cur).unwrap_or(0) as isize;
        let next = (pos + delta).rem_euclid(visible.len() as isize) as usize;
        self.path[depth] = visible[next];
    }

    /// Whether the current selection is actually visible — false only when a
    /// filter is active and matches nothing (the selected index then points at a
    /// hidden row, so acting on it would run/descend a command the user can't see).
    fn selection_visible(&self) -> bool {
        self.filter.is_none()
            || self
                .path
                .get(self.active_depth())
                .is_some_and(|&i| self.filtered_indices().contains(&i))
    }

    fn descend(&mut self) {
        if !self.selection_visible() {
            return;
        }
        self.clear_filter();
        let Some(focused) = self.focused() else { return };
        if !focused.is_expandable() {
            return;
        }
        match &focused.children {
            Children::Loaded(children) if !children.is_empty() => {
                self.path.push(0);
                self.ensure_focused_loading();
            }
            Children::Unloaded => self.ensure_focused_loading(),
            _ => {}
        }
    }

    fn ascend(&mut self) {
        self.clear_filter();
        if self.path.len() > 1 {
            self.path.pop();
        }
    }

    // --- filtering ---------------------------------------------------------

    fn matches_filter(&self, node: &Node) -> bool {
        match &self.filter {
            None => true,
            Some(q) if q.is_empty() => true,
            Some(q) => {
                let q = q.to_lowercase();
                // Match the name or the description: short command names (`aks`)
                // are often only findable via their description ("Kubernetes").
                node.name.to_lowercase().contains(&q)
                    || node.description.to_lowercase().contains(&q)
            }
        }
    }

    /// Indices into the active column that pass the current filter (all of them
    /// when not filtering).
    fn filtered_indices(&self) -> Vec<usize> {
        let depth = self.active_depth();
        self.column_nodes(depth)
            .iter()
            .enumerate()
            .filter(|(_, n)| self.matches_filter(n))
            .map(|(i, _)| i)
            .collect()
    }

    fn clear_filter(&mut self) {
        self.filter = None;
    }

    fn start_filter(&mut self) {
        self.filter = Some(String::new());
    }

    /// Keep the selection on a matching row as the query changes.
    fn snap_to_filter(&mut self) {
        let depth = self.active_depth();
        let visible = self.filtered_indices();
        if !visible.contains(&self.path[depth])
            && let Some(&first) = visible.first() {
                self.path[depth] = first;
            }
    }

    fn filter_input(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.clear_filter(),
            KeyCode::Backspace => {
                if let Some(q) = self.filter.as_mut()
                    && q.pop().is_none() {
                        self.clear_filter();
                    }
                self.snap_to_filter();
            }
            KeyCode::Char(c) => {
                if let Some(q) = self.filter.as_mut() {
                    q.push(c);
                }
                self.snap_to_filter();
            }
            _ => {}
        }
    }

    /// The command chain from the tool root down to the focused node, so the form
    /// can group flags per level (the root carries the tool's own flags, e.g.
    /// `mani`'s `--config`).
    fn ancestor_chain(&self) -> Vec<&Node> {
        (0..=self.active_depth())
            .filter_map(|d| self.column_nodes(d).get(self.path[d]))
            .collect()
    }

    fn run_focused(&self) -> Option<ViewAction> {
        if !self.selection_visible() {
            return None;
        }
        let focused = self.focused()?;
        // Only run once the node's own help has loaded: before that we don't have
        // its flags, and a Cobra node's runnability isn't known yet (it defaults to
        // runnable until proven a pure group). Otherwise the form would open empty
        // or for a command that can't actually be run.
        if !matches!(focused.children, Children::Loaded(_)) || !focused.runnable {
            return None;
        }
        let meta = self.tool_meta.get(&focused.tool_id)?;
        let ancestors = self.ancestor_chain();
        Some(ViewAction::Push(Box::new(FormView::new(
            &ancestors,
            &meta.bin,
            &meta.separator,
            &focused.tool_id,
            self.env.as_ref(),
            self.memory.clone(),
        ))))
    }

    /// Enter is the smart "primary action": descend a branch, run a leaf, or
    /// trigger a load for something not yet resolved.
    fn activate(&mut self) -> Option<ViewAction> {
        if !self.selection_visible() {
            return None;
        }
        enum Act {
            Descend,
            Load,
            Run,
        }
        let act = match &self.focused()?.children {
            Children::Loaded(children) if !children.is_empty() => Act::Descend,
            Children::Loaded(_) => Act::Run,
            Children::Unloaded | Children::Loading => Act::Load,
        };
        match act {
            Act::Descend => {
                self.descend();
                None
            }
            Act::Load => {
                self.ensure_focused_loading();
                None
            }
            Act::Run => self.run_focused(),
        }
    }

    // --- helpers for tests -------------------------------------------------

    /// Drive loading to completion (used with a synchronous loader in tests).
    fn settle(&mut self) {
        for _ in 0..64 {
            if self.pending.is_empty() {
                break;
            }
            self.integrate_results();
        }
    }

    /// Navigate to a command by display-name path (e.g. `["az", "account", "list"]`),
    /// loading each level as needed. Returns false if any segment isn't found.
    pub fn select_path(&mut self, names: &[&str]) -> bool {
        let Some(first) = names.first() else { return false };
        let Some(root_idx) = self.roots.iter().position(|n| &n.name == first) else {
            return false;
        };
        self.path = vec![root_idx];
        self.ensure_focused_loading();
        self.settle();

        for name in &names[1..] {
            let depth = self.path.len();
            let Some(idx) = self.column_nodes(depth).iter().position(|n| &n.name == name) else {
                return false;
            };
            self.path.push(idx);
            self.ensure_focused_loading();
            self.settle();
        }
        true
    }

    pub fn focused_command(&self) -> Option<&str> {
        self.focused().map(|n| n.name.as_str())
    }

    pub fn root_names(&self) -> Vec<String> {
        self.roots.iter().map(|n| n.name.clone()).collect()
    }

    pub fn pending_loads(&self) -> usize {
        self.pending.len()
    }

    pub fn focused_loaded(&self) -> bool {
        self.focused().is_some_and(|n| matches!(n.children, Children::Loaded(_)))
    }

    pub fn focused_description(&self) -> Option<String> {
        self.focused().map(|n| n.description.clone())
    }

    pub fn depth(&self) -> usize {
        self.active_depth()
    }

    pub fn focused_runnable(&self) -> bool {
        self.focused().is_some_and(|n| n.runnable)
    }

    pub fn focused_expandable(&self) -> bool {
        self.focused().is_some_and(|n| n.is_expandable())
    }

    /// Run an idle tick to completion (focused load + background peeks), for tests
    /// driving the synchronous loader.
    pub fn pump(&mut self) {
        self.ensure_focused_loading();
        self.peek_visible();
        self.settle();
    }

    /// How many items in the visible columns still have unknown expandability.
    /// Drops to zero once peeks resolve — the basis of the "no grey dots" promise.
    pub fn unresolved_visible(&self) -> usize {
        let active = self.active_depth();
        (active..=active + 1)
            .flat_map(|d| self.column_nodes(d))
            .filter(|n| matches!(n.kind, NodeKind::Unknown))
            .count()
    }

    // --- rendering ---------------------------------------------------------

    fn breadcrumb(&self) -> Line<'static> {
        let mut spans = vec![Span::styled(
            " ",
            Style::default(),
        )];
        for depth in 0..self.path.len() {
            if let Some(node) = self.column_nodes(depth).get(self.path[depth]) {
                if depth > 0 {
                    spans.push(Span::styled("  ›  ", Style::new().fg(Color::DarkGray)));
                }
                let style = if depth == self.active_depth() {
                    Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD)
                } else {
                    Style::new().fg(Color::Gray)
                };
                spans.push(Span::styled(node.name.clone(), style));
            }
        }
        Line::from(spans)
    }

    fn footer(&self) -> Line<'static> {
        let key = Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD);
        let label = Style::new().fg(Color::DarkGray);
        let sep = || Span::styled("   ", label);
        let mut spans = vec![Span::raw(" ")];

        let focused = self.focused();
        let expandable = focused.is_some_and(|f| f.is_expandable());
        let runnable = focused.is_some_and(|f| f.runnable);

        if expandable {
            spans.push(Span::styled("→/↵", key));
            spans.push(Span::styled(" open", label));
            if runnable {
                spans.push(sep());
                spans.push(Span::styled("r", key));
                spans.push(Span::styled(" run", label));
            }
        } else if runnable {
            spans.push(Span::styled("↵", key));
            spans.push(Span::styled(" run", label));
        }
        spans.push(sep());
        spans.push(Span::styled("←", key));
        spans.push(Span::styled(" back", label));
        spans.push(sep());

        if self.filter.is_some() {
            spans.push(Span::styled("esc", key));
            spans.push(Span::styled(" clear filter", label));
        } else {
            spans.push(Span::styled("/", key));
            spans.push(Span::styled(" search", label));
            spans.push(sep());
            spans.push(Span::styled("q", key));
            spans.push(Span::styled(" quit", label));
        }

        if let Some(error) = &self.error {
            let trimmed: String = error.lines().next().unwrap_or_default().chars().take(60).collect();
            spans.push(sep());
            spans.push(Span::styled(format!("⚠ {trimmed}"), Style::new().fg(Color::Red)));
        }

        Line::from(spans)
    }

    fn render_columns(&self, frame: &mut Frame, area: Rect) {
        let active = self.active_depth();
        let lo = active.saturating_sub(MAX_VISIBLE_COLUMNS - 2);
        // Visible depths: a little context to the left, the active column, and
        // the preview column to the right (active + 1).
        let depths: Vec<usize> = (lo..=active + 1).collect();
        let constraints: Vec<Constraint> =
            depths.iter().map(|_| Constraint::Ratio(1, depths.len() as u32)).collect();
        let chunks = Layout::horizontal(constraints).split(area);

        for (chunk, &depth) in chunks.iter().zip(&depths) {
            if depth == active + 1 {
                self.render_preview(frame, *chunk);
            } else if depth == active && self.filter.is_some() {
                let visible = self.filtered_indices();
                let nodes = self.column_nodes(depth);
                let refs: Vec<&Node> = visible.iter().map(|&i| &nodes[i]).collect();
                let selected = visible.iter().position(|&i| i == self.path[depth]);
                let title = format!("/{}", self.filter.as_deref().unwrap_or(""));
                render_list(frame, *chunk, &title, &refs, selected, ColumnStyle::Active, None);
            } else {
                let style = if depth == active { ColumnStyle::Active } else { ColumnStyle::Trail };
                let title = self.column_title(depth);
                let refs: Vec<&Node> = self.column_nodes(depth).iter().collect();
                let selected = self.path.get(depth).copied();
                let separator = if depth == 0 { self.root_separator() } else { None };
                render_list(frame, *chunk, &title, &refs, selected, style, separator);
            }
        }
    }

    /// Draw a separator after the pinned Mise kernel (Tasks + Mise) in the root
    /// column, when there are tools below it.
    fn root_separator(&self) -> Option<usize> {
        let pinned = self
            .roots
            .iter()
            .take_while(|n| matches!(n.tool_id.as_str(), "mise_tasks" | "mise"))
            .count();
        (pinned > 0 && self.roots.len() > pinned).then(|| pinned - 1)
    }

    /// Full-width description of the focused node — the navigation helper that
    /// makes a bare command name meaningful, especially for branch nodes whose
    /// preview column shows children rather than their own description.
    fn render_detail(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::TOP)
            .border_style(Style::new().fg(Color::Rgb(60, 64, 72)))
            .padding(Padding::new(1, 1, 0, 0));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let Some(focused) = self.focused() else { return };
        if focused.description.is_empty() {
            return;
        }
        frame.render_widget(
            Paragraph::new(focused.description.clone())
                .style(Style::new().fg(Color::Gray).add_modifier(Modifier::ITALIC))
                .wrap(Wrap { trim: true }),
            inner,
        );
    }

    fn column_title(&self, depth: usize) -> String {
        if depth == 0 {
            "Tools".to_string()
        } else {
            self.column_nodes(depth - 1)
                .get(self.path[depth - 1])
                .map(|n| n.name.clone())
                .unwrap_or_default()
        }
    }

    /// The rightmost slot: children of the focused node, or — when the focus is
    /// a leaf or still loading — a detail card / spinner.
    fn render_preview(&self, frame: &mut Frame, area: Rect) {
        let depth = self.active_depth() + 1;
        let nodes = self.column_nodes(depth);
        if !nodes.is_empty() {
            let title = self.column_title(depth);
            let refs: Vec<&Node> = nodes.iter().collect();

            // A node that is both runnable and a parent (e.g. `mani edit`, or a
            // mise task `app:check`) gets the full run preview — its description
            // and parameters grouped by command level, exactly as the run form and
            // leaf card present them — above a separator, then its subtree below.
            if let Some(focused) = self.focused().filter(|f| f.runnable && f.is_expandable()) {
                self.render_runnable_branch_preview(frame, area, focused, &title, &refs);
                return;
            }

            render_list(frame, area, &title, &refs, None, ColumnStyle::Preview, None);
            return;
        }

        let Some(focused) = self.focused() else { return };
        let block = card_block(&focused.name);
        let inner = block.inner(area);
        frame.render_widget(block, area);

        // Anything not yet loaded (Loading, or Unloaded because only a background
        // peek was requested) shows the spinner rather than a misleading empty
        // "No parameters" card — the focused node is always loaded on the next tick.
        if !matches!(focused.children, Children::Loaded(_)) {
            let spinner = SPINNER[self.spinner % SPINNER.len()];
            let line = Line::from(vec![
                Span::styled(format!("{spinner} "), Style::new().fg(Color::Cyan)),
                Span::styled("loading…", Style::new().fg(Color::DarkGray)),
            ]);
            frame.render_widget(Paragraph::new(line), inner);
            return;
        }

        // Show the full, grouped parameter set the run form will present — pulled
        // from the whole ancestor chain, not just the leaf.
        let ancestors = self.ancestor_chain();
        let levels = crate::ui::form::param_levels(&ancestors);
        frame.render_widget(
            Paragraph::new(detail_lines(focused, &levels)).wrap(Wrap { trim: true }),
            inner,
        );
    }

    /// Render a dual node's child column: a run preview (run affordance +
    /// description + grouped params) up top, a separator, then the subtree.
    fn render_runnable_branch_preview(
        &self,
        frame: &mut Frame,
        area: Rect,
        focused: &Node,
        title: &str,
        refs: &[&Node],
    ) {
        let block = Block::default()
            .borders(Borders::RIGHT)
            .border_style(Style::new().fg(Color::DarkGray))
            .padding(Padding::new(1, 1, 0, 0))
            .title(Span::styled(
                format!(" {title} "),
                Style::new().fg(Color::DarkGray).add_modifier(Modifier::BOLD),
            ));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let ancestors = self.ancestor_chain();
        let levels = crate::ui::form::param_levels(&ancestors);
        let mut preview = vec![run_affordance(&focused.name), Line::raw("")];
        preview.extend(param_section_lines(focused, &levels));

        // Give the preview the height it needs, capped so the subtree keeps room.
        let cap = (inner.height * 3 / 5).max(4);
        let preview_h = (preview.len() as u16).min(cap);
        let [preview_area, sep_area, list_area] = Layout::vertical([
            Constraint::Length(preview_h),
            Constraint::Length(1),
            Constraint::Fill(1),
        ])
        .areas(inner);

        frame.render_widget(Paragraph::new(preview).wrap(Wrap { trim: true }), preview_area);
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "─".repeat(inner.width as usize),
                Style::new().fg(Color::Rgb(60, 64, 72)),
            ))),
            sep_area,
        );
        let items: Vec<ListItem> = refs.iter().map(|n| node_list_item(n, true)).collect();
        frame.render_widget(List::new(items), list_area);
    }
}

impl View for MillerView {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn on_idle(&mut self) {
        self.spinner = self.spinner.wrapping_add(1);
        self.integrate_results();
        // Idle ticks only fire once input pauses, which is exactly the debounce we
        // want: load the settled focus first, then peek the rest of the column.
        self.ensure_focused_loading();
        self.peek_visible();
    }

    fn render(&mut self, frame: &mut Frame) {
        self.integrate_results();

        let [crumb_area, body_area, detail_area, footer_area] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Fill(1),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .areas(frame.area());

        frame.render_widget(Paragraph::new(self.breadcrumb()), crumb_area);
        self.render_columns(frame, body_area);
        self.render_detail(frame, detail_area);
        frame.render_widget(Paragraph::new(self.footer()), footer_area);
    }

    fn handle_key(&mut self, key: KeyEvent) -> Option<ViewAction> {
        // While filtering, only the arrows/Enter navigate; every other key edits
        // the query (so typing `q`/`r`/`j` filters rather than quits/runs/moves).
        if self.filter.is_some() {
            return match key.code {
                KeyCode::Up => {
                    self.move_selection(-1);
                    Some(ViewAction::Consumed)
                }
                KeyCode::Down => {
                    self.move_selection(1);
                    Some(ViewAction::Consumed)
                }
                KeyCode::Left => {
                    self.ascend();
                    Some(ViewAction::Consumed)
                }
                KeyCode::Right => {
                    self.descend();
                    Some(ViewAction::Consumed)
                }
                KeyCode::Enter => self.activate().or(Some(ViewAction::Consumed)),
                _ => {
                    self.filter_input(key);
                    Some(ViewAction::Consumed)
                }
            };
        }

        match key.code {
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_selection(1);
                Some(ViewAction::Consumed)
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_selection(-1);
                Some(ViewAction::Consumed)
            }
            KeyCode::Right | KeyCode::Char('l') => {
                self.descend();
                Some(ViewAction::Consumed)
            }
            KeyCode::Left | KeyCode::Char('h') => {
                self.ascend();
                Some(ViewAction::Consumed)
            }
            KeyCode::Enter => self.activate().or(Some(ViewAction::Consumed)),
            KeyCode::Char('r') => {
                // `run_focused` only fires once the node's help has loaded; if it
                // hasn't, kick off the load so a follow-up press can run it.
                self.run_focused().or_else(|| {
                    self.ensure_focused_loading();
                    Some(ViewAction::Consumed)
                })
            }
            KeyCode::Char('/') => {
                self.start_filter();
                Some(ViewAction::Consumed)
            }
            _ => None,
        }
    }
}

fn icon(kind: NodeKind, runnable: bool) -> Span<'static> {
    match kind {
        // A branch that is also runnable (a command *and* a parent) gets its own
        // glyph so it stands out from a plain namespace among its siblings.
        NodeKind::Branch if runnable => Span::styled("◆", Style::new().fg(Color::Green)),
        NodeKind::Branch => Span::styled("▸", Style::new().fg(Color::Cyan)),
        NodeKind::Leaf if runnable => Span::styled("•", Style::new().fg(Color::Green)),
        // A leaf with nothing to run is a dead end — most often a tool whose help
        // could not be read. Green would promise a run form that never opens.
        NodeKind::Leaf => Span::styled("•", Style::new().fg(Color::DarkGray)),
        NodeKind::Unknown => Span::styled("·", Style::new().fg(Color::DarkGray)),
    }
}

fn render_list(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    nodes: &[&Node],
    selected: Option<usize>,
    style: ColumnStyle,
    separator_after: Option<usize>,
) {
    let dim = matches!(style, ColumnStyle::Trail | ColumnStyle::Preview);
    // The icon (▸ branch, ◆ runnable branch, • leaf, · unknown) signals each row's
    // nature; the row also carries a dim description for context (clipped to width).
    let mut items: Vec<ListItem> = Vec::with_capacity(nodes.len() + 1);
    for (i, node) in nodes.iter().enumerate() {
        items.push(node_list_item(node, dim));
        if separator_after == Some(i) {
            let rule = "─".repeat(area.width.saturating_sub(2) as usize);
            items.push(ListItem::new(Line::from(Span::styled(rule, Style::new().fg(Color::Rgb(60, 64, 72))))));
        }
    }
    // The separator is render-only; shift the highlight past it for later rows.
    let selected = selected.map(|s| match separator_after {
        Some(after) if s > after => s + 1,
        _ => s,
    });

    let highlight = match style {
        ColumnStyle::Active => Style::new().bg(Color::Rgb(40, 44, 52)).add_modifier(Modifier::BOLD),
        ColumnStyle::Trail => Style::new().fg(Color::Cyan),
        ColumnStyle::Preview => Style::default(),
    };

    let border_color = if matches!(style, ColumnStyle::Active) { Color::Cyan } else { Color::DarkGray };
    let block = Block::default()
        .borders(Borders::RIGHT)
        .border_style(Style::new().fg(border_color))
        .padding(Padding::new(1, 1, 0, 0))
        .title(Span::styled(
            format!(" {title} "),
            Style::new().fg(border_color).add_modifier(Modifier::BOLD),
        ));

    let list = List::new(items).block(block).highlight_style(highlight);
    let mut state = ListState::default();
    state.select(selected);
    frame.render_stateful_widget(list, area, &mut state);
}

fn card_block(title: &str) -> Block<'static> {
    Block::default()
        .borders(Borders::LEFT)
        .border_type(BorderType::Plain)
        .border_style(Style::new().fg(Color::DarkGray))
        .padding(Padding::new(2, 1, 0, 0))
        .title(Span::styled(
            format!(" {title} "),
            Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ))
}

/// The command's description plus its parameters grouped by command level (the
/// leaf's own params first, then each parent level under its name) — the shared
/// body of both the leaf detail card and the dual-node run preview.
fn param_section_lines(focused: &Node, levels: &[(String, &Node)]) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if !focused.description.is_empty() {
        lines.push(Line::from(Span::styled(
            focused.description.clone(),
            Style::new().fg(Color::Gray).add_modifier(Modifier::ITALIC),
        )));
        lines.push(Line::raw(""));
    }

    if levels.is_empty() {
        lines.push(Line::from(Span::styled("No parameters", Style::new().fg(Color::DarkGray))));
    } else {
        for (label, node) in levels {
            let header = if label.is_empty() { "Parameters".to_string() } else { label.clone() };
            lines.push(Line::from(Span::styled(
                header,
                Style::new().fg(Color::DarkGray).add_modifier(Modifier::BOLD),
            )));
            for arg in &node.args {
                lines.push(param_line(&arg.name, arg.required));
            }
            for flag in &node.flags {
                lines.push(param_line(&flag.name, flag.required));
            }
            lines.push(Line::raw(""));
        }
    }
    lines
}

/// The leaf detail card: the parameter section plus an Enter-to-run hint (a leaf
/// is run by Enter, since it has no children to descend into).
fn detail_lines(focused: &Node, levels: &[(String, &Node)]) -> Vec<Line<'static>> {
    let mut lines = param_section_lines(focused, levels);
    if focused.runnable {
        lines.push(Line::from(vec![
            Span::styled("↵", Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::styled(" configure & run", Style::new().fg(Color::DarkGray)),
        ]));
    }
    lines
}

/// The prominent run affordance leading a dual node's preview. A dual node is
/// descended into with Enter, so its run action is bound to `r`.
fn run_affordance(name: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled("▶ run ", Style::new().fg(Color::Green).add_modifier(Modifier::BOLD)),
        Span::styled(name.to_string(), Style::new().fg(Color::Green).add_modifier(Modifier::BOLD)),
        Span::styled("   (r)", Style::new().fg(Color::DarkGray)),
    ])
}

fn node_list_item(node: &Node, dim: bool) -> ListItem<'static> {
    let name_style = if dim { Style::new().fg(Color::Gray) } else { Style::new().fg(Color::White) };
    let mut spans = vec![icon(node.kind, node.runnable), Span::raw(" "), Span::styled(node.name.clone(), name_style)];
    if !node.description.is_empty() {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(node.description.clone(), Style::new().fg(Color::DarkGray)));
    }
    ListItem::new(Line::from(spans))
}

fn param_line(name: &str, required: bool) -> Line<'static> {
    let mut spans = vec![
        Span::styled("  • ", Style::new().fg(Color::DarkGray)),
        Span::styled(name.to_string(), Style::new().fg(Color::White)),
    ];
    if required {
        spans.push(Span::styled("  required", Style::new().fg(Color::Yellow)));
    }
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::source::help_cache::CachingHelpProvider;
    use crate::data::source::mise_tools::HelpToolSource;
    use crate::data::source::HelpProvider;
    use helptext_parser::InputFormat;
    use std::path::PathBuf;

    /// Stands in for the slow `mise exec -- tool --help`; returns fixed help so
    /// the only variable under test is cached vs. uncached, never a subprocess.
    struct StaticProvider(String);
    impl HelpProvider for StaticProvider {
        fn fetch_help(&self, _b: &str, _p: &[&str]) -> Result<String, Box<dyn std::error::Error>> {
            Ok(self.0.clone())
        }
    }

    fn mani_root_help() -> String {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/cli-help/mani_0.32.0_root.txt");
        std::fs::read_to_string(path).unwrap()
    }

    /// A MillerView backed by the real (threaded) background loader, so the
    /// synchronous cache path is genuinely distinguishable from the async one.
    fn view_with(src: Box<dyn Source>) -> MillerView {
        MillerView::with_loader(vec![src], |s| Box::new(BackgroundLoader::new(s)))
    }

    #[test]
    fn cached_focus_load_resolves_synchronously_without_a_spinner() {
        let tmp = tempfile::tempdir().unwrap();
        let provider = CachingHelpProvider::new(
            Box::new(StaticProvider(mani_root_help())),
            tmp.path().to_path_buf(),
            "v".into(),
        );
        let src = HelpToolSource::new("mani".into(), InputFormat::CobraHelptext, Box::new(provider));
        src.load(&[]).unwrap(); // prime the root cache file

        // new() runs ensure_focused_loading on the root. A cache hit is resolved
        // inline: nothing is dispatched to the loader and the node is already
        // Loaded — so no spinner ever shows.
        let view = view_with(Box::new(src));
        assert_eq!(view.pending_loads(), 0, "a cache hit must not touch the background loader");
        assert!(view.focused_loaded(), "the cached root loads synchronously, never Loading");
    }

    #[test]
    fn uncached_focus_load_goes_async_and_shows_a_spinner() {
        let src = HelpToolSource::new(
            "mani".into(),
            InputFormat::CobraHelptext,
            Box::new(StaticProvider(mani_root_help())),
        );

        // No cache (is_cached defaults false): the load is dispatched to the
        // background thread and the node is marked Loading until a later poll.
        let view = view_with(Box::new(src));
        assert_eq!(view.pending_loads(), 1, "a miss is dispatched to the background loader");
        assert!(!view.focused_loaded(), "the node shows a Loading spinner, not Loaded");
    }
}
