use std::collections::BTreeSet;
use std::env;
use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::{Mutex, OnceLock};

use crate::parser::{CommandStage, ParsedCommand, RedirectTarget};

static JOBS: OnceLock<Mutex<Vec<Job>>> = OnceLock::new();

struct Job {
    number: usize,
    #[allow(dead_code)]
    pid: u32,
    command: String,
    status: JobStatus,
    child: Child,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum JobStatus {
    Running,
    Done,
}

pub enum CommandOutcome {
    Continue,
    Exit,
}

pub fn execute(parsed: &ParsedCommand) -> io::Result<CommandOutcome> {
    match parsed.stages.as_slice() {
        [] => Ok(CommandOutcome::Continue),
        [stage] => execute_stage(stage, parsed.run_in_background),
        stages => execute_pipeline(stages),
    }
}

pub fn run_pipeline_builtin(arguments: Vec<String>) -> io::Result<()> {
    let Some((command, arguments)) = arguments.split_first() else {
        return Ok(());
    };

    let mut stdout = io::stdout();
    let mut stderr = io::stderr();
    let _ = execute_builtin(command, arguments, &mut stdout, &mut stderr, false)?;
    Ok(())
}

fn execute_stage(stage: &CommandStage, run_in_background: bool) -> io::Result<CommandOutcome> {
    let Some((command, arguments)) = stage.arguments.split_first() else {
        return Ok(CommandOutcome::Continue);
    };

    let mut stdout_file = open_redirection(stage.stdout.as_ref())?;
    let mut stderr_file = open_redirection(stage.stderr.as_ref())?;
    let mut terminal_stdout = io::stdout();
    let mut terminal_stderr = io::stderr();

    if let Some(outcome) = execute_builtin(
        command,
        arguments,
        output_writer(&mut stdout_file, &mut terminal_stdout),
        output_writer(&mut stderr_file, &mut terminal_stderr),
        true,
    )? {
        return Ok(outcome);
    }

    // Every other command is resolved and launched from PATH.
    execute_external(
        command,
        arguments,
        stdout_file,
        stderr_file,
        run_in_background,
    )
}

fn execute_builtin(
    command: &str,
    arguments: &[String],
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
    exit_shell: bool,
) -> io::Result<Option<CommandOutcome>> {
    let outcome = match command {
        // Exit is handled by the shell loop instead of an external process.
        "exit" => {
            if exit_shell {
                CommandOutcome::Exit
            } else {
                CommandOutcome::Continue
            }
        }
        // Builtins write through the selected stdout/stderr destination.
        "echo" => {
            writeln!(stdout, "{}", arguments.join(" "))?;
            CommandOutcome::Continue
        }
        "type" => {
            execute_type(arguments.first().map(String::as_str), stdout)?;
            CommandOutcome::Continue
        }
        "jobs" => {
            execute_jobs(stdout)?;
            CommandOutcome::Continue
        }
        "pwd" => {
            let current_dir = env::current_dir().expect("current directory should be available");
            writeln!(stdout, "{}", current_dir.display())?;
            CommandOutcome::Continue
        }
        "cd" => {
            execute_cd(arguments.first().map(String::as_str), stderr)?;
            CommandOutcome::Continue
        }
        _ => return Ok(None),
    };

    Ok(Some(outcome))
}

fn execute_type(target: Option<&str>, output: &mut dyn Write) -> io::Result<()> {
    let Some(target) = target else {
        return Ok(());
    };

    // Builtins are reported directly.
    if is_builtin(target) {
        writeln!(output, "{target} is a shell builtin")?;
    // External commands report their resolved executable path.
    } else if let Some(path) = find_executable(target) {
        writeln!(output, "{target} is {}", path.display())?;
    // Unknown names match the shell's not-found response.
    } else {
        writeln!(output, "{target}: not found")?;
    }

    Ok(())
}

fn execute_cd(directory: Option<&str>, error: &mut dyn Write) -> io::Result<()> {
    let Some(directory) = directory else {
        return Ok(());
    };

    // Expand the home shortcut before changing directories.
    let target = if directory == "~" {
        env::var("HOME").expect("HOME should be set")
    // Otherwise use the path exactly as entered.
    } else {
        directory.to_owned()
    };

    // Keep cd failures as shell-style diagnostics rather than aborting.
    if env::set_current_dir(target).is_err() {
        writeln!(error, "cd: {directory}: No such file or directory")?;
    }

    Ok(())
}

fn execute_external(
    command: &str,
    arguments: &[String],
    stdout_file: Option<File>,
    mut stderr_file: Option<File>,
    run_in_background: bool,
) -> io::Result<CommandOutcome> {
    let Some(path) = find_executable(command) else {
        let mut terminal_stderr = io::stderr();
        writeln!(
            output_writer(&mut stderr_file, &mut terminal_stderr),
            "{command}: command not found"
        )?;
        return Ok(CommandOutcome::Continue);
    };

    let mut process = Command::new(path);
    process.arg0(command).args(arguments);

    configure_stdio(&mut process, stdout_file, stderr_file);

    // Background commands start and return control to the shell immediately.
    if run_in_background {
        let child = process.spawn()?;
        let pid = child.id();
        let job_number = track_job(Job {
            number: 0,
            pid,
            command: display_command(command, arguments),
            status: JobStatus::Running,
            child,
        });
        println!("[{job_number}] {pid}");
    } else {
        process.status()?;
    }

    Ok(CommandOutcome::Continue)
}

fn execute_pipeline(stages: &[CommandStage]) -> io::Result<CommandOutcome> {
    if stages.iter().any(|stage| stage.arguments.is_empty()) {
        return Ok(CommandOutcome::Continue);
    }

    let mut children = Vec::with_capacity(stages.len());
    let mut previous_stdout = None;

    for (index, stage) in stages.iter().enumerate() {
        let pipe_stdout = index + 1 < stages.len();
        let mut child = spawn_pipeline_stage(stage, previous_stdout.take(), pipe_stdout)?;

        previous_stdout = if pipe_stdout {
            Some(
                child
                    .stdout
                    .take()
                    .ok_or_else(|| io::Error::other("pipeline stage missing stdout"))?,
            )
        } else {
            None
        };

        children.push(child);
    }

    for mut child in children.into_iter().rev() {
        child.wait()?;
    }

    Ok(CommandOutcome::Continue)
}

fn spawn_pipeline_stage(
    stage: &CommandStage,
    stdin: Option<ChildStdout>,
    pipe_stdout: bool,
) -> io::Result<Child> {
    let Some((command, arguments)) = stage.arguments.split_first() else {
        return Err(io::Error::other("pipeline stage missing command"));
    };
    let path = if is_builtin(command) {
        env::current_exe()?
    } else {
        find_executable(command)
            .ok_or_else(|| io::Error::other(format!("{command}: command not found")))?
    };

    let mut process = Command::new(path);
    if is_builtin(command) {
        process.arg("--pipeline-builtin").arg(command).args(arguments);
    } else {
        process.arg0(command).args(arguments);
    }

    configure_pipeline_stdio(&mut process, stage, stdin, pipe_stdout)?;
    process.spawn()
}

fn configure_pipeline_stdio(
    process: &mut Command,
    stage: &CommandStage,
    stdin: Option<ChildStdout>,
    pipe_stdout: bool,
) -> io::Result<()> {
    if let Some(source) = stdin {
        process.stdin(Stdio::from(source));
    }

    if pipe_stdout {
        process.stdout(Stdio::piped());
    } else if let Some(file) = open_redirection(stage.stdout.as_ref())? {
        process.stdout(Stdio::from(file));
    } else {
        process.stdout(Stdio::inherit());
    }

    if let Some(file) = open_redirection(stage.stderr.as_ref())? {
        process.stderr(Stdio::from(file));
    } else {
        process.stderr(Stdio::inherit());
    }

    Ok(())
}

fn configure_stdio(process: &mut Command, stdout_file: Option<File>, stderr_file: Option<File>) {
    // External commands share the shell terminal unless a redirection overrides it.
    process.stdout(match stdout_file {
        Some(file) => Stdio::from(file),
        None => Stdio::inherit(),
    });
    process.stderr(match stderr_file {
        Some(file) => Stdio::from(file),
        None => Stdio::inherit(),
    });
}

fn execute_jobs(output: &mut dyn Write) -> io::Result<()> {
    let mut jobs = jobs().lock().expect("job list mutex should not be poisoned");
    refresh_job_statuses(&mut jobs)?;

    for (index, job) in jobs.iter().enumerate() {
        writeln!(
            output,
            "[{}]{}  {:<24}{}",
            job.number,
            marker_for(index, jobs.len()),
            job.status.as_str(),
            job.display_command()
        )?;
    }

    jobs.retain(|job| !job.is_done());

    Ok(())
}

pub fn reap_completed_jobs(output: &mut dyn Write) -> io::Result<()> {
    let mut jobs = jobs().lock().expect("job list mutex should not be poisoned");
    refresh_job_statuses(&mut jobs)?;

    for (index, job) in jobs.iter().enumerate() {
        if job.is_done() {
            writeln!(
                output,
                "[{}]{}  {:<24}{}",
                job.number,
                marker_for(index, jobs.len()),
                job.status.as_str(),
                job.display_command()
            )?;
        }
    }

    jobs.retain(|job| !job.is_done());

    Ok(())
}

fn track_job(mut job: Job) -> usize {
    let mut jobs = jobs()
        .lock()
        .expect("job list mutex should not be poisoned");
    let number = next_job_number(&jobs);
    job.number = number;
    jobs.push(job);
    number
}

fn jobs() -> &'static Mutex<Vec<Job>> {
    JOBS.get_or_init(|| Mutex::new(Vec::new()))
}

