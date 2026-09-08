use std::process::ExitCode;

use clap::Parser;
use lore::cli::Cli;
use lore::console;

/// Exit status for a command line that could not be understood, matching the
/// convention clap uses when it reports usage errors itself.
const USAGE: u8 = 2;

fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) if error.use_stderr() => {
            console::report(&error.render().to_string());
            return ExitCode::from(USAGE);
        }
        // Help and version are output, not failures.
        Err(help) => {
            let _ = help.print();
            return ExitCode::SUCCESS;
        }
    };

    match cli.run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            console::report(&format!("lore: {error:#}\n"));
            ExitCode::FAILURE
        }
    }
}
