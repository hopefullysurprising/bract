use bract::cli::Cli;
use clap::Parser;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    bract::run_main(Cli::parse())
}
