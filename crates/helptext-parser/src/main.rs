use helptext_parser::{parse, InputFormat};
use std::io::Read;

fn main() {
    let format = match std::env::args().nth(1).as_deref() {
        Some("usage-kdl") => InputFormat::UsageKdl,
        Some("cobra-helptext") => InputFormat::CobraHelptext,
        Some(other) => {
            eprintln!("unknown format: {other}");
            eprintln!("supported formats: usage-kdl, cobra-helptext");
            std::process::exit(1);
        }
        None => {
            eprintln!("usage: helptext-parser <format>");
            eprintln!("reads content from stdin");
            eprintln!("supported formats: usage-kdl, cobra-helptext");
            std::process::exit(1);
        }
    };

    let mut content = String::new();
    std::io::stdin().read_to_string(&mut content).unwrap_or_else(|e| {
        eprintln!("failed to read stdin: {e}");
        std::process::exit(1);
    });

    match parse(format, &content) {
        Ok(spec) => println!("{spec:#?}"),
        Err(e) => {
            eprintln!("parse error: {e}");
            std::process::exit(1);
        }
    }
}
