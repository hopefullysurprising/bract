use std::fmt;

#[derive(Debug)]
pub enum ParseError {
    InvalidInput(String),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::InvalidInput(msg) => write!(f, "invalid input: {msg}"),
        }
    }
}

impl std::error::Error for ParseError {}

impl From<usage::error::UsageErr> for ParseError {
    fn from(err: usage::error::UsageErr) -> Self {
        ParseError::InvalidInput(err.to_string())
    }
}
