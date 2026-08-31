use std::borrow::Cow::{self, Borrowed, Owned};
use std::env;
use std::io;

use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::history::DefaultHistory;
use rustyline::validate::Validator;
use rustyline::{CompletionType, Config, Context, Editor, Helper};

use crate::command;

#[derive(Default)]
struct ShellHelper;

impl Helper for ShellHelper {}

impl Hinter for ShellHelper {
    type Hint = String;
}

impl Highlighter for ShellHelper {}

impl Validator for ShellHelper {}

impl Completer for ShellHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _context: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let (start, prefix) = completion_prefix(line, pos);
        let candidates = if start == pos {
            Vec::new()
        } else {
            command::completion_candidates(prefix, env::var_os("PATH").as_deref())
                .into_iter()
                .map(|candidate| Pair {
                    display: candidate.clone(),
                    replacement: candidate,
                })
                .collect()
        };

        Ok((start, candidates))
    }
}

pub struct ShellEditor {
    editor: Editor<ShellHelper, DefaultHistory>,
}

impl ShellEditor {
    pub fn new() -> io::Result<Self> {
        let config = Config::builder()
            .completion_type(CompletionType::List)
            .build();
        let mut editor =
            Editor::with_config(config).map_err(|error| io::Error::other(error.to_string()))?;
        editor.set_helper(Some(ShellHelper));

        Ok(Self { editor })
    }

    pub fn read_line(&mut self, prompt: &str) -> io::Result<Option<String>> {
        match self.editor.readline(prompt) {
            Ok(line) => {
                self.remember(&line);
                Ok(Some(submitted_line(&line)))
            }
            Err(ReadlineError::Eof) => Ok(None),
            Err(ReadlineError::Interrupted) => Ok(Some(String::new())),
            Err(error) => Err(io::Error::other(error.to_string())),
        }
    }

    fn remember(&mut self, line: &str) {
        let _ = self.editor.add_history_entry(line);
    }
}

fn submitted_line(line: &str) -> String {
    format!("{line}\n")
}

fn completion_prefix(line: &str, cursor: usize) -> (usize, &str) {
    let prefix = &line[..cursor];
    let start = prefix
        .rfind(char::is_whitespace)
        .map_or(0, |index| index + 1);

    if start != 0 {
        return (cursor, "");
    }

    (start, &prefix[start..])
}

#[allow(dead_code)]
pub(crate) fn completion_display(parts: &[String]) -> Cow<'_, str> {
    if parts.is_empty() {
        Borrowed("")
    } else {
        Owned(parts.join("  "))
    }
}

#[cfg(test)]
mod tests {
    use super::submitted_line;

    #[test]
    fn submitted_lines_are_normalized_for_shell_execution() {
        assert_eq!(submitted_line("echo world"), "echo world\n");
    }
}
