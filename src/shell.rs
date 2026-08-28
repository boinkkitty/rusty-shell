use std::io::{self, Write};

use crate::command::{self, CommandOutcome};
use crate::parser::parse_command_line;

pub fn run() -> io::Result<()> {
    loop {
        print!("$ ");
        io::stdout().flush()?;

        let mut input = String::new();
        let bytes_read = io::stdin().read_line(&mut input)?;

        if bytes_read == 0 {
            return Ok(());
        }

        let command = parse_command_line(&input);

        match command::execute(&command) {
            Ok(CommandOutcome::Exit) => return Ok(()),
            Ok(CommandOutcome::Continue) => {}
            Err(error) => eprintln!("shell: {error}"),
        }
    }
}
