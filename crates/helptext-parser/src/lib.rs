mod error;
mod format;
mod parsers;

pub use error::ParseError;
pub use format::InputFormat;
pub use usage::{Spec, SpecArg, SpecChoices, SpecCommand, SpecFlag};

pub fn parse(format: InputFormat, content: &str) -> Result<Spec, ParseError> {
    read(format, content).map(|parsed| parsed.spec)
}

/// A spec, and what of the input was left out so the rest could be read.
pub struct Parsed {
    pub spec: Spec,
    /// usage-lib's reason for each entry it does not support, in the order dropped:
    /// `unsupported spec key unknown_flags`. Only a Usage spec has any.
    pub skipped: Vec<String>,
}

pub fn read(format: InputFormat, content: &str) -> Result<Parsed, ParseError> {
    match format {
        InputFormat::UsageKdl => parsers::usage_kdl::read(content),
        other => other.parse(content).map(|spec| Parsed { spec, skipped: Vec::new() }),
    }
}