fn display_command(command: &str, arguments: &[String]) -> String {
    let mut display = String::from(command);

    if !arguments.is_empty() {
        display.push(' ');
        display.push_str(&arguments.join(" "));
    }

    display
}

fn marker_for(index: usize, total_jobs: usize) -> char {
    match total_jobs.saturating_sub(index) {
        1 => '+',
        2 => '-',
        _ => ' ',
    }
}

fn next_job_number(jobs: &[Job]) -> usize {
    jobs.iter()
        .map(|job| job.number)
        .max()
        .map_or(1, |highest| highest + 1)
}

impl JobStatus {
    fn as_str(self) -> &'static str {
        match self {
            JobStatus::Running => "Running",
            JobStatus::Done => "Done",
        }
    }
}

impl Job {
    fn refresh_status(&mut self) -> io::Result<()> {
        if self.status == JobStatus::Done {
            return Ok(());
        }

        if exited_normally(self.child.try_wait()?) {
            self.status = JobStatus::Done;
        }

        Ok(())
    }

    fn display_command(&self) -> String {
        match self.status {
            JobStatus::Running => format!("{} &", self.command),
            JobStatus::Done => self.command.clone(),
        }
    }

    fn is_done(&self) -> bool {
        matches!(self.status, JobStatus::Done)
    }
}

