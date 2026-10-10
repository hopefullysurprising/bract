//! Usage specs, as usage-lib reads them — and read anyway when the tool that wrote
//! one is newer than this usage-lib. mise 2026.9.7 describes itself with features
//! from usage 6 (`unknown_flags`, a flag's `conflicts=` and `overrides`, `choices`
//! written as a block); usage-lib rejects what it does not know, and one such entry
//! used to cost the whole spec.

use kdl::{KdlDocument, KdlNode};
use usage::error::UsageErr;
use usage::Spec;

use crate::error::ParseError;
use crate::Parsed;

/// Far more than any real spec needs; a bound so a parser that keeps complaining
/// cannot keep this loop going.
const MOST_PASSES: usize = 50;

pub fn parse(content: &str) -> Result<Spec, ParseError> {
    read(content).map(|parsed| parsed.spec)
}

/// The spec, without whatever usage-lib rejects, and the names of what was left out.
///
/// usage-lib names one rejected node or property at a time. Every other of the same
/// kind — the same property on the same kind of node, the same child in the same
/// form under the same kind of parent — goes with it, so a spec using a newer
/// feature a hundred times is reread a handful of times, not a hundred.
pub fn read(content: &str) -> Result<Parsed, ParseError> {
    let mut text = content.to_string();
    let mut skipped: Vec<String> = Vec::new();
    for _ in 0..MOST_PASSES {
        match text.parse::<Spec>() {
            Ok(spec) => return Ok(Parsed { spec, skipped }),
            // usage-lib displays this as "Invalid usage config" and keeps the reason
            // and its place beside it.
            Err(UsageErr::InvalidInput(reason, span, _)) => match without(&text, span.offset()) {
                Some((rest, name)) => {
                    if !skipped.contains(&name) {
                        skipped.push(name);
                    }
                    text = rest;
                }
                None => {
                    let line = text[..span.offset().min(text.len())].lines().count().max(1);
                    return Err(ParseError::InvalidInput(format!("{reason} (line {line})")));
                }
            },
            Err(other) => return Err(ParseError::from(other)),
        }
    }
    Err(ParseError::InvalidInput(format!("still rejected after leaving out {}", skipped.join(", "))))
}

/// What usage-lib pointed at: a node by its name, or a property by its key.
enum Rejected {
    Node { parent: Option<String>, name: String, block: bool },
    Property { node: String, key: String },
}

/// `text` without the item at `offset` and every other of its kind, and its name.
fn without(text: &str, offset: usize) -> Option<(String, String)> {
    let mut document: KdlDocument = text.parse().ok()?;
    let rejected = locate(document.nodes(), None, offset)?;
    drop_all(document.nodes_mut(), None, &rejected);
    let name = match rejected {
        Rejected::Node { name, .. } => name,
        Rejected::Property { key, .. } => key,
    };
    Some((document.to_string(), name))
}

fn locate(nodes: &[KdlNode], parent: Option<&str>, offset: usize) -> Option<Rejected> {
    nodes.iter().find_map(|node| {
        if node.name().span().offset() == offset {
            return Some(Rejected::Node {
                parent: parent.map(String::from),
                name: node.name().value().to_string(),
                block: node.children().is_some(),
            });
        }
        if let Some(key) = node.entries().iter().find(|e| e.span().offset() == offset).and_then(|e| e.name()) {
            return Some(Rejected::Property { node: node.name().value().to_string(), key: key.value().to_string() });
        }
        node.children().and_then(|children| locate(children.nodes(), Some(node.name().value()), offset))
    })
}

fn drop_all(nodes: &mut Vec<KdlNode>, parent: Option<&str>, rejected: &Rejected) {
    if let Rejected::Node { parent: p, name, block } = rejected {
        nodes.retain(|n| !(p.as_deref() == parent && n.name().value() == name && n.children().is_some() == *block));
    }
    for node in nodes.iter_mut() {
        let own = node.name().value().to_string();
        if let Rejected::Property { node: owner, key } = rejected
            && *owner == own
        {
            node.entries_mut().retain(|e| e.name().map(|k| k.value()) != Some(key.as_str()));
        }
        if let Some(children) = node.children_mut() {
            drop_all(children.nodes_mut(), Some(&own), rejected);
        }
    }
}
