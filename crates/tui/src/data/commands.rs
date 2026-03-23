#[derive(Clone)]
pub struct Tool {
    pub id: String,
    pub name: String,
    pub bin: Vec<String>,
    pub path_separator: String,
    pub description: String,
    pub flags: Vec<Flag>,
    pub args: Vec<Arg>,
    pub commands: Vec<Command>,
}

#[derive(Clone)]
pub struct Command {
    pub id: String,
    pub name: String,
    pub description: String,
    pub flags: Vec<Flag>,
    pub args: Vec<Arg>,
    pub subcommands: Vec<Command>,
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
