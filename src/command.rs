use std::collections::BTreeSet;
use std::env;
use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
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
        // Exit is handled by the shell loop instead of an external process.
        "exit" => CommandOutcome::Exit,
        // Builtins write through the selected stdout/stderr destination.
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
            // Every other command is resolved and launched from PATH.
            return execute_external(command, arguments, stdout_file, stderr_file);
        }
    };

    Ok(outcome)
}

fn execute_type(target: Option<&str>, output: &mut dyn Write) -> io::Result<()> {
    let Some(target) = target else {
        return Ok(());
    };

    // Builtins are reported directly.
    if is_builtin(target) {
        writeln!(output, "{target} is a shell builtin")?;
    // External commands report their resolved executable path.
    } else if let Some(path) = find_executable(target) {
        writeln!(output, "{target} is {}", path.display())?;
    // Unknown names match the shell's not-found response.
    } else {
        writeln!(output, "{target}: not found")?;
    }

    Ok(())
}

fn execute_cd(directory: Option<&str>, error: &mut dyn Write) -> io::Result<()> {
    let Some(directory) = directory else {
        return Ok(());
    };

    // Expand the home shortcut before changing directories.
    let target = if directory == "~" {
        let Ok(home) = env::var("HOME") else {
            writeln!(error, "cd: HOME not set")?;
            return Ok(());
        };
        home
    // Otherwise use the path exactly as entered.
    } else {
        directory.to_owned()
    };

    // Keep cd failures as shell-style diagnostics rather than aborting.
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

    // Attach each redirected stream only when requested.
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
    // Prefer the file; otherwise write to the terminal stream.
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

            // Append preserves existing bytes; truncate replaces them.
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
        is_executable(&candidate).then_some(candidate)
    })
}

// Lists executable PATH entries that begin with the requested prefix.
pub(crate) fn executable_names(prefix: &str, path: Option<&OsStr>) -> Vec<String> {
    // No PATH means there are no external completion candidates.
    let Some(path) = path else {
        return Vec::new();
    };

    // Ignore missing directories and unreadable entries while scanning PATH.
    env::split_paths(path)
        .filter_map(|directory| fs::read_dir(directory).ok())
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            (name.starts_with(prefix) && is_executable(&entry.path())).then_some(name)
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

// Checks that a path is a regular file with at least one execute bit set.
fn is_executable(path: &Path) -> bool {
    fs::metadata(path)
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    use super::executable_names;

    #[test]
    fn finds_executable_prefix_when_path_contains_a_missing_directory() {
        let directory =
            env::temp_dir().join(format!("rusty-shell-completion-{}", std::process::id()));
        fs::create_dir_all(&directory).expect("temporary directory should be created");

        let executable = directory.join("custom_executable");
        fs::write(&executable, "").expect("temporary executable should be created");
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755))
            .expect("temporary executable should be executable");

        let missing_directory = directory.join("missing");
        let path = env::join_paths([missing_directory, directory.clone()])
            .expect("test PATH should be valid");

        assert_eq!(
            executable_names("custom", Some(&path)),
            vec!["custom_executable"]
        );

        fs::remove_dir_all(directory).expect("temporary directory should be removed");
    }
}
