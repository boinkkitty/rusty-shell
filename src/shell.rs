use std::env;
use std::ffi::OsStr;
use std::io::{self, IsTerminal, Read, Write};
use std::process::{Command as ProcessCommand, Stdio};

use crate::command::{self, CommandOutcome};
use crate::parser::parse_command_line;

const COMPLETABLE_BUILTINS: [&str; 2] = ["echo", "exit"];

// Finds builtins whose names start with the text already typed.
fn matching_builtins(partial: &str) -> impl Iterator<Item = &'static str> + '_ {
    COMPLETABLE_BUILTINS
        .into_iter()
        .filter(move |builtin| builtin.starts_with(partial))
}

// Combines builtin and executable matches into one sorted, duplicate-free list.
fn completion_matches(partial: &str, path: Option<&OsStr>) -> Vec<String> {
    // Whitespace means the cursor is already in an argument, not the command name.
    if partial.is_empty() || partial.chars().any(char::is_whitespace) {
        return Vec::new();
    }

    // Start with the builtins supported by this shell.
    let mut matches: Vec<_> = matching_builtins(partial).map(str::to_owned).collect();
    // Add executable names discovered in PATH.
    matches.extend(command::executable_names(partial, path));
    // Keep output deterministic and avoid duplicate names.
    matches.sort_unstable();
    matches.dedup();
    matches
}

// Finds the longest character-safe prefix shared by every candidate.
fn longest_common_prefix(matches: &[String]) -> String {
    // No candidates means there is no prefix to complete.
    let Some(first) = matches.first() else {
        return String::new();
    };

    // Shrink the prefix until it is shared by every candidate.
    matches
        .iter()
        .skip(1)
        .fold(first.clone(), |mut prefix, candidate| {
            let common_length = prefix
                .chars()
                .zip(candidate.chars())
                .take_while(|(left, right)| left == right)
                .map(|(character, _)| character.len_utf8())
                .sum();
            prefix.truncate(common_length);
            prefix
        })
}

// Returns only the text that still needs to be inserted for a completion.
fn completion_suffix_for(partial: &str, matches: &[String]) -> Option<String> {
    // A single match gets a trailing space; multiple matches use their LCP.
    let completion = match matches {
        [single] => format!("{single} "),
        [_, _, ..] => longest_common_prefix(matches),
        [] => return None,
    };

    completion
        .strip_prefix(partial)
        .filter(|suffix| !suffix.is_empty())
        .map(str::to_owned)
}

// Applies one Tab press: complete, ring, or display repeated matches.
fn apply_completion(
    input: &mut Vec<u8>,
    output: &mut dyn Write,
    path: Option<&OsStr>,
    repeated_prefix: &mut Option<Vec<u8>>,
) -> io::Result<()> {
    // Invalid UTF-8 cannot be completed, so leave the buffer alone and ring.
    let Some(partial) = std::str::from_utf8(input).ok() else {
        *repeated_prefix = None;
        output.write_all(b"\x07")?;
        return output.flush();
    };
    let matches = completion_matches(partial, path);

    // Extend the current input when a unique match or LCP provides new text.
    if let Some(suffix) = completion_suffix_for(partial, &matches) {
        input.extend_from_slice(suffix.as_bytes());
        output.write_all(suffix.as_bytes())?;
        *repeated_prefix = None;
    // A repeated Tab at an unchanged ambiguous prefix prints all candidates.
    } else if matches.len() > 1 && repeated_prefix.as_deref() == Some(input.as_slice()) {
        write!(output, "\r\n{}\r\n$ {partial}", matches.join("  "))?;
        *repeated_prefix = None;
    // First ambiguous Tab, or any invalid completion, only rings the bell.
    } else {
        output.write_all(b"\x07")?;
        *repeated_prefix = (matches.len() > 1).then(|| input.clone());
    }

    output.flush()
}

// Temporarily enables raw terminal input and restores the original settings on drop.
struct RawMode {
    original_settings: String,
}

