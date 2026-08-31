use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::fd::OwnedFd;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

#[derive(Debug)]
struct ShellOutput {
    stdout: String,
    stderr: String,
}

fn run_shell_output(input: &str) -> ShellOutput {
    let mut child = Command::new(env!("CARGO_BIN_EXE_rusty_shell"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("shell binary should start");

    child
        .stdin
        .as_mut()
        .expect("stdin should be piped")
        .write_all(input.as_bytes())
        .expect("test input should be written");

    let output = child.wait_with_output().expect("shell should exit");
    assert!(output.status.success());
    ShellOutput {
        stdout: String::from_utf8(output.stdout).expect("shell stdout should be UTF-8"),
        stderr: String::from_utf8(output.stderr).expect("shell stderr should be UTF-8"),
    }
}

fn run_shell(input: &str) -> String {
    run_shell_output(input).stdout
}

fn run_shell_output_with_path(input: &str, path: &str) -> ShellOutput {
    let mut child = Command::new(env!("CARGO_BIN_EXE_rusty_shell"))
        .env("PATH", path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("shell binary should start");

    child
        .stdin
        .as_mut()
        .expect("stdin should be piped")
        .write_all(input.as_bytes())
        .expect("test input should be written");

    let output = child.wait_with_output().expect("shell should exit");
    assert!(output.status.success());
    ShellOutput {
        stdout: String::from_utf8(output.stdout).expect("shell stdout should be UTF-8"),
        stderr: String::from_utf8(output.stderr).expect("shell stderr should be UTF-8"),
    }
}

fn run_shell_output_with_env(input: &str, environment: &[(&str, &str)]) -> ShellOutput {
    let mut child = Command::new(env!("CARGO_BIN_EXE_rusty_shell"));
    child
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    for (key, value) in environment {
        child.env(key, value);
    }

    let mut child = child.spawn().expect("shell binary should start");

    child
        .stdin
        .as_mut()
        .expect("stdin should be piped")
        .write_all(input.as_bytes())
        .expect("test input should be written");

    let output = child.wait_with_output().expect("shell should exit");
    assert!(output.status.success());
    ShellOutput {
        stdout: String::from_utf8(output.stdout).expect("shell stdout should be UTF-8"),
        stderr: String::from_utf8(output.stderr).expect("shell stderr should be UTF-8"),
    }
}

fn temporary_bin(name: &str, mappings: &[(&str, &str)]) -> PathBuf {
    let directory = temporary_path(name);
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).expect("temporary bin directory should be created");

    for (link_name, target) in mappings {
        std::os::unix::fs::symlink(target, directory.join(link_name))
            .expect("temporary symlink should be created");
    }

    directory
}

fn read_prompt_line(reader: &mut BufReader<impl std::io::Read>) -> String {
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .expect("shell stdout should be readable");
    line
}

fn read_prompt(reader: &mut BufReader<impl std::io::Read>) -> String {
    let mut prompt = [0_u8; 2];
    reader
        .read_exact(&mut prompt)
        .expect("prompt should be readable");
    String::from_utf8(prompt.to_vec()).expect("prompt should be valid UTF-8")
}

fn read_until_contains(reader: &mut BufReader<impl std::io::Read>, needle: &str) -> String {
    let mut output = String::new();

    while !output.contains(needle) {
        let mut byte = [0_u8; 1];
        reader
            .read_exact(&mut byte)
            .expect("shell output should remain readable");
        output.push(byte[0] as char);
    }

    output
}

fn output_lines(output: &str) -> Vec<&str> {
    output.lines().collect()
}

fn temporary_file(name: &str, contents: &str) -> PathBuf {
    let path =
        std::env::temp_dir().join(format!("codecrafters-shell-{}-{name}", std::process::id()));
    fs::write(&path, contents).expect("temporary fixture should be written");
    path
}

fn temporary_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("codecrafters-shell-{}-{name}", std::process::id()))
}

fn temporary_fifo(name: &str) -> PathBuf {
    let path = temporary_path(name);
    let _ = fs::remove_file(&path);

    let status = Command::new("mkfifo")
        .arg(&path)
        .status()
        .expect("mkfifo should run");
    assert!(status.success(), "mkfifo should succeed");

    path
}

fn temporary_executable(name: &str, contents: &str) -> PathBuf {
    let path = temporary_file(name, contents);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
        .expect("temporary executable should be executable");
    path
}

#[test]
fn unquoted_whitespace_separates_arguments() {
    assert_eq!(
        run_shell("echo hello    world\nexit\n"),
        "$ hello world\n$ "
    );
}

#[test]
fn reports_unknown_commands() {
    let output = run_shell_output("definitely-not-a-shell-command\nexit\n");

    assert_eq!(output.stdout, "$ $ ");
    assert_eq!(
        output.stderr,
        "definitely-not-a-shell-command: command not found\n"
    );
}

#[test]
fn type_recognizes_builtins() {
    assert_eq!(
        run_shell("type echo\nexit\n"),
        "$ echo is a shell builtin\n$ "
    );
}

#[test]
fn type_recognizes_jobs_as_a_builtin() {
    assert_eq!(
        run_shell("type jobs\nexit\n"),
        "$ jobs is a shell builtin\n$ "
    );
}

#[test]
fn type_recognizes_history_as_a_builtin() {
    assert_eq!(
        run_shell("type history\nexit\n"),
        "$ history is a shell builtin\n$ "
    );
}

#[test]
fn type_recognizes_declare_as_a_builtin() {
    assert_eq!(
        run_shell("type declare\nexit\n"),
        "$ declare is a shell builtin\n$ "
    );
}

#[test]
fn jobs_builtin_produces_no_output_when_no_jobs_exist() {
    assert_eq!(run_shell("jobs\nexit\n"), "$ $ ");
}

#[test]
fn history_lists_previously_executed_commands_in_order() {
    let output = run_shell_output("echo hello\necho world\ninvalid_command\nhistory\nexit\n");

    assert_eq!(
        output.stdout,
        "$ hello\n$ world\n$ $     1  echo hello\n    2  echo world\n    3  invalid_command\n    4  history\n$ "
    );
    assert_eq!(output.stderr, "invalid_command: command not found\n");
}

#[test]
fn history_with_a_limit_shows_only_the_last_n_commands() {
    let output = run_shell_output("echo hello\necho world\ninvalid_command\nhistory 2\nexit\n");

    assert_eq!(
        output.stdout,
        "$ hello\n$ world\n$ $     3  invalid_command\n    4  history 2\n$ "
    );
    assert_eq!(output.stderr, "invalid_command: command not found\n");
}

