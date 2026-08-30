use std::path::PathBuf;

#[derive(Clone, Copy)]
enum QuoteMode {
    Unquoted,
    Single,
    Double,
}

#[derive(Clone, Copy)]
enum RedirectStream {
    Stdout,
    Stderr,
}

#[derive(Clone, Copy)]
struct PendingRedirection {
    stream: RedirectStream,
    append: bool,
}

pub struct RedirectTarget {
    pub path: PathBuf,
    pub append: bool,
}

#[derive(Default)]
pub struct ParsedCommand {
    pub arguments: Vec<String>,
    pub stdout: Option<RedirectTarget>,
    pub stderr: Option<RedirectTarget>,
}

pub fn parse_command_line(input: &str) -> ParsedCommand {
    let mut command = ParsedCommand::default(); // Parsed command result
    let mut current = String::new(); // Argument being built
    let mut token_started = false; // Preserves empty quotes
    let mut mode = QuoteMode::Unquoted; // Current quoting context
    let mut pending_redirection = None; // Redirection awaiting a target

    let mut characters = input.chars().peekable();

    while let Some(character) = characters.next() {
        match (mode, character) {
            (QuoteMode::Unquoted, character) if character.is_whitespace() => {
                complete_argument(
                    &mut command,
                    &mut current,
                    &mut token_started,
                    &mut pending_redirection,
                );
            }
            (QuoteMode::Unquoted, descriptor @ ('1' | '2'))
                if !token_started && matches!(characters.peek(), Some('>')) =>
            {
                characters.next();
                let append = matches!(characters.peek(), Some('>'));
                if append {
                    characters.next();
                }
                pending_redirection = Some(PendingRedirection {
                    stream: if descriptor == '2' {
                        RedirectStream::Stderr
                    } else {
                        RedirectStream::Stdout
                    },
                    append,
                });
            }
            (QuoteMode::Unquoted, '>') => {
                complete_argument(
                    &mut command,
                    &mut current,
                    &mut token_started,
                    &mut pending_redirection,
                );
                let append = matches!(characters.peek(), Some('>'));
                if append {
                    characters.next();
                }
                pending_redirection = Some(PendingRedirection {
                    stream: RedirectStream::Stdout,
                    append,
                });
            }
            (QuoteMode::Unquoted, '\'') => {
                mode = QuoteMode::Single;
                token_started = true;
            }
            (QuoteMode::Unquoted, '"') => {
                mode = QuoteMode::Double;
                token_started = true;
            }
            (QuoteMode::Unquoted, '\\') => {
                if let Some(escaped) = characters.next() {
                    current.push(escaped);
                }
                token_started = true;
            }
            (QuoteMode::Single, '\'') => mode = QuoteMode::Unquoted,
            (QuoteMode::Double, '"') => mode = QuoteMode::Unquoted,
            (QuoteMode::Double, '\\') => {
                if matches!(characters.peek(), Some('"' | '\\')) {
                    current.push(characters.next().expect("peeked character should exist"));
                } else {
                    current.push('\\');
                }
                token_started = true;
            }
            (_, character) => {
                current.push(character);
                token_started = true;
            }
        }
    }

    complete_argument(
        &mut command,
        &mut current,
        &mut token_started,
        &mut pending_redirection,
    );

    command
}

fn complete_argument(
    command: &mut ParsedCommand,
    current: &mut String,
    token_started: &mut bool,
    pending_redirection: &mut Option<PendingRedirection>,
) {
    if !*token_started {
        return;
    }

    let argument = std::mem::take(current);

    match pending_redirection.take() {
        Some(redirection) => {
            let target = RedirectTarget {
                path: argument.into(),
                append: redirection.append,
            };

            match redirection.stream {
                RedirectStream::Stdout => command.stdout = Some(target),
                RedirectStream::Stderr => command.stderr = Some(target),
            }
        }
        None => command.arguments.push(argument),
    }

    *token_started = false;
}