impl RawMode {
    // Saves the terminal settings before switching to raw, no-echo mode.
    fn enable() -> io::Result<Self> {
        // Capture the current settings so they can be restored later.
        let original = ProcessCommand::new("stty")
            .arg("-g")
            .stdin(Stdio::inherit())
            .output()?;
        if !original.status.success() {
            return Err(io::Error::other(format!(
                "failed to read terminal settings: {}",
                String::from_utf8_lossy(&original.stderr).trim()
            )));
        }

        let original_settings = String::from_utf8(original.stdout)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?
            .trim()
            .to_owned();

        // Disable canonical input and terminal echo while reading keys.
        let status = ProcessCommand::new("stty")
            .args(["raw", "-echo"])
            .status()?;
        if !status.success() {
            return Err(io::Error::other("failed to enable raw terminal mode"));
        }

        Ok(Self { original_settings })
    }
}

impl Drop for RawMode {
    // Restore terminal settings even when input handling exits early.
    fn drop(&mut self) {
        let _ = ProcessCommand::new("stty")
            .arg(&self.original_settings)
            .status();
    }
}

// Reads one command using line input for pipes and key input for terminals.
fn read_input() -> io::Result<Option<String>> {
    // Pipes already provide complete lines; terminals need key-by-key input.
    if io::stdin().is_terminal() {
        return read_terminal_input();
    }

    // Preserve prompt behavior for non-interactive input.
    print!("$ ");
    io::stdout().flush()?;

    let mut input = String::new();
    match io::stdin().read_line(&mut input)? {
        0 => Ok(None),
        _ => Ok(Some(input)),
    }
}

// Handles terminal keys while raw mode is active.
fn read_terminal_input() -> io::Result<Option<String>> {
    let _raw_mode = RawMode::enable()?;
    print!("$ ");
    io::stdout().flush()?;

    let mut input = Vec::new();
    let mut byte = [0_u8; 1];
    let mut stdin = io::stdin().lock();
    let mut stdout = io::stdout().lock();
    // Tracks whether the previous Tab was pressed at this same ambiguous prefix.
    let mut repeated_prefix = None;

    loop {
        // EOF ends the shell cleanly.
        if stdin.read(&mut byte)? == 0 {
            return Ok(None);
        }

        // Any edit other than Tab starts a fresh completion sequence.
        if byte[0] != b'\t' {
            repeated_prefix = None;
        }

        match byte[0] {
            // Enter
            b'\r' | b'\n' => {
                stdout.write_all(b"\r\n")?;
                stdout.flush()?;
                return String::from_utf8(input)
                    .map(Some)
                    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error));
            }
            // Tab
            b'\t' => {
                let path = env::var_os("PATH");
                apply_completion(
                    &mut input,
                    &mut stdout,
                    path.as_deref(),
                    &mut repeated_prefix,
                )?;
            }
            // Ctrl C
            3 => {
                stdout.write_all(b"^C\r\n")?;
                stdout.flush()?;
                return Ok(Some(String::new()));
            }
            // Ctrl D
            4 if input.is_empty() => return Ok(None),
            // Backspace
            8 | 127 if !input.is_empty() => {
                input.pop();
                stdout.write_all(b"\x08 \x08")?;
                stdout.flush()?;
            }
            character if !character.is_ascii_control() => {
                // Echo ordinary input and retain it in the current command.
                input.push(character);
                stdout.write_all(&byte)?;
                stdout.flush()?;
            }
            _ => {}
        }
    }
}

