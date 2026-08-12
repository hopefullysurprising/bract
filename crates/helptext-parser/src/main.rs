use clap::Parser;
use helptext_parser::{parse, InputFormat};
use std::io::Read;

// `about` renders the crate description, so it stays in one place (Cargo.toml).
#[derive(Parser)]
#[command(version, about, after_help = "Reads the content to parse from stdin.")]
struct Cli {
    /// Format of the input on stdin
    format: InputFormat,
}

fn main() {
    let cli = Cli::parse();

    let mut content = String::new();
    if let Err(e) = std::io::stdin().read_to_string(&mut content) {
        eprintln!("failed to read stdin: {e}");
        std::process::exit(1);
    }

    match parse(cli.format, &content) {
        Ok(spec) => println!("{spec:#?}"),
        Err(e) => {
            eprintln!("parse error: {e}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }
}
