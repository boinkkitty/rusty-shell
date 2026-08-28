use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn run_shell(input: &str) -> String {
    let mut child = Command::new(env!("CARGO_BIN_EXE_rusty_shell"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
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
    String::from_utf8(output.stdout).expect("shell output should be UTF-8")
}

fn temporary_file(name: &str, contents: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("rusty_shell-{}-{name}", std::process::id()));
    fs::write(&path, contents).expect("temporary fixture should be written");
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
    assert_eq!(
        run_shell("definitely-not-a-shell-command\nexit\n"),
        "$ definitely-not-a-shell-command: command not found\n$ "
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
