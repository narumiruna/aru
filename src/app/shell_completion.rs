use std::io::{self, Write};

use clap::CommandFactory;
use clap_complete::{Shell, generate as generate_completion};

use crate::cli::{Cli, CompletionShell, GenerateShellCompletionArgs};
use crate::error::{IoContext, Result};

pub(super) fn generate(args: GenerateShellCompletionArgs) -> Result<()> {
    let shell = match args.shell {
        CompletionShell::Bash => Shell::Bash,
        CompletionShell::Zsh => Shell::Zsh,
        CompletionShell::Fish => Shell::Fish,
    };
    let mut command = Cli::command();
    let binary_name = command.get_name().to_owned();
    let mut script = Vec::new();
    generate_completion(shell, &mut command, binary_name, &mut script);

    let stdout = io::stdout();
    stdout.lock().write_all(&script).at("<stdout>")
}
