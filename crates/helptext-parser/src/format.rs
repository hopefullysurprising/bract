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
    OclifHelptext,
    OclifCommandsJson,
    FangHelptext,
}

impl InputFormat {
    /// The format a tool's pages are read in, settled by its root page — asked for
    /// only where it can change the answer. A binary can link fang without sending
    /// its help through it, and such a tool's pages are Cobra's own. Without a root
    /// page, the detection stands.
    pub fn confirm(self, root_page: impl FnOnce() -> Option<String>) -> InputFormat {
        match self {
            InputFormat::FangHelptext if root_page().is_some_and(|page| !parsers::fang_helptext::renders(&page)) => {
                InputFormat::CobraHelptext
            }
            format => format,
        }
    }

    pub(crate) fn parse(self, content: &str) -> Result<Spec, ParseError> {
        match self {
            InputFormat::UsageKdl => parsers::usage_kdl::parse(content),
            InputFormat::CobraHelptext => parsers::cobra_helptext::parse(content),
            InputFormat::FangHelptext => parsers::fang_helptext::parse(content),
            InputFormat::KnackHelptext => parsers::knack_helptext::parse(content),
            InputFormat::ClapHelptext => parsers::clap_helptext::parse(content),
            InputFormat::OclifHelptext => parsers::oclif_helptext::parse(content),
            InputFormat::OclifCommandsJson => parsers::oclif_commands_json::parse(content),
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
            ["usage-kdl", "cobra-helptext", "knack-helptext", "clap-helptext", "oclif-helptext", "oclif-commands-json", "fang-helptext"]
        );
    }
}
