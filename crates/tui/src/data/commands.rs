pub struct Tool {
    pub id: String,
    pub name: String,
    pub commands: Vec<Command>,
}

pub struct Command {
    pub id: String,
    pub name: String,
    pub description: String,
    pub subcommands: Vec<Command>,
}
