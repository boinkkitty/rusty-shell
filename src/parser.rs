#[derive(Clone, Copy)]
enum QuoteMode {
    Unquoted,
    Single,
    Double,
}

pub fn parse_command_line(input: &str) -> Vec<String> {
    let mut arguments = Vec::new(); // Completed arguments
    let mut current = String::new(); // Argument being built
    let mut token_started = false; // Preserves empty quotes
    let mut mode = QuoteMode::Unquoted; // Current quoting context

    let mut characters = input.chars().peekable();

    while let Some(character) = characters.next() {
        match (mode, character) {
            (QuoteMode::Unquoted, character) if character.is_whitespace() => {
                if token_started {
                    arguments.push(std::mem::take(&mut current));
                    token_started = false;
                }
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

    if token_started {
        arguments.push(current);
    }

    arguments
}
