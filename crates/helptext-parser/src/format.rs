use crate::error::ParseError;
use crate::parsers;
use usage::Spec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputFormat {
    UsageKdl,
    CobraHelptext,
}

impl InputFormat {
    pub(crate) fn parse(self, content: &str) -> Result<Spec, ParseError> {
        match self {
            InputFormat::UsageKdl => parsers::usage_kdl::parse(content),
            InputFormat::CobraHelptext => parsers::cobra_helptext::parse(content),
        }
    }
}
