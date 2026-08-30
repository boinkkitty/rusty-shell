# rusty-shell

`rusty-shell` is a small Unix shell written in Rust. It provides an interactive prompt, a focused set of built-in commands, executable discovery through `PATH`, shell-style quoting and escaping, and output redirection for both built-ins and external programs.

## Requirements

- A Unix-like operating system
- Rust 1.96 or newer

## Run

```sh
cargo run
```

The shell displays a `$ ` prompt and reads one command at a time:

```text
$ echo hello world
hello world
$ pwd
/path/to/current/directory
$ exit
```

## Built-in commands

| Command | Description | Example |
| --- | --- | --- |
| `echo [arguments...]` | Prints its arguments separated by spaces. | `echo hello world` |
| `type <command>` | Reports whether a command is built in, resolves its executable from `PATH`, or reports that it was not found. | `type cargo` |
| `jobs` | Lists active background jobs and reports jobs that have completed since the previous check. | `jobs` |
| `pwd` | Prints the current working directory. | `pwd` |
| `cd <directory>` | Changes the current working directory. `cd ~` uses `HOME`. | `cd /tmp` |
| `exit` | Exits the shell. | `exit` |

## External programs

Commands that are not built in are resolved by searching the directories in `PATH`. The shell executes regular files with at least one executable permission bit and forwards every parsed argument to the child process.

```text
$ printf '<%s>\n' first second
<first>
<second>
```

The child process inherits the terminal unless its output is redirected.

## Interactive completion

When `stdin` is a terminal, pressing `Tab` attempts to complete the command name before the first argument.

- A unique match inserts the remaining text and a trailing space.
- Multiple matches extend to the longest shared prefix when possible.
- If the prefix is still ambiguous, the first `Tab` rings the terminal bell and a second `Tab` prints the sorted matches.

Completion candidates currently include:

- Built-in commands `echo` and `exit`
- Executable file names discovered in `PATH`

Completion is limited to the command position. It does not complete arguments, file paths, or later built-ins like `cd`, `pwd`, or `type`.

## Background jobs

Append `&` to an external command to run it in the background. The shell prints a job number and process ID, then immediately displays the next prompt:

```text
$ sleep 10 &
[1] 12345
$ jobs
[1]+  Running                 sleep 10 &
```

Completed jobs are reported before the next prompt and removed from the job list. Job numbers are reused after completed jobs are reaped.

## Quoting and escaping

Whitespace separates arguments unless it is quoted or escaped.

### Single quotes

Single quotes preserve every enclosed character literally, including whitespace and backslashes:

```text
$ echo 'hello    world' 'a\b'
hello    world a\b
```

### Double quotes

Double quotes preserve whitespace. Within double quotes, `\"` produces a literal double quote and `\\` produces a literal backslash. Backslashes before other characters remain unchanged.

```text
$ echo "hello    world" "say \"hi\""
hello    world say "hi"
```

### Unquoted backslashes

Outside quotes, a backslash escapes the following character:

```text
$ echo hello\ world
hello world
```

Quoted and unquoted segments can be adjacent and form one argument:

```text
$ echo "hello"world
helloworld
```

## Output redirection

Redirection works for built-in commands and external programs. Targets may be quoted when their paths contain spaces.

| Syntax | Behavior |
| --- | --- |
| `> file` or `1> file` | Redirects standard output and overwrites the file. |
| `>> file` or `1>> file` | Redirects standard output and appends to the file. |
| `2> file` | Redirects standard error and overwrites the file. |
| `2>> file` | Redirects standard error and appends to the file. |

```text
$ echo first > output.txt
$ echo second >> output.txt
$ cat output.txt
first
second
$ cat missing.txt 2> errors.txt
```

Quoted or escaped redirect operators are treated as ordinary arguments:

```text
$ echo '>' \> ">>"
> > >>
```

## Project structure

- `src/parser.rs` parses arguments, quoting, escaping, and redirection targets.
- `src/command.rs` dispatches built-ins and external programs and configures output streams.
- `src/shell.rs` owns the interactive read-execute loop.
- `tests/shell_cli.rs` exercises the shell through its command-line interface.

## Test

```sh
cargo test --locked
```

The test suite covers built-ins, executable lookup, command-name completion, argument parsing, quoting, escaping, EOF handling, `cd ~` without `HOME`, and stdout/stderr overwrite and append redirection.

## Current scope

This project intentionally implements a focused subset of shell behavior. Pipelines, environment-variable expansion, wildcard expansion, command substitution, input redirection, argument/path completion, and compound commands are not currently supported.
