use std::io::{self, IsTerminal, Write};

use crate::command::{self, CommandOutcome};
use crate::parser::parse_command_line;
use crate::repl::ShellEditor;

// Reads one command using line input for pipes and key input for terminals.
fn read_terminal_input(editor: &mut ShellEditor) -> io::Result<Option<String>> {
    let mut stdout = io::stdout().lock();
    let mut reaped_output = Vec::new();
    command::reap_completed_jobs(&mut reaped_output)?;
    stdout.write_all(&normalize_terminal_newlines(&reaped_output))?;
    stdout.flush()?;

    editor.read_line("$ ")
}

fn write_prompt(output: &mut dyn Write) -> io::Result<()> {
    command::reap_completed_jobs(output)?;
    output.write_all(b"$ ")?;
    output.flush()
}

fn normalize_terminal_newlines(output: &[u8]) -> Vec<u8> {
    let mut normalized = Vec::with_capacity(output.len());

    for byte in output {
        if *byte == b'\n' {
            normalized.push(b'\r');
        }
        normalized.push(*byte);
    }

    normalized
}

// Runs the shell loop until exit, EOF, or an unrecoverable input error.
pub fn run() -> io::Result<()> {
    let interactive = io::stdin().is_terminal();
    let mut editor = interactive.then(ShellEditor::new).transpose()?;

    loop {
        let input = if let Some(editor) = editor.as_mut() {
            read_terminal_input(editor)?
        } else {
            let mut stdout = io::stdout().lock();
            write_prompt(&mut stdout)?;

            let mut input = String::new();
            match io::stdin().read_line(&mut input)? {
                0 => None,
                _ => Some(input),
            }
        };

        let Some(input) = input else {
            return Ok(());
        };

        command::record_history(&input);
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
    use super::normalize_terminal_newlines;

    #[test]
    fn normalizes_terminal_newlines_to_carriage_return_line_feed() {
        assert_eq!(
            normalize_terminal_newlines(b"[1]+  Done                    cat fifo\n$ "),
            b"[1]+  Done                    cat fifo\r\n$ "
        );
    }
}
