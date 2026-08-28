use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

struct ShellOutput {
    stdout: String,
    stderr: String,
}

fn shell_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_rusty_shell"))
}

fn run_shell_output_with(mut command: Command, input: &str) -> ShellOutput {
    let mut child = command
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

fn run_shell_output(input: &str) -> ShellOutput {
    run_shell_output_with(shell_command(), input)
}

fn run_shell(input: &str) -> String {
    run_shell_output(input).stdout
}

fn temporary_file(name: &str, contents: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("rusty_shell-{}-{name}", std::process::id()));
    fs::write(&path, contents).expect("temporary fixture should be written");
    path
}

fn temporary_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("rusty_shell-{}-{name}", std::process::id()))
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
fn exits_when_standard_input_closes() {
    let mut child = shell_command()
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("shell binary should start");
    let deadline = Instant::now() + Duration::from_millis(500);

    while Instant::now() < deadline {
        if let Some(status) = child.try_wait().expect("shell status should be available") {
            assert!(status.success());
            return;
        }
        thread::sleep(Duration::from_millis(10));
    }

    child.kill().expect("stuck shell should be terminated");
    child.wait().expect("terminated shell should be reaped");
    panic!("shell did not exit when standard input closed");
}

#[test]
fn cd_home_reports_an_error_when_home_is_unset() {
    let mut command = shell_command();
    command.env_remove("HOME");

    let output = run_shell_output_with(command, "cd ~\nexit\n");

    assert_eq!(output.stdout, "$ $ ");
    assert_eq!(output.stderr, "cd: HOME not set\n");
}
