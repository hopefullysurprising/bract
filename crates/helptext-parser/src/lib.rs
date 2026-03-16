mod error;
mod format;
mod parsers;

pub use error::ParseError;
pub use format::InputFormat;
pub use usage::{Spec, SpecArg, SpecChoices, SpecCommand, SpecFlag};

pub fn parse(format: InputFormat, content: &str) -> Result<Spec, ParseError> {
    format.parse(content)
}