fn refresh_job_statuses(jobs: &mut [Job]) -> io::Result<()> {
    for job in jobs {
        job.refresh_status()?;
    }

    Ok(())
}

fn exited_normally(status: Option<ExitStatus>) -> bool {
    matches!(status, Some(exit_status) if exit_status.success() || exit_status.code().is_some())
}

// Uses the redirected file when present, otherwise the terminal
fn output_writer<'a>(
    redirected: &'a mut Option<File>,
    terminal: &'a mut dyn Write,
) -> &'a mut dyn Write {
    // Prefer the file; otherwise write to the terminal stream.
    match redirected {
        Some(file) => file,
        None => terminal,
    }
}

fn open_redirection(target: Option<&RedirectTarget>) -> io::Result<Option<File>> {
    target
        .map(|target| {
            let mut options = OpenOptions::new();
            options.create(true).write(true);

            // Append preserves existing bytes; truncate replaces them.
            if target.append {
                options.append(true);
            } else {
                options.truncate(true);
            }

            options.open(&target.path)
        })
        .transpose()
}

fn is_builtin(command: &str) -> bool {
    matches!(command, "echo" | "exit" | "type" | "jobs" | "pwd" | "cd")
}

fn find_executable(target: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;

    env::split_paths(&path).find_map(|directory| {
        let candidate = directory.join(target);
        is_executable(&candidate).then_some(candidate)
    })
}

// Lists executable PATH entries that begin with the requested prefix.
pub(crate) fn executable_names(prefix: &str, path: Option<&OsStr>) -> Vec<String> {
    // No PATH means there are no external completion candidates.
    let Some(path) = path else {
        return Vec::new();
    };

    // Ignore missing directories and unreadable entries while scanning PATH.
    env::split_paths(path)
        .filter_map(|directory| fs::read_dir(directory).ok())
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            (name.starts_with(prefix) && is_executable(&entry.path())).then_some(name)
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

// Checks that a path is a regular file with at least one execute bit set.
fn is_executable(path: &Path) -> bool {
    fs::metadata(path)
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    use super::executable_names;

    #[test]
    fn finds_executable_prefix_when_path_contains_a_missing_directory() {
        let directory = env::temp_dir().join(format!(
            "codecrafters-shell-completion-{}",
            std::process::id()
        ));
        fs::create_dir_all(&directory).expect("temporary directory should be created");

        let executable = directory.join("custom_executable");
        fs::write(&executable, "").expect("temporary executable should be created");
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755))
            .expect("temporary executable should be executable");

        let missing_directory = directory.join("missing");
        let path = env::join_paths([missing_directory, directory.clone()])
            .expect("test PATH should be valid");

        assert_eq!(
            executable_names("custom", Some(&path)),
            vec!["custom_executable"]
        );

        fs::remove_dir_all(directory).expect("temporary directory should be removed");
    }
}
