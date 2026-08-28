use std::io::{self, Write};

use crate::command::{self, CommandOutcome};
use crate::parser::parse_command_line;

pub fn run() -> io::Result<()> {
    loop {
        print!("$ ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        let arguments = parse_command_line(&input);

        if matches!(command::execute(&arguments), CommandOutcome::Exit) {
            return Ok(());
        }
    }
}
