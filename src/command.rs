use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use crate::parser::{ParsedCommand, RedirectTarget};

pub enum CommandOutcome {
    Continue,
    Exit,
}

pub fn execute(parsed: &ParsedCommand) -> io::Result<CommandOutcome> {
    let Some((command, arguments)) = parsed.arguments.split_first() else {
        return Ok(CommandOutcome::Continue);
    };

    let mut stdout_file = open_redirection(parsed.stdout.as_ref())?;
    let mut stderr_file = open_redirection(parsed.stderr.as_ref())?;
    let mut terminal_stdout = io::stdout();
    let mut terminal_stderr = io::stderr();

    let outcome = match command.as_str() {
        "exit" => CommandOutcome::Exit,
        "echo" => {
            writeln!(
                output_writer(&mut stdout_file, &mut terminal_stdout),
                "{}",
                arguments.join(" ")
            )?;
            CommandOutcome::Continue
        }
        "type" => {
            execute_type(
                arguments.first().map(String::as_str),
                output_writer(&mut stdout_file, &mut terminal_stdout),
            )?;
            CommandOutcome::Continue
        }
        "pwd" => {
            let current_dir = env::current_dir().expect("current directory should be available");
            writeln!(
                output_writer(&mut stdout_file, &mut terminal_stdout),
                "{}",
                current_dir.display()
            )?;
            CommandOutcome::Continue
        }
        "cd" => {
            execute_cd(
                arguments.first().map(String::as_str),
                output_writer(&mut stderr_file, &mut terminal_stderr),
            )?;
            CommandOutcome::Continue
        }
        command => {
            return execute_external(command, arguments, stdout_file, stderr_file);
        }
    };

    Ok(outcome)
}

fn execute_type(target: Option<&str>, output: &mut dyn Write) -> io::Result<()> {
    let Some(target) = target else {
        return Ok(());
    };

    if is_builtin(target) {
        writeln!(output, "{target} is a shell builtin")?;
    } else if let Some(path) = find_executable(target) {
        writeln!(output, "{target} is {}", path.display())?;
    } else {
        writeln!(output, "{target}: not found")?;
    }

    Ok(())
}

fn execute_cd(directory: Option<&str>, error: &mut dyn Write) -> io::Result<()> {
    let Some(directory) = directory else {
        return Ok(());
    };

    let target = if directory == "~" {
        let Ok(home) = env::var("HOME") else {
            writeln!(error, "cd: HOME not set")?;
            return Ok(());
        };
        home
    } else {
        directory.to_owned()
    };

    if env::set_current_dir(target).is_err() {
        writeln!(error, "cd: {directory}: No such file or directory")?;
    }

    Ok(())
}

fn execute_external(
    command: &str,
    arguments: &[String],
    stdout_file: Option<File>,
    mut stderr_file: Option<File>,
) -> io::Result<CommandOutcome> {
    let Some(path) = find_executable(command) else {
        let mut terminal_stderr = io::stderr();
        writeln!(
            output_writer(&mut stderr_file, &mut terminal_stderr),
            "{command}: command not found"
        )?;
        return Ok(CommandOutcome::Continue);
    };

    let mut process = Command::new(path);
    process.arg0(command).args(arguments);

    if let Some(file) = stdout_file {
        process.stdout(Stdio::from(file));
    }
    if let Some(file) = stderr_file {
        process.stderr(Stdio::from(file));
    }

    process.status()?;
    Ok(CommandOutcome::Continue)
}

// Uses the redirected file when present, otherwise the terminal
fn output_writer<'a>(
    redirected: &'a mut Option<File>,
    terminal: &'a mut dyn Write,
) -> &'a mut dyn Write {
    match redirected {
        Some(file) => file,
        None => terminal,
    }
}

fn open_redirection(target: Option<&RedirectTarget>) -> io::Result<Option<File>> {
    target
        .map(|target| {
            let mut options = OpenOptions::new();
            options.create(true).write(true);

            if target.append {
                options.append(true);
            } else {
                options.truncate(true);
            }

            options.open(&target.path)
        })
        .transpose()
}

fn is_builtin(command: &str) -> bool {
    matches!(command, "echo" | "exit" | "type" | "pwd" | "cd")
}

fn find_executable(target: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;

    env::split_paths(&path).find_map(|directory| {
        let candidate = directory.join(target);
        let metadata = fs::metadata(&candidate).ok()?;

        (metadata.is_file() && metadata.permissions().mode() & 0o111 != 0).then_some(candidate)
    })
}
