mod command;
mod parser;
mod shell;

fn main() {
    if let Err(error) = shell::run() {
        eprintln!("shell: {error}");
    }
}
