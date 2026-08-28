use std::env;
use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

fn is_builtin(cmd: &str) -> bool {
    matches!(cmd, "echo" | "exit" | "type" | "pwd" | "cd")
}

fn find_target(target: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;

    for dir in env::split_paths(&path) {
        let candidate = dir.join(target);

        if let Ok(metadata) = fs::metadata(&candidate) {
            if metadata.is_file() && metadata.permissions().mode() & 0o111 != 0 {
                return Some(candidate);
            }
        }
    }

    None
}

fn main() {
    loop {
        print!("$ ");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        let result = io::stdin().read_line(&mut input);

        match result {
            Ok(_) => {
                let parts: Vec<&str> = input.split_whitespace().collect();

                if parts.is_empty() {
                    continue;
                }

                let cmd = parts[0];

                match cmd {
                    "exit" => break,

                    "echo" => {
                        println!("{}", parts[1..].join(" "))
                    }

                    "type" => {
                        if parts.len() < 2 {
                            continue;
                        }

                        let target = parts[1];

                        if is_builtin(target) {
                            println!("{target} is a shell builtin");
                        } else if let Some(path) = find_target(target) {
                            println!("{target} is {}", path.display());
                        } else {
                            println!("{target}: not found");
                        }
                    }

                    "pwd" => {
                        let current_dir = env::current_dir().unwrap();

                        println!("{}", current_dir.display())
                    }

                    "cd" => {
                        if parts.len() < 2 {
                            continue;
                        }

                        let dir = parts[1];

                        let target = if dir == "~" {
                            env::var("HOME").unwrap()
                        } else {
                            dir.to_string()
                        };

                        if let Err(_) = env::set_current_dir(&target) {
                            println!("cd: {}: No such file or directory", dir);
                        }
                    }

                    _ => {
                        if let Some(path) = find_target(cmd) {
                            if let Err(error) =
                                Command::new(path).arg0(cmd).args(&parts[1..]).status()
                            {
                                eprintln!("{cmd}: {error}");
                            }
                        } else {
                            println!("{cmd}: command not found");
                        }
                    }
                }
            }
            Err(_) => {
                println!("Failed to read input");
            }
        }
    }
}
