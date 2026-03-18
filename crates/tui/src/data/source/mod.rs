pub mod mise_tasks;
pub mod mise_tools;

use std::collections::BTreeMap;

use super::commands::{Command, Tool};

pub struct DiscoveryResult {
    pub description: String,
    pub commands: Vec<Command>,
}

pub trait Source {
    fn tool_id(&self) -> &str;
    fn tool_name(&self) -> &str;
    fn discover(&self) -> Result<DiscoveryResult, Box<dyn std::error::Error>>;
}

pub fn assemble_tools(sources: Vec<Box<dyn Source>>) -> Result<Vec<Tool>, Box<dyn std::error::Error>> {
    let mut tool_map: BTreeMap<String, Tool> = BTreeMap::new();

    for source in &sources {
        let result = source.discover()?;
        let tool = tool_map
            .entry(source.tool_id().to_string())
            .or_insert_with(|| Tool {
                id: source.tool_id().to_string(),
                name: source.tool_name().to_string(),
                description: String::new(),
                commands: vec![],
            });
        if !result.description.is_empty() {
            tool.description = result.description;
        }
        tool.commands.extend(result.commands);
    }

    Ok(tool_map.into_values().collect())
}
