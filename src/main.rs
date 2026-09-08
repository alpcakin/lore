use anyhow::Result;
use clap::Parser;
use lore::cli::Cli;

fn main() -> Result<()> {
    Cli::parse().run()
}
