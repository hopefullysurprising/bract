use crate::error::ParseError;
use crate::parsers;
use usage::Spec;

/// The variant list is also the CLI's accepted values: `ValueEnum` renders each
/// as kebab-case (`CobraHelptext` → `cobra-helptext`), so adding a parser adds
/// its CLI value and its `--help` entry with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum InputFormat {
    UsageKdl,
    CobraHelptext,
    KnackHelptext,
    ClapHelptext,
}

impl InputFormat {
    pub(crate) fn parse(self, content: &str) -> Result<Spec, ParseError> {
        match self {
            InputFormat::UsageKdl => parsers::usage_kdl::parse(content),
            InputFormat::CobraHelptext => parsers::cobra_helptext::parse(content),
            InputFormat::KnackHelptext => parsers::knack_helptext::parse(content),
            InputFormat::ClapHelptext => parsers::clap_helptext::parse(content),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::ValueEnum;

    // The kebab-case values are the published CLI surface — a variant rename would
    // silently change what users (and the docs) type.
    #[test]
    fn cli_values_are_stable() {
        let values: Vec<String> = InputFormat::value_variants()
            .iter()
            .map(|f| f.to_possible_value().unwrap().get_name().to_string())
            .collect();
        assert_eq!(
            values,
            ["usage-kdl", "cobra-helptext", "knack-helptext", "clap-helptext"]
        );
    }
}
