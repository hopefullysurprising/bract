//! The navigation model. A [`Node`] is one entry in the Miller-columns tree: a
//! tool, a command group, or a runnable command. Children are loaded lazily so
//! that a CLI the size of `az` (thousands of commands) costs nothing at startup
//! and only pays for the subtrees the user actually opens.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NodeKind {
    /// Has children to descend into (loaded on demand).
    Branch,
    /// A terminal command with no children.
    Leaf,
    /// Expandability not yet known — resolved by loading the node's children
    /// (used for frameworks like Cobra whose help doesn't advertise nesting).
    Unknown,
}

#[derive(Clone)]
pub enum Children {
    /// Not fetched yet; call the source's loader with this node's command path.
    Unloaded,
    /// Fetch in progress (a request is in flight on the background loader).
    Loading,
    Loaded(Vec<Node>),
}

#[derive(Clone)]
pub struct Node {
    /// Unique identifier across the whole tree (the full path).
    pub id: String,
    /// Display name (the last path segment).
    pub name: String,
    pub description: String,
    pub kind: NodeKind,
    /// Whether selecting this node can open a run form (a real invokable command).
    pub runnable: bool,
    pub flags: Vec<Flag>,
    pub args: Vec<Arg>,
    /// Which tool this node belongs to (used to route lazy loads to its source).
    pub tool_id: String,
    /// Command segments under the tool binary, e.g. ["account", "list"].
    pub command_path: Vec<String>,
    pub children: Children,
}

impl Node {
    pub fn is_expandable(&self) -> bool {
        !matches!(self.kind, NodeKind::Leaf)
    }
}

#[derive(Clone)]
pub enum FlagKind {
    Boolean,
    Value {
        arg_name: String,
        default: String,
        choices: Vec<String>,
    },
}

#[derive(Clone)]
pub struct Flag {
    pub name: String,
    pub short: Option<char>,
    pub long: Option<String>,
    pub description: String,
    pub required: bool,
    pub kind: FlagKind,
}

#[derive(Clone)]
pub struct Arg {
    pub name: String,
    pub description: String,
    pub required: bool,
    pub default: String,
    pub choices: Vec<String>,
}