#[test]
fn history_read_appends_commands_from_a_file() {
    let history_path = temporary_file("history-read", "echo hello\necho world\n\n");
    let output = run_shell_output(&format!(
        "history -r {}\nhistory\nexit\n",
        history_path.display()
    ));

    assert_eq!(
        output.stdout,
        format!(
            "$ $     1  history -r {}\n    2  echo hello\n    3  echo world\n    4  history\n$ ",
            history_path.display()
        )
    );
    assert_eq!(output.stderr, "");

    fs::remove_file(history_path).expect("temporary history file should be removed");
}

#[test]
fn history_write_persists_in_memory_commands_to_a_file() {
    let history_path = temporary_path("history-write");
    let output = run_shell_output(&format!(
        "echo hello\necho world\nhistory -w {}\nexit\n",
        history_path.display()
    ));

    assert_eq!(output.stdout, "$ hello\n$ world\n$ $ ");
    assert_eq!(output.stderr, "");
    assert_eq!(
        fs::read_to_string(&history_path).expect("written history file should be readable"),
        format!(
            "echo hello\necho world\nhistory -w {}\n",
            history_path.display()
        )
    );

    fs::remove_file(history_path).expect("temporary history file should be removed");
}

#[test]
fn history_append_writes_only_commands_since_the_last_append() {
    let history_path = temporary_file("history-append", "echo initial_command_1\necho initial_command_2\n");
    let output = run_shell_output(&format!(
        "echo new_command\nhistory -a {}\nhistory -a {}\nexit\n",
        history_path.display(),
        history_path.display()
    ));

    assert_eq!(output.stdout, "$ new_command\n$ $ $ ");
    assert_eq!(output.stderr, "");
    assert_eq!(
        fs::read_to_string(&history_path).expect("appended history file should be readable"),
        format!(
            "echo initial_command_1\necho initial_command_2\necho new_command\nhistory -a {}\nhistory -a {}\n",
            history_path.display(),
            history_path.display()
        )
    );

    fs::remove_file(history_path).expect("temporary history file should be removed");
}

#[test]
fn history_loads_from_histfile_on_startup() {
    let history_path = temporary_file("histfile-startup", "echo hello\necho world\n");
    let histfile = history_path.display().to_string();
    let output = run_shell_output_with_env("history\nexit\n", &[("HISTFILE", &histfile)]);

    assert_eq!(
        output.stdout,
        "$     1  echo hello\n    2  echo world\n    3  history\n$ "
    );
    assert_eq!(output.stderr, "");

    fs::remove_file(history_path).expect("temporary history file should be removed");
}

#[test]
fn history_appends_session_commands_to_histfile_on_exit() {
    let history_path = temporary_path("histfile-exit-write");
    let histfile = history_path.display().to_string();
    let output = run_shell_output_with_env(
        "echo hello\necho world\nexit\n",
        &[("HISTFILE", &histfile)],
    );

    assert_eq!(output.stdout, "$ hello\n$ world\n$ ");
    assert_eq!(output.stderr, "");
    assert_eq!(
        fs::read_to_string(&history_path).expect("history file should be readable"),
        "echo hello\necho world\nexit\n"
    );

    fs::remove_file(history_path).expect("temporary history file should be removed");
}

#[test]
fn history_appends_new_session_commands_to_existing_histfile_on_exit() {
    let history_path =
        temporary_file("histfile-exit-append", "echo initial_command_1\necho initial_command_2\n");
    let histfile = history_path.display().to_string();
    let output = run_shell_output_with_env("echo new_command\nexit\n", &[("HISTFILE", &histfile)]);

    assert_eq!(output.stdout, "$ new_command\n$ ");
    assert_eq!(output.stderr, "");
    assert_eq!(
        fs::read_to_string(&history_path).expect("history file should be readable"),
        "echo initial_command_1\necho initial_command_2\necho new_command\nexit\n"
    );

    fs::remove_file(history_path).expect("temporary history file should be removed");
}

#[test]
fn declare_stores_and_prints_a_variable() {
    let output = run_shell_output("declare foo=bar\ndeclare -p foo\nexit\n");

    assert_eq!(output.stdout, "$ $ declare -- foo=\"bar\"\n$ ");
    assert_eq!(output.stderr, "");
}

#[test]
fn declare_replaces_an_existing_variable_value() {
    let output =
        run_shell_output("declare foo=bar\ndeclare foo=updated\ndeclare -p foo\nexit\n");

    assert_eq!(output.stdout, "$ $ $ declare -- foo=\"updated\"\n$ ");
    assert_eq!(output.stderr, "");
}

#[test]
fn declare_reports_missing_variables() {
    let output = run_shell_output("declare -p missing_variable\nexit\n");

    assert_eq!(output.stdout, "$ $ ");
    assert_eq!(output.stderr, "declare: missing_variable: not found\n");
}

#[test]
fn declare_rejects_invalid_identifiers() {
    let output = run_shell_output("declare 23=x\nexit\n");

    assert_eq!(output.stdout, "$ $ ");
    assert_eq!(output.stderr, "declare: `23=x': not a valid identifier\n");
}

#[test]
fn declare_accepts_underscores_and_digits_after_the_first_character() {
    let output = run_shell_output("declare _FOO123=BAR\ndeclare -p _FOO123\nexit\n");

    assert_eq!(output.stdout, "$ $ declare -- _FOO123=\"BAR\"\n$ ");
    assert_eq!(output.stderr, "");
}

#[test]
fn parameter_expansion_replaces_simple_variables_for_builtins() {
    let output = run_shell_output("declare Item=widget\ndeclare Foo1=Bar2\necho $Item\necho ${Item}_id\necho start_${Item}_end\necho ${Item}and${Foo1}\necho ${missing}world\nexit\n");

    assert_eq!(
        output.stdout,
        "$ $ $ widget\n$ widget_id\n$ start_widget_end\n$ widgetandBar2\n$ world\n$ "
    );
    assert_eq!(output.stderr, "");
}

#[test]
fn parameter_expansion_replaces_variables_for_external_commands() {
    let script = temporary_executable(
        "print-args.sh",
        "#!/usr/bin/env python3\nimport sys\nfor index, argument in enumerate(sys.argv[1:], start=1):\n    print(f'Arg #{index}: {argument}')\n",
    );
    let output = run_shell_output(&format!(
        "declare Variable_1=Value_1\ndeclare Variable_2=Value_2\n{} $Variable_1 $Variable_2\nexit\n",
        script.display()
    ));

    assert_eq!(
        output.stdout,
        "$ $ $ Arg #1: Value_1\nArg #2: Value_2\n$ "
    );
    assert_eq!(output.stderr, "");

    fs::remove_file(script).expect("temporary executable should be removed");
}

