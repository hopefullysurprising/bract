use crate::error::ParseError;
use usage::Spec;

pub fn parse(content: &str) -> Result<Spec, ParseError> {
    content.parse::<Spec>().map_err(ParseError::from)
}
