pub mod mise_tasks;

use std::collections::BTreeMap;

use super::commands::{Command, Tool};

pub trait Source {
    fn tool_id(&self) -> &str;
    fn tool_name(&self) -> &str;
    fn discover(&self) -> Result<Vec<Command>, Box<dyn std::error::Error>>;
}

pub fn assemble_tools(sources: Vec<Box<dyn Source>>) -> Result<Vec<Tool>, Box<dyn std::error::Error>> {
    let mut tool_map: BTreeMap<String, Tool> = BTreeMap::new();

    for source in &sources {
        let commands = source.discover()?;
        tool_map
            .entry(source.tool_id().to_string())
            .or_insert_with(|| Tool {
                id: source.tool_id().to_string(),
                name: source.tool_name().to_string(),
                commands: vec![],
            })
            .commands
            .extend(commands);
    }

    Ok(tool_map.into_values().collect())
}