#[test]
fn parameter_expansion_removes_arguments_that_become_empty() {
    let script = temporary_executable(
        "print-args-empty.sh",
        "#!/usr/bin/env python3\nimport sys\nfor index, argument in enumerate(sys.argv[1:], start=1):\n    print(f'Arg #{index}: {argument}')\n",
    );
    let output = run_shell_output(&format!(
        "declare existing=existingsvalue\n{} ${{missing1}}end ${{existing}} ${{missing2}}\nexit\n",
        script.display()
    ));

    assert_eq!(
        output.stdout,
        "$ $ Arg #1: end\nArg #2: existingsvalue\n$ "
    );
    assert_eq!(output.stderr, "");

    fs::remove_file(script).expect("temporary executable should be removed");
}

#[test]
fn jobs_lists_a_single_running_background_job() {
    let output = run_shell("sleep 10 &\njobs\nexit\n");
    let lines = output_lines(&output);

    assert_eq!(lines.len(), 3, "unexpected output: {output:?}");
    assert!(lines[0].starts_with("$ [1] "), "unexpected output: {output:?}");
    assert_eq!(lines[1], "$ [1]+  Running                 sleep 10 &");
    assert_eq!(lines[2], "$ ");
}

#[test]
fn jobs_lists_multiple_background_jobs_in_start_order_with_markers() {
    let output = run_shell("sleep 10 &\njobs\nsleep 20 &\njobs\nsleep 30 &\njobs\nexit\n");
    let lines = output_lines(&output);

    assert_eq!(lines.len(), 10, "unexpected output: {output:?}");
    assert!(lines[0].starts_with("$ [1] "), "unexpected output: {output:?}");
    assert_eq!(lines[1], "$ [1]+  Running                 sleep 10 &");
    assert!(lines[2].starts_with("$ [2] "), "unexpected output: {output:?}");
    assert_eq!(lines[3], "$ [1]-  Running                 sleep 10 &");
    assert_eq!(lines[4], "[2]+  Running                 sleep 20 &");
    assert!(lines[5].starts_with("$ [3] "), "unexpected output: {output:?}");
    assert_eq!(lines[6], "$ [1]   Running                 sleep 10 &");
    assert_eq!(lines[7], "[2]-  Running                 sleep 20 &");
    assert_eq!(lines[8], "[3]+  Running                 sleep 30 &");
    assert_eq!(lines[9], "$ ");
}

