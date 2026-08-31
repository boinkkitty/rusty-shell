mod command;
mod parser;
mod shell;

fn main() {
    let mut arguments = std::env::args();
    let _program = arguments.next();

    if matches!(arguments.next().as_deref(), Some("--pipeline-builtin")) {
        let stage_arguments = arguments.collect();
        if let Err(error) = command::run_pipeline_builtin(stage_arguments) {
            eprintln!("shell: {error}");
            std::process::exit(1);
        }
        return;
    }

    if let Err(error) = shell::run() {
        eprintln!("shell: {error}");
    }
}
