use std::io::{self, Write};

use anyhow::Result;
use clap::CommandFactory;
use clap_complete::Shell;

use crate::args::Cli;

/// Render the completion script for `shell` into `out`.
pub fn write_script(shell: Shell, out: &mut dyn Write) {
    let mut cmd = Cli::command();
    let bin_name = cmd.get_name().to_string();
    clap_complete::generate(shell, &mut cmd, bin_name, out);
}

/// Print the completion script for `shell` to stdout.
pub fn run(shell: Shell) -> Result<()> {
    // Render into memory first: the generator panics on write errors, and a
    // reader that exits early (`super completions zsh | head`) is not a failure.
    let mut script = Vec::new();
    write_script(shell, &mut script);

    let stdout = io::stdout();
    let mut out = stdout.lock();
    match out.write_all(&script).and_then(|()| out.flush()) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
#[path = "tests/completions_tests.rs"]
mod tests;
