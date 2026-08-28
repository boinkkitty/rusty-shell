use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

pub enum CommandOutcome {
    Continue,
    Exit,
}

pub fn execute(arguments: &[String]) -> CommandOutcome {
    let Some((command, arguments)) = arguments.split_first() else {
        return CommandOutcome::Continue;
    };

    match command.as_str() {
        "exit" => CommandOutcome::Exit,
        "echo" => {
            println!("{}", arguments.join(" "));
            CommandOutcome::Continue
        }
        "type" => {
            execute_type(arguments.first().map(String::as_str));
            CommandOutcome::Continue
        }
        "pwd" => {
            let current_dir = env::current_dir().expect("current directory should be available");
            println!("{}", current_dir.display());
            CommandOutcome::Continue
        }
        "cd" => {
            execute_cd(arguments.first().map(String::as_str));
            CommandOutcome::Continue
        }
        command => {
            execute_external(command, arguments);
            CommandOutcome::Continue
        }
    }
}

fn execute_type(target: Option<&str>) {
    let Some(target) = target else {
        return;
    };

    if is_builtin(target) {
        println!("{target} is a shell builtin");
    } else if let Some(path) = find_executable(target) {
        println!("{target} is {}", path.display());
    } else {
        println!("{target}: not found");
    }
}

fn execute_cd(directory: Option<&str>) {
    let Some(directory) = directory else {
        return;
    };

    let target = if directory == "~" {
        env::var("HOME").expect("HOME should be set")
    } else {
        directory.to_owned()
    };

    if env::set_current_dir(target).is_err() {
        println!("cd: {directory}: No such file or directory");
    }
}

fn execute_external(command: &str, arguments: &[String]) {
    let Some(path) = find_executable(command) else {
        println!("{command}: command not found");
        return;
    };

    if let Err(error) = Command::new(path).arg0(command).args(arguments).status() {
        eprintln!("{command}: {error}");
    }
}

fn is_builtin(command: &str) -> bool {
    matches!(command, "echo" | "exit" | "type" | "pwd" | "cd")
}

fn find_executable(target: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;

    env::split_paths(&path).find_map(|directory| {
        let candidate = directory.join(target);
        let metadata = fs::metadata(&candidate).ok()?;

        (metadata.is_file() && metadata.permissions().mode() & 0o111 != 0).then_some(candidate)
    })
}
