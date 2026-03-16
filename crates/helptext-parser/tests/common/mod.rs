use helptext_parser::{parse, InputFormat, Spec};

pub fn parse_fixture(format: InputFormat, format_dir: &str, filename: &str) -> Spec {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(format_dir)
        .join(filename);
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read fixture {}: {e}", path.display()));
    parse(format, &content)
        .unwrap_or_else(|e| panic!("failed to parse fixture {}: {e}", path.display()))
}