// Runs the shell loop until exit, EOF, or an unrecoverable input error.
pub fn run() -> io::Result<()> {
    loop {
        let Some(input) = read_input()? else {
            return Ok(());
        };

        let command = parse_command_line(&input);

        match command::execute(&command) {
            Ok(CommandOutcome::Exit) => return Ok(()),
            Ok(CommandOutcome::Continue) => {}
            Err(error) => eprintln!("shell: {error}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::ffi::OsString;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    use super::{apply_completion, completion_matches, completion_suffix_for};

    fn completion_suffix(partial: &str, path: Option<&std::ffi::OsStr>) -> Option<String> {
        let matches = completion_matches(partial, path);
        completion_suffix_for(partial, &matches)
    }

    fn executable_path(label: &str, names: &[&str]) -> (PathBuf, OsString) {
        let directory = env::temp_dir().join(format!("rusty-shell-{label}-{}", std::process::id()));
        fs::create_dir_all(&directory).expect("temporary directory should be created");

        for name in names {
            let executable = directory.join(name);
            fs::write(&executable, "").expect("temporary executable should be created");
            fs::set_permissions(&executable, fs::Permissions::from_mode(0o755))
                .expect("temporary executable should be executable");
        }

        let path = env::join_paths([&directory]).expect("test PATH should be valid");
        (directory, path)
    }

    #[test]
    fn completes_supported_builtin_prefixes_with_a_trailing_space() {
        assert_eq!(completion_suffix("ech", None), Some("o ".to_owned()));
        assert_eq!(completion_suffix("exi", None), Some("t ".to_owned()));
    }

    #[test]
    fn does_not_complete_arguments_or_unknown_commands() {
        assert_eq!(completion_suffix("", None), None);
        assert_eq!(completion_suffix("e", None), None);
        assert_eq!(completion_suffix("echo h", None), None);
        assert_eq!(completion_suffix("cat", None), None);
    }

    #[test]
    fn invalid_completion_rings_bell_without_changing_input() {
        let mut input = b"xyz".to_vec();
        let mut output = Vec::new();
        let mut repeated_prefix = None;

        apply_completion(&mut input, &mut output, None, &mut repeated_prefix)
            .expect("completion should be handled");

        assert_eq!(input, b"xyz");
        assert_eq!(output, b"\x07");
    }

    #[test]
    fn consecutive_tabs_ring_then_list_sorted_matches() {
        let (directory, path) = executable_path("multiple", &["xyz_quz", "xyz_bar", "xyz_baz"]);
        let mut input = b"xyz_".to_vec();
        let mut output = Vec::new();
        let mut repeated_prefix = None;

        apply_completion(&mut input, &mut output, Some(&path), &mut repeated_prefix)
            .expect("first completion should be handled");
        assert_eq!(input, b"xyz_");
        assert_eq!(output, b"\x07");

        apply_completion(&mut input, &mut output, Some(&path), &mut repeated_prefix)
            .expect("second completion should be handled");
        assert_eq!(input, b"xyz_");
        assert_eq!(output, b"\x07\r\nxyz_bar  xyz_baz  xyz_quz\r\n$ xyz_");

        fs::remove_dir_all(directory).expect("temporary directory should be removed");
    }

    #[test]
    fn progressively_completes_longest_common_prefix_then_unique_match() {
        let (directory, path) = executable_path(
            "common-prefix",
            &["xyz_foo", "xyz_foo_bar", "xyz_foo_bar_baz"],
        );
        let mut input = b"xyz_".to_vec();
        let mut output = Vec::new();
        let mut repeated_prefix = None;

        apply_completion(&mut input, &mut output, Some(&path), &mut repeated_prefix)
            .expect("first common prefix should complete");
        assert_eq!(input, b"xyz_foo");

        input.push(b'_');
        repeated_prefix = None;
        apply_completion(&mut input, &mut output, Some(&path), &mut repeated_prefix)
            .expect("second common prefix should complete");
        assert_eq!(input, b"xyz_foo_bar");

        input.push(b'_');
        repeated_prefix = None;
        apply_completion(&mut input, &mut output, Some(&path), &mut repeated_prefix)
            .expect("unique match should complete");
        assert_eq!(input, b"xyz_foo_bar_baz ");

        fs::remove_dir_all(directory).expect("temporary directory should be removed");
    }
}