#[test]
fn jobs_reports_done_once_and_then_removes_a_completed_background_job() {
    let fifo = temporary_fifo("single-reap.fifo");
    let mut child = Command::new(env!("CARGO_BIN_EXE_rusty_shell"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("shell binary should start");
    let mut stdout = BufReader::new(child.stdout.take().expect("stdout should be piped"));

    child
        .stdin
        .as_mut()
        .expect("stdin should be piped")
        .write_all(format!("cat {} &\n", fifo.display()).as_bytes())
        .expect("background command should be written");

    assert_eq!(read_prompt(&mut stdout), "$ ");
    let background_job_line = read_prompt_line(&mut stdout);
    assert!(
        background_job_line.starts_with("[1] "),
        "unexpected job line: {background_job_line:?}"
    );
    assert_eq!(read_prompt(&mut stdout), "$ ");

    child
        .stdin
        .as_mut()
        .expect("stdin should be piped")
        .write_all(b"jobs\n")
        .expect("jobs command should be written");

    let running_output = read_until_contains(
        &mut stdout,
        &format!("Running                 cat {} &\n", fifo.display()),
    );
    assert_eq!(
        running_output,
        format!("[1]+  Running                 cat {} &\n", fifo.display())
    );
    assert_eq!(read_prompt(&mut stdout), "$ ");

    fs::write(&fifo, "").expect("fifo should receive EOF");
    thread::sleep(Duration::from_millis(100));

    child
        .stdin
        .as_mut()
        .expect("stdin should be piped")
        .write_all(b"jobs\n")
        .expect("jobs command should be written");

    let done_output = read_until_contains(
        &mut stdout,
        &format!("Done                    cat {}\n", fifo.display()),
    );
    assert_eq!(
        done_output,
        format!("[1]+  Done                    cat {}\n", fifo.display())
    );
    assert_eq!(read_prompt(&mut stdout), "$ ");

    child
        .stdin
        .as_mut()
        .expect("stdin should be piped")
        .write_all(b"jobs\nexit\n")
        .expect("final commands should be written");

    assert_eq!(read_prompt(&mut stdout), "$ ");

    let status = child.wait().expect("shell should exit");
    assert!(status.success());

    fs::remove_file(fifo).expect("fifo should be removed");
}

#[test]
fn jobs_reaps_multiple_completed_background_jobs_and_recalculates_markers() {
    let first_fifo = temporary_fifo("multi-reap-first.fifo");
    let second_fifo = temporary_fifo("multi-reap-second.fifo");
    let mut child = Command::new(env!("CARGO_BIN_EXE_rusty_shell"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("shell binary should start");
    let mut stdout = BufReader::new(child.stdout.take().expect("stdout should be piped"));

    child
        .stdin
        .as_mut()
        .expect("stdin should be piped")
        .write_all(
            format!(
                "sleep 500 &\ncat {} &\ncat {} &\n",
                first_fifo.display(),
                second_fifo.display()
            )
            .as_bytes(),
        )
        .expect("background commands should be written");

    for expected_job in 1..=3 {
        assert_eq!(read_prompt(&mut stdout), "$ ");
        let job_line = read_prompt_line(&mut stdout);
        assert!(
            job_line.starts_with(&format!("[{expected_job}] ")),
            "unexpected job line: {job_line:?}"
        );
    }
    assert_eq!(read_prompt(&mut stdout), "$ ");

    fs::write(&first_fifo, "").expect("first fifo should receive EOF");
    thread::sleep(Duration::from_millis(100));

    child
        .stdin
        .as_mut()
        .expect("stdin should be piped")
        .write_all(b"jobs\n")
        .expect("jobs command should be written");

    let first_jobs_output = read_until_contains(
        &mut stdout,
        &format!("cat {} &\n", second_fifo.display()),
    );
    assert_eq!(
        first_jobs_output,
        format!(
            "[1]   Running                 sleep 500 &\n[2]-  Done                    cat {}\n[3]+  Running                 cat {} &\n",
            first_fifo.display(),
            second_fifo.display()
        )
    );
    assert_eq!(read_prompt(&mut stdout), "$ ");

    fs::write(&second_fifo, "").expect("second fifo should receive EOF");
    thread::sleep(Duration::from_millis(100));

    child
        .stdin
        .as_mut()
        .expect("stdin should be piped")
        .write_all(b"jobs\n")
        .expect("jobs command should be written");

    let second_jobs_output = read_until_contains(
        &mut stdout,
        &format!("Done                    cat {}\n", second_fifo.display()),
    );
    assert_eq!(
        second_jobs_output,
        format!(
            "[1]-  Running                 sleep 500 &\n[3]+  Done                    cat {}\n",
            second_fifo.display()
        )
    );
    assert_eq!(read_prompt(&mut stdout), "$ ");

    child
        .stdin
        .as_mut()
        .expect("stdin should be piped")
        .write_all(b"jobs\nexit\n")
        .expect("final commands should be written");

    let final_jobs_output =
        read_until_contains(&mut stdout, "Running                 sleep 500 &\n");
    assert_eq!(final_jobs_output, "[1]+  Running                 sleep 500 &\n");
    assert_eq!(read_prompt(&mut stdout), "$ ");

    let status = child.wait().expect("shell should exit");
    assert!(status.success());

    fs::remove_file(first_fifo).expect("first fifo should be removed");
    fs::remove_file(second_fifo).expect("second fifo should be removed");
}

#[test]
fn completed_jobs_are_reaped_before_the_next_prompt() {
    let fifo = temporary_fifo("prompt-reap.fifo");
    let mut child = Command::new(env!("CARGO_BIN_EXE_rusty_shell"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("shell binary should start");
    let mut stdout = BufReader::new(child.stdout.take().expect("stdout should be piped"));

    child
        .stdin
        .as_mut()
        .expect("stdin should be piped")
        .write_all(format!("sleep 500 &\ncat {} &\n", fifo.display()).as_bytes())
        .expect("background commands should be written");

    for expected_job in 1..=2 {
        assert_eq!(read_prompt(&mut stdout), "$ ");
        let job_line = read_prompt_line(&mut stdout);
        assert!(
            job_line.starts_with(&format!("[{expected_job}] ")),
            "unexpected job line: {job_line:?}"
        );
    }
    assert_eq!(read_prompt(&mut stdout), "$ ");

    child
        .stdin
        .as_mut()
        .expect("stdin should be piped")
        .write_all(b"jobs\n")
        .expect("jobs command should be written");

    let initial_jobs_output = read_until_contains(
        &mut stdout,
        &format!("cat {} &\n", fifo.display()),
    );
    assert_eq!(
        initial_jobs_output,
        format!(
            "[1]-  Running                 sleep 500 &\n[2]+  Running                 cat {} &\n",
            fifo.display()
        )
    );
    assert_eq!(read_prompt(&mut stdout), "$ ");

    fs::write(&fifo, "").expect("fifo should receive EOF");
    thread::sleep(Duration::from_millis(100));

    child
        .stdin
        .as_mut()
        .expect("stdin should be piped")
        .write_all(b"echo banana\n")
        .expect("echo command should be written");

    let command_and_reap_output = read_until_contains(
        &mut stdout,
        &format!("Done                    cat {}\n", fifo.display()),
    );
    assert_eq!(
        command_and_reap_output,
        format!("banana\n[2]+  Done                    cat {}\n", fifo.display())
    );
    assert_eq!(read_prompt(&mut stdout), "$ ");

    child
        .stdin
        .as_mut()
        .expect("stdin should be piped")
        .write_all(b"jobs\nexit\n")
        .expect("final commands should be written");

    let remaining_jobs_output =
        read_until_contains(&mut stdout, "Running                 sleep 500 &\n");
    assert_eq!(remaining_jobs_output, "[1]+  Running                 sleep 500 &\n");
    assert_eq!(read_prompt(&mut stdout), "$ ");

    let status = child.wait().expect("shell should exit");
    assert!(status.success());

    fs::remove_file(fifo).expect("fifo should be removed");
}

#[test]
fn job_numbers_recycle_to_one_when_all_jobs_have_been_reaped() {
    let fifo = temporary_fifo("recycle-empty.fifo");
    let mut child = Command::new(env!("CARGO_BIN_EXE_rusty_shell"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("shell binary should start");
    let mut stdout = BufReader::new(child.stdout.take().expect("stdout should be piped"));

    child
        .stdin
        .as_mut()
        .expect("stdin should be piped")
        .write_all(format!("cat {} &\n", fifo.display()).as_bytes())
        .expect("background command should be written");

    assert_eq!(read_prompt(&mut stdout), "$ ");
    let first_job_line = read_prompt_line(&mut stdout);
    assert!(
        first_job_line.starts_with("[1] "),
        "unexpected job line: {first_job_line:?}"
    );
    assert_eq!(read_prompt(&mut stdout), "$ ");

    fs::write(&fifo, "").expect("fifo should receive EOF");
    thread::sleep(Duration::from_millis(100));

    child
        .stdin
        .as_mut()
        .expect("stdin should be piped")
        .write_all(b"echo apple\n")
        .expect("echo command should be written");

    let reap_output = read_until_contains(
        &mut stdout,
        &format!("Done                    cat {}\n", fifo.display()),
    );
    assert_eq!(
        reap_output,
        format!("apple\n[1]+  Done                    cat {}\n", fifo.display())
    );
    assert_eq!(read_prompt(&mut stdout), "$ ");

    child
        .stdin
        .as_mut()
        .expect("stdin should be piped")
        .write_all(b"sleep 100 &\njobs\nexit\n")
        .expect("final commands should be written");

    let recycled_job_line = read_prompt_line(&mut stdout);
    assert!(
        recycled_job_line.starts_with("[1] "),
        "unexpected job line: {recycled_job_line:?}"
    );
    let recycled_jobs_output =
        read_until_contains(&mut stdout, "Running                 sleep 100 &\n");
    assert_eq!(recycled_jobs_output, "$ [1]+  Running                 sleep 100 &\n");
    assert_eq!(read_prompt(&mut stdout), "$ ");

    let status = child.wait().expect("shell should exit");
    assert!(status.success());

    fs::remove_file(fifo).expect("fifo should be removed");
}

#[test]
fn job_numbers_reuse_the_highest_completed_slot_when_older_jobs_remain() {
    let fifo = temporary_fifo("recycle-partial.fifo");
    let mut child = Command::new(env!("CARGO_BIN_EXE_rusty_shell"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("shell binary should start");
    let mut stdout = BufReader::new(child.stdout.take().expect("stdout should be piped"));

    child
        .stdin
        .as_mut()
        .expect("stdin should be piped")
        .write_all(format!("sleep 100 &\ncat {} &\n", fifo.display()).as_bytes())
        .expect("background commands should be written");

    for expected_job in 1..=2 {
        assert_eq!(read_prompt(&mut stdout), "$ ");
        let job_line = read_prompt_line(&mut stdout);
        assert!(
            job_line.starts_with(&format!("[{expected_job}] ")),
            "unexpected job line: {job_line:?}"
        );
    }
    assert_eq!(read_prompt(&mut stdout), "$ ");

    fs::write(&fifo, "").expect("fifo should receive EOF");
    thread::sleep(Duration::from_millis(100));

    child
        .stdin
        .as_mut()
        .expect("stdin should be piped")
        .write_all(b"echo word\n")
        .expect("echo command should be written");

    let reap_output = read_until_contains(
        &mut stdout,
        &format!("Done                    cat {}\n", fifo.display()),
    );
    assert_eq!(
        reap_output,
        format!("word\n[2]+  Done                    cat {}\n", fifo.display())
    );
    assert_eq!(read_prompt(&mut stdout), "$ ");

    child
        .stdin
        .as_mut()
        .expect("stdin should be piped")
        .write_all(b"sleep 50 &\njobs\nexit\n")
        .expect("final commands should be written");

    let recycled_job_line = read_prompt_line(&mut stdout);
    assert!(
        recycled_job_line.starts_with("[2] "),
        "unexpected job line: {recycled_job_line:?}"
    );
    let recycled_jobs_output =
        read_until_contains(&mut stdout, "Running                 sleep 50 &\n");
    assert_eq!(
        recycled_jobs_output,
        "$ [1]-  Running                 sleep 100 &\n[2]+  Running                 sleep 50 &\n"
    );
    assert_eq!(read_prompt(&mut stdout), "$ ");

    let status = child.wait().expect("shell should exit");
    assert!(status.success());

    fs::remove_file(fifo).expect("fifo should be removed");
}

#[test]
fn single_quotes_preserve_whitespace_and_special_characters() {
    assert_eq!(
        run_shell("echo 'hello    $world * ~ \\'\nexit\n"),
        "$ hello    $world * ~ \\\n$ "
    );
}

#[test]
fn adjacent_single_quoted_and_unquoted_segments_form_one_argument() {
    assert_eq!(
        run_shell("echo hello''world 'shell''test'\nexit\n"),
        "$ helloworld shelltest\n$ "
    );
}

#[test]
fn double_quotes_preserve_whitespace_and_treat_single_quotes_literally() {
    assert_eq!(
        run_shell("echo \"quz  hello\"  \"shell's\"\nexit\n"),
        "$ quz  hello shell's\n$ "
    );
}

#[test]
fn adjacent_double_quoted_and_unquoted_segments_form_one_argument() {
    assert_eq!(
        run_shell("echo \"hello\"\"world\" next\"door\"\nexit\n"),
        "$ helloworld nextdoor\n$ "
    );
}

#[test]
fn external_commands_receive_single_quoted_paths_as_single_arguments() {
    let path = temporary_file("single quoted file", "single quote content\n");
    let input = format!("cat '{}'\nexit\n", path.display());

    assert_eq!(run_shell(&input), "$ single quote content\n$ ");

    fs::remove_file(path).expect("temporary fixture should be removed");
}

#[test]
fn external_commands_receive_double_quoted_paths_as_single_arguments() {
    let path = temporary_file("double 'quoted' file", "double quote content\n");
    let input = format!("cat \"{}\"\nexit\n", path.display());

    assert_eq!(run_shell(&input), "$ double quote content\n$ ");

    fs::remove_file(path).expect("temporary fixture should be removed");
}

#[test]
fn single_quotes_keep_backslashes_literal() {
    assert_eq!(
        run_shell("echo 'multiple\\\\slashes' 'every\\\"thing_is\\\"literal'\nexit\n"),
        "$ multiple\\\\slashes every\\\"thing_is\\\"literal\n$ "
    );
}

#[test]
fn double_quotes_escape_quotes_and_backslashes() {
    assert_eq!(
        run_shell("echo \"A \\\\ escapes itself\" \"A \\\" inside double quotes\"\nexit\n"),
        "$ A \\ escapes itself A \" inside double quotes\n$ "
    );
}

#[test]
fn double_quotes_preserve_backslashes_before_ordinary_characters() {
    assert_eq!(
        run_shell("echo \"just'one'\\n'backslash\"\nexit\n"),
        "$ just'one'\\n'backslash\n$ "
    );
}

#[test]
fn escaped_quotes_can_join_double_quoted_and_unquoted_segments() {
    assert_eq!(
        run_shell("echo \"inside\\\"literal_quote.\"outside\\\"\nexit\n"),
        "$ inside\"literal_quote.outside\"\n$ "
    );
}

#[test]
fn cat_receives_literal_backslashes_from_single_quotes() {
    let path = temporary_file("one slash \\2", "single backslash content\n");
    let input = format!("cat '{}'\nexit\n", path.display());

    assert_eq!(run_shell(&input), "$ single backslash content\n$ ");

    fs::remove_file(path).expect("temporary fixture should be removed");
}

#[test]
fn cat_receives_escaped_quotes_and_backslashes_from_double_quotes() {
    let quote_path = temporary_file("doublequote \" 2", "double quote content\n");
    let slash_path = temporary_file("backslash \\ 3", "backslash content\n");
    let quote_argument = quote_path.display().to_string().replace('"', "\\\"");
    let slash_argument = slash_path.display().to_string().replace('\\', "\\\\");
    let input = format!("cat \"{quote_argument}\" \"{slash_argument}\"\nexit\n");

    assert_eq!(
        run_shell(&input),
        "$ double quote content\nbackslash content\n$ "
    );

    fs::remove_file(quote_path).expect("temporary fixture should be removed");
    fs::remove_file(slash_path).expect("temporary fixture should be removed");
}

#[test]
fn pipelines_connect_stdout_of_one_external_command_to_stdin_of_another() {
    let input_path = temporary_file("pipeline-source", "apple\nbanana\ncarrot\ndate\neggplant\n");
    let output = run_shell(&format!("cat {} | wc\nexit\n", input_path.display()));

    assert!(output.starts_with("$ "), "unexpected output: {output:?}");
    assert!(output.ends_with("$ "), "unexpected output: {output:?}");
    assert!(output.contains("5"), "unexpected output: {output:?}");
    assert!(output.contains("apple") == false, "unexpected output: {output:?}");

    fs::remove_file(input_path).expect("temporary fixture should be removed");
}

#[test]
fn pipelines_chain_three_external_commands() {
    let input_path = temporary_file("pipeline-three-source", "a\nbb\ncccc\ndddd\neeeee\n");
    let output = run_shell(&format!(
        "cat {} | head -n 3 | wc\nexit\n",
        input_path.display()
    ));

    assert!(output.starts_with("$ "), "unexpected output: {output:?}");
    assert!(output.ends_with("$ "), "unexpected output: {output:?}");
    assert!(output.contains("3"), "unexpected output: {output:?}");
    assert!(output.contains("10"), "unexpected output: {output:?}");
    assert!(!output.contains("apple"), "unexpected output: {output:?}");

    fs::remove_file(input_path).expect("temporary fixture should be removed");
}

#[test]
fn pipelines_chain_four_external_commands() {
    let directory = temporary_path("pipeline-four-dir");
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).expect("temporary directory should be created");
    fs::write(directory.join("file"), "apple").expect("fixture file should be written");
    fs::write(directory.join("file-two"), "banana").expect("fixture file should be written");
    fs::write(directory.join("note"), "pear").expect("fixture file should be written");

    let output = run_shell(&format!(
        "ls -la {} | tail -n 5 | head -n 3 | grep \"file\"\nexit\n",
        directory.display()
    ));

    assert!(output.starts_with("$ "), "unexpected output: {output:?}");
    assert!(output.ends_with("$ "), "unexpected output: {output:?}");
    assert!(output.contains("file"), "unexpected output: {output:?}");
    assert!(!output.contains("note"), "unexpected output: {output:?}");

    fs::remove_dir_all(directory).expect("temporary directory should be removed");
}

#[test]
fn pipelines_connect_builtin_stdout_to_external_stdin() {
    let bin = temporary_bin("pipeline-builtin-left-bin", &[("wc", "/usr/bin/wc")]);
    let output = run_shell_output_with_path("echo apple-orange | wc\nexit\n", &bin.display().to_string());

    assert!(output.stdout.starts_with("$ "), "unexpected output: {output:?}");
    assert!(output.stdout.ends_with("$ "), "unexpected output: {output:?}");
    assert!(output.stdout.contains("1"), "unexpected output: {output:?}");
    assert!(output.stdout.contains("13"), "unexpected output: {output:?}");
    assert!(
        !output.stdout.contains("apple-orange"),
        "unexpected output: {output:?}"
    );
    assert_eq!(output.stderr, "");

    fs::remove_dir_all(bin).expect("temporary bin directory should be removed");
}

#[test]
fn pipelines_allow_builtins_in_middle_stages() {
    let bin = temporary_bin("pipeline-builtin-middle-bin", &[("wc", "/usr/bin/wc")]);
    let output = run_shell_output_with_path(
        "echo apple-orange | type exit | wc\nexit\n",
        &bin.display().to_string(),
    );

    assert!(output.stdout.starts_with("$ "), "unexpected output: {output:?}");
    assert!(output.stdout.ends_with("$ "), "unexpected output: {output:?}");
    assert!(
        output.stdout.contains("1"),
        "unexpected output: {output:?}"
    );
    assert!(
        output.stdout.contains("24"),
        "unexpected output: {output:?}"
    );
    assert_eq!(output.stderr, "");

    fs::remove_dir_all(bin).expect("temporary bin directory should be removed");
}

#[test]
fn pipelines_allow_builtins_to_consume_pipeline_position_without_printing_upstream_output() {
    let bin = temporary_bin("pipeline-builtin-right-bin", &[("ls", "/bin/ls")]);
    let output = run_shell_output_with_path("ls | type exit\nexit\n", &bin.display().to_string());

    assert_eq!(output.stdout, "$ exit is a shell builtin\n$ ");
    assert_eq!(output.stderr, "");

    fs::remove_dir_all(bin).expect("temporary bin directory should be removed");
}

#[test]
fn pipelines_keep_streaming_until_the_downstream_command_finishes() {
    let fifo = temporary_fifo("pipeline-stream.fifo");

    let mut child = Command::new(env!("CARGO_BIN_EXE_rusty_shell"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("shell binary should start");
    let mut stdout = BufReader::new(child.stdout.take().expect("stdout should be piped"));

    child
        .stdin
        .as_mut()
        .expect("stdin should be piped")
        .write_all(format!("cat {} | head -n 5\n", fifo.display()).as_bytes())
        .expect("pipeline should be written");

    let fifo_for_writer = fifo.clone();
    let append_thread = thread::spawn(move || {
        let mut writer = fs::OpenOptions::new()
            .write(true)
            .open(&fifo_for_writer)
            .expect("stream fifo should be opened for writing");
        writer
            .write_all(b"raspberry strawberry\npear mango\npineapple apple\n")
            .expect("initial stream lines should be written");
        thread::sleep(Duration::from_millis(100));
        writer
            .write_all(b"This is line 4.\nThis is line 5.\n")
            .expect("appended stream lines should be written");
    });

    let initial_output = read_until_contains(&mut stdout, "pineapple apple\n");
    assert_eq!(
        initial_output,
        "$ raspberry strawberry\npear mango\npineapple apple\n"
    );

    let appended_output = read_until_contains(&mut stdout, "This is line 5.\n");
    assert_eq!(appended_output, "This is line 4.\nThis is line 5.\n");
    assert_eq!(read_prompt(&mut stdout), "$ ");

    append_thread
        .join()
        .expect("append thread should complete successfully");

    child
        .stdin
        .as_mut()
        .expect("stdin should be piped")
        .write_all(b"exit\n")
        .expect("exit should be written");

    let status = child.wait().expect("shell should exit");
    assert!(status.success());

    fs::remove_file(fifo).expect("temporary fifo should be removed");
}

#[test]
fn pipelines_let_the_upstream_process_exit_naturally_after_downstream_completion() {
    let script = temporary_executable(
        "pipeline-natural-exit.sh",
        "#!/usr/bin/env python3\nimport os\nimport sys\n\nmarker = os.environ['MARKER']\ntry:\n    while True:\n        sys.stdout.write('line\\n')\n        sys.stdout.flush()\nexcept BrokenPipeError:\n    with open(marker, 'w', encoding='utf-8') as handle:\n        handle.write('PIPE\\n')\n    os._exit(0)\n",
    );
    let marker = temporary_path("pipeline-natural-exit.marker");

    let mut child = Command::new(env!("CARGO_BIN_EXE_rusty_shell"))
        .env("MARKER", &marker)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("shell binary should start");
    child
        .stdin
        .take()
        .expect("stdin should be piped")
        .write_all(format!("{} | head -n 5\nexit\n", script.display()).as_bytes())
        .expect("pipeline should be written");
    let output = child.wait_with_output().expect("shell should exit");

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stderr).expect("shell stderr should be UTF-8"),
        ""
    );
    assert_eq!(
        fs::read_to_string(&marker).expect("marker should be written"),
        "PIPE\n"
    );

    fs::remove_file(script).expect("temporary executable should be removed");
    fs::remove_file(marker).expect("marker should be removed");
}

#[test]
fn unquoted_backslashes_escape_spaces_quotes_and_regular_characters() {
    assert_eq!(
        run_shell(
            "echo multiple\\ \\ \\ \\ spaces\necho \\'\\\"literal quotes\\\"\\'\necho ignore\\_backslash\nexit\n"
        ),
        "$ multiple    spaces\n$ '\"literal quotes\"'\n$ ignore_backslash\n$ "
    );
}

#[test]
fn cat_receives_characters_escaped_outside_quotes() {
    let ignored_path = temporary_file("_ignored_1", "content1\n");
    let regular_path = temporary_file("ignore_2", "content2\n");
    let slash_path = temporary_file("just_one_\\_3", "content3\n");
    let ignored_argument = ignored_path
        .display()
        .to_string()
        .replace("_ignored_1", "\\_ignored_1");
    let regular_argument = regular_path
        .display()
        .to_string()
        .replace("ignore_2", "ignore_\\2");
    let slash_argument = slash_path.display().to_string().replace('\\', "\\\\");
    let input = format!("cat {ignored_argument} {regular_argument} {slash_argument}\nexit\n");

    assert_eq!(run_shell(&input), "$ content1\ncontent2\ncontent3\n$ ");

    fs::remove_file(ignored_path).expect("temporary fixture should be removed");
    fs::remove_file(regular_path).expect("temporary fixture should be removed");
    fs::remove_file(slash_path).expect("temporary fixture should be removed");
}

#[test]
fn stdout_redirection_creates_and_overwrites_files_for_builtins() {
    let output_path = temporary_file("stdout-redirection", "stale content\n");
    let input = format!(
        "echo first>{0}\necho Hello James 1> {0}\ncat {0}\nexit\n",
        output_path.display()
    );
    let output = run_shell_output(&input);

    assert_eq!(output.stdout, "$ $ $ Hello James\n$ ");
    assert_eq!(output.stderr, "");
    assert_eq!(
        fs::read_to_string(&output_path).expect("redirected output should be readable"),
        "Hello James\n"
    );

    fs::remove_file(output_path).expect("temporary fixture should be removed");
}

#[test]
fn stdout_redirection_does_not_capture_external_stderr() {
    let source_path = temporary_file("stdout-source", "blueberry\n");
    let output_path = temporary_path("stdout-output");
    let missing_path = temporary_path("stdout-missing");
    let input = format!(
        "cat {} {} > {}\nexit\n",
        source_path.display(),
        missing_path.display(),
        output_path.display()
    );
    let output = run_shell_output(&input);

    assert_eq!(output.stdout, "$ $ ");
    assert!(output.stderr.contains(&missing_path.display().to_string()));
    assert_eq!(
        fs::read_to_string(&output_path).expect("redirected output should be readable"),
        "blueberry\n"
    );

    fs::remove_file(source_path).expect("temporary fixture should be removed");
    fs::remove_file(output_path).expect("temporary fixture should be removed");
}

#[test]
fn stderr_redirection_does_not_capture_external_stdout() {
    let source_path = temporary_file("stderr-source", "pear\n");
    let error_path = temporary_file("stderr-output", "stale error\n");
    let missing_path = temporary_path("stderr-missing");
    let input = format!(
        "cat {} {} 2> {}\nexit\n",
        source_path.display(),
        missing_path.display(),
        error_path.display()
    );
    let output = run_shell_output(&input);
    let redirected_error =
        fs::read_to_string(&error_path).expect("redirected error should be readable");

    assert_eq!(output.stdout, "$ pear\n$ ");
    assert_eq!(output.stderr, "");
    assert!(redirected_error.contains(&missing_path.display().to_string()));
    assert!(!redirected_error.contains("pear"));

    fs::remove_file(source_path).expect("temporary fixture should be removed");
    fs::remove_file(error_path).expect("temporary fixture should be removed");
}

#[test]
fn stderr_redirection_creates_an_empty_file_for_successful_builtins() {
    let error_path = temporary_file("empty-stderr", "stale error\n");
    let input = format!(
        "echo Maria file cannot be found 2> {}\nexit\n",
        error_path.display()
    );
    let output = run_shell_output(&input);

    assert_eq!(output.stdout, "$ Maria file cannot be found\n$ ");
    assert_eq!(output.stderr, "");
    assert_eq!(
        fs::read_to_string(&error_path).expect("redirected error should be readable"),
        ""
    );

    fs::remove_file(error_path).expect("temporary fixture should be removed");
}

#[test]
fn quoted_and_escaped_redirect_operators_remain_literal() {
    assert_eq!(run_shell("echo '>' \\> \">\"\nexit\n"), "$ > > >\n$ ");
}

#[test]
fn redirection_targets_can_be_quoted() {
    let output_path = temporary_file("redirect target with spaces", "stale content\n");
    let input = format!("echo quoted > '{}'\nexit\n", output_path.display());
    let output = run_shell_output(&input);

    assert_eq!(output.stdout, "$ $ ");
    assert_eq!(output.stderr, "");
    assert_eq!(
        fs::read_to_string(&output_path).expect("redirected output should be readable"),
        "quoted\n"
    );

    fs::remove_file(output_path).expect("temporary fixture should be removed");
}

#[test]
fn stdout_append_preserves_existing_content_for_builtins() {
    let output_path = temporary_file("stdout-append", "existing\n");
    let input = format!(
        "echo first>>{0}\necho second 1>> {0}\ncat {0}\nexit\n",
        output_path.display()
    );
    let output = run_shell_output(&input);

    assert_eq!(output.stdout, "$ $ $ existing\nfirst\nsecond\n$ ");
    assert_eq!(output.stderr, "");
    assert_eq!(
        fs::read_to_string(&output_path).expect("appended output should be readable"),
        "existing\nfirst\nsecond\n"
    );

    fs::remove_file(output_path).expect("temporary fixture should be removed");
}

#[test]
fn stdout_append_creates_files_and_leaves_external_stderr_on_terminal() {
    let source_path = temporary_file("append-source", "apple\n");
    let output_path = temporary_path("append-created-output");
    let missing_path = temporary_path("append-missing");
    let input = format!(
        "cat {} {} >> {}\nexit\n",
        source_path.display(),
        missing_path.display(),
        output_path.display()
    );
    let output = run_shell_output(&input);

    assert_eq!(output.stdout, "$ $ ");
    assert!(output.stderr.contains(&missing_path.display().to_string()));
    assert_eq!(
        fs::read_to_string(&output_path).expect("appended output should be readable"),
        "apple\n"
    );

    fs::remove_file(source_path).expect("temporary fixture should be removed");
    fs::remove_file(output_path).expect("temporary fixture should be removed");
}

#[test]
fn stdout_overwrite_can_be_followed_by_append() {
    let source_path = temporary_file("mixed-source", "apple\nbanana\n");
    let output_path = temporary_file("mixed-output", "stale\n");
    let input = format!(
        "echo List of files: > {0}\ncat {1} >> {0}\ncat {0}\nexit\n",
        output_path.display(),
        source_path.display()
    );
    let output = run_shell_output(&input);

    assert_eq!(output.stdout, "$ $ $ List of files:\napple\nbanana\n$ ");
    assert_eq!(output.stderr, "");

    fs::remove_file(source_path).expect("temporary fixture should be removed");
    fs::remove_file(output_path).expect("temporary fixture should be removed");
}

#[test]
fn stderr_append_preserves_existing_errors_and_stdout() {
    let error_path = temporary_file("stderr-append", "existing error\n");
    let first_missing = temporary_path("append-first-missing");
    let second_missing = temporary_path("append-second-missing");
    let input = format!(
        "cat {} 2>> {2}\nls {} 2>> {2}\necho visible 2>> {2}\nexit\n",
        first_missing.display(),
        second_missing.display(),
        error_path.display()
    );
    let output = run_shell_output(&input);
    let errors = fs::read_to_string(&error_path).expect("appended errors should be readable");
    let first_position = errors
        .find(&first_missing.display().to_string())
        .expect("first error should be appended");
    let second_position = errors
        .find(&second_missing.display().to_string())
        .expect("second error should be appended");

    assert_eq!(output.stdout, "$ $ $ visible\n$ ");
    assert_eq!(output.stderr, "");
    assert!(errors.starts_with("existing error\n"));
    assert!(first_position < second_position);
    assert!(!errors.contains("visible"));

    fs::remove_file(error_path).expect("temporary fixture should be removed");
}

#[test]
fn stderr_append_creates_an_empty_file_for_successful_builtins() {
    let error_path = temporary_path("created-stderr-append");
    let input = format!("echo visible 2>> {}\nexit\n", error_path.display());
    let output = run_shell_output(&input);

    assert_eq!(output.stdout, "$ visible\n$ ");
    assert_eq!(output.stderr, "");
    assert_eq!(
        fs::read_to_string(&error_path).expect("appended error file should be readable"),
        ""
    );

    fs::remove_file(error_path).expect("temporary fixture should be removed");
}

#[test]
fn quoted_and_escaped_append_operators_remain_literal() {
    assert_eq!(
        run_shell("echo '>>' \\>\\> \">>\"\nexit\n"),
        "$ >> >> >>\n$ "
    );
}

#[test]
fn background_commands_print_job_info_and_return_the_prompt_immediately() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_rusty_shell"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("shell binary should start");

    let mut stdout = BufReader::new(child.stdout.take().expect("stdout should be piped"));

    child
        .stdin
        .as_mut()
        .expect("stdin should be piped")
        .write_all(b"sleep 1 &\n")
        .expect("background command should be written");

    let prompt = read_prompt(&mut stdout);
    assert_eq!(prompt, "$ ");

    let first_line = read_prompt_line(&mut stdout);
    assert!(
        first_line.starts_with("[1] "),
        "unexpected first line: {first_line:?}"
    );

    let pid = first_line
        .trim_end()
        .split_whitespace()
        .last()
        .expect("job line should include a pid")
        .parse::<u32>()
        .expect("pid should be numeric");

    assert_eq!(
        std::process::Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .status()
            .expect("kill should run")
            .code(),
        Some(0)
    );

    let prompt = read_prompt(&mut stdout);
    assert_eq!(prompt, "$ ");

    child
        .stdin
        .as_mut()
        .expect("stdin should be piped")
        .write_all(b"exit\n")
        .expect("exit should be written");

    let status = child.wait().expect("shell should exit");
    assert!(status.success());
}

#[test]
fn background_and_foreground_processes_share_the_shell_terminal_output() {
    let background_fifo = temporary_fifo("background-output.fifo");
    let foreground_fifo = temporary_fifo("foreground-output.fifo");
    let (terminal_reader, terminal_writer) =
        UnixStream::pair().expect("pseudo terminal stream should be created");
    let terminal_writer_for_stderr = terminal_writer
        .try_clone()
        .expect("writer clone should be created");
    let stdout_fd: OwnedFd = terminal_writer.into();
    let stderr_fd: OwnedFd = terminal_writer_for_stderr.into();

    let mut child = Command::new(env!("CARGO_BIN_EXE_rusty_shell"))
        .stdin(Stdio::piped())
        .stdout(Stdio::from(stdout_fd))
        .stderr(Stdio::from(stderr_fd))
        .spawn()
        .expect("shell binary should start");
    let mut stdin = child.stdin.take().expect("stdin should be piped");

    stdin
        .write_all(
            format!(
                "cat {} &\ncat {}\n",
                background_fifo.display(),
                foreground_fifo.display()
            )
            .as_bytes(),
        )
        .expect("commands should be written");

    let background_fifo_for_writer = background_fifo.clone();
    let background_writer = thread::spawn(move || {
        thread::sleep(Duration::from_millis(100));
        fs::write(&background_fifo_for_writer, "Hello from FIFO#1\n")
            .expect("background fifo should receive data");
    });

    let foreground_fifo_for_writer = foreground_fifo.clone();
    let foreground_writer = thread::spawn(move || {
        thread::sleep(Duration::from_millis(200));
        fs::write(&foreground_fifo_for_writer, "Hello from FIFO#2\n")
            .expect("foreground fifo should receive data");
    });

    let mut reader = BufReader::new(terminal_reader);
    let mut output = read_until_contains(&mut reader, "Hello from FIFO#1\n");
    output.push_str(&read_until_contains(&mut reader, "Hello from FIFO#2\n"));

    stdin.write_all(b"exit\n").expect("exit should be written");
    drop(stdin);

    background_writer
        .join()
        .expect("background writer thread should finish");
    foreground_writer
        .join()
        .expect("foreground writer thread should finish");

    let status = child.wait().expect("shell should exit");
    assert!(status.success());
    assert!(output.contains("Hello from FIFO#1\n"));
    assert!(output.contains("Hello from FIFO#2\n"));

    fs::remove_file(background_fifo).expect("background fifo should be removed");
    fs::remove_file(foreground_fifo).expect("foreground fifo should be removed");
}
