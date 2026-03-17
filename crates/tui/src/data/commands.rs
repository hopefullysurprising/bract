pub struct Tool {
    pub id: &'static str,
    pub name: &'static str,
    pub commands: Vec<Command>,
}

pub struct Command {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub subcommands: Vec<Command>,
}

pub fn sample_tools() -> Vec<Tool> {
    vec![
        Tool {
            id: "mise",
            name: "mise",
            commands: vec![
                Command {
                    id: "mise:build",
                    name: "build",
                    description: "Build the project",
                    subcommands: vec![],
                },
                Command {
                    id: "mise:deploy",
                    name: "deploy",
                    description: "Deploy to target environment",
                    subcommands: vec![],
                },
                Command {
                    id: "mise:format",
                    name: "format",
                    description: "Run formatters",
                    subcommands: vec![],
                },
                Command {
                    id: "mise:lint",
                    name: "lint",
                    description: "Run linters",
                    subcommands: vec![],
                },
                Command {
                    id: "mise:test",
                    name: "test",
                    description: "Run test suite",
                    subcommands: vec![],
                },
                Command {
                    id: "mise:backstage",
                    name: "backstage",
                    description: "Backstage utilities",
                    subcommands: vec![
                        Command {
                            id: "mise:backstage:generate_mise_docs",
                            name: "generate_mise_docs",
                            description: "Generate mise documentation",
                            subcommands: vec![],
                        },
                        Command {
                            id: "mise:backstage:validate_configs",
                            name: "validate_configs",
                            description: "Validate all config files",
                            subcommands: vec![],
                        },
                    ],
                },
            ],
        },
        Tool {
            id: "mani",
            name: "mani",
            commands: vec![
                Command {
                    id: "mani:sync",
                    name: "sync",
                    description: "Sync all repositories",
                    subcommands: vec![],
                },
                Command {
                    id: "mani:run",
                    name: "run",
                    description: "Run command across repos",
                    subcommands: vec![],
                },
                Command {
                    id: "mani:list",
                    name: "list",
                    description: "List projects and tags",
                    subcommands: vec![],
                },
            ],
        },
        Tool {
            id: "docker",
            name: "docker",
            commands: vec![
                Command {
                    id: "docker:compose",
                    name: "compose",
                    description: "Docker Compose operations",
                    subcommands: vec![
                        Command {
                            id: "docker:compose:up",
                            name: "up",
                            description: "Start services",
                            subcommands: vec![],
                        },
                        Command {
                            id: "docker:compose:down",
                            name: "down",
                            description: "Stop services",
                            subcommands: vec![],
                        },
                        Command {
                            id: "docker:compose:logs",
                            name: "logs",
                            description: "View service logs",
                            subcommands: vec![],
                        },
                    ],
                },
                Command {
                    id: "docker:build",
                    name: "build",
                    description: "Build an image",
                    subcommands: vec![],
                },
                Command {
                    id: "docker:ps",
                    name: "ps",
                    description: "List containers",
                    subcommands: vec![],
                },
            ],
        },
    ]
}
