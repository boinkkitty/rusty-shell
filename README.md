# rusty-shell

`rusty-shell` is a small Unix shell written in Rust. It is an educational implementation of common Unix shell behavior, not a POSIX-compatible replacement for `sh`, `bash`, or `zsh`.

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

## Command reference

Built-ins execute inside the shell process unless they are placed in a pipeline, where they run through the `--pipeline-builtin` helper mode. This mirrors the real-shell rule that stateful commands such as `cd` must affect the current shell, while allowing pipeline stages to have independent standard streams.

| Command | Description | Example |
| --- | --- | --- |
| `echo [arguments...]` | Prints its arguments separated by spaces. | `echo hello world` |
| `type <command>` | Reports whether a command is built in, resolves its executable from `PATH`, or reports that it was not found. | `type cargo` |
| `jobs` | Lists active background jobs and reports jobs that have completed since the previous check. | `jobs` |
| `history [count]` | Prints commands recorded during the current session, optionally limited to the most recent count. | `history 10` |
| `pwd` | Prints the current working directory. | `pwd` |
| `cd <directory>` | Changes the current working directory. `cd ~` uses `HOME`. | `cd /tmp` |
| `exit` | Exits the shell. | `exit` |

The shell also launches external programs found through `PATH`, passing parsed arguments to them without reimplementing their functionality.

## Feature behavior

### Built-ins and external commands

**Mimics a real shell:** Built-ins are dispatched without spawning a process, external commands are resolved through `PATH`, `type` distinguishes built-ins from executables, and `cd` changes the shell's working directory.

**Implementation support:** Rust's `std::process::Command`, `std::env`, filesystem metadata APIs, and Unix permission APIs provide process launching, environment lookup, executable discovery, and directory changes.

**Does not yet mimic:** Command lookup does not implement shell functions, aliases, hashed command tables, login startup files, or command precedence beyond this small built-in registry. Error messages and `cd` argument handling are simplified.

**Future work:** Add a structured command-resolution layer with aliases/functions, `CDPATH`, `OLDPWD`/`PWD`, shell startup configuration, and shell-compatible diagnostics.

### Parsing, quoting, and escaping

**Mimics a real shell:** Unquoted whitespace separates arguments; single quotes preserve literal text; double quotes preserve whitespace while recognizing escaped quotes and backslashes; adjacent quoted and unquoted text forms one argument; and an unquoted backslash escapes the next character.

**Implementation support:** `src/parser.rs` uses a small state machine with explicit quote modes, redirect state, and pipeline-stage state. This keeps parsing independent from process execution and makes the grammar directly testable.

**Does not yet mimic:** There is no variable expansion, globbing, command substitution, arithmetic expansion, here-documents, subshell syntax, comments, or full error reporting for unterminated quotes and malformed commands.

**Future work:** Introduce a tokenization and expansion phase before parsing into an execution AST, report syntax locations, and add expansion ordering compatible with POSIX shells.

### Pipelines

**Mimics a real shell:** `cmd1 | cmd2 | cmd3` connects each process's standard output to the next process's standard input. Any number of stages is supported, external processes stream concurrently, and built-ins can occupy pipeline positions.

**Implementation support:** Rust `ChildStdout`, `Stdio::piped`, and `std::process::Command` connect stages. Built-ins in pipelines are relaunched with `--pipeline-builtin` so their output can be piped without changing the parent shell state.

**Does not yet mimic:** Pipeline exit status is not propagated as `$?` or `pipefail`; signals, job control, process groups, and terminal ownership are not managed as a group; and pipeline-level background execution is limited.

**Future work:** Add process groups and `waitpid`-style status collection, expose exit statuses, implement `pipefail`, and handle signals and terminal control correctly.

### Redirection

**Mimics a real shell:** `>`, `>>`, `1>`, `1>>`, `2>`, and `2>>` redirect standard output or standard error, with overwrite and append behavior. Redirection applies to built-ins and external commands, and quoted paths are supported.

**Implementation support:** `OpenOptions` opens files with create, truncate, or append modes; `Stdio::from` attaches them to child processes; built-ins write through an output writer selected at execution time.

**Does not yet mimic:** Input redirection (`<`), file-descriptor duplication (`2>&1`), descriptor closing, multiple ordered redirections, and heredocs are not implemented. Redirection errors use Rust I/O errors rather than shell-compatible diagnostics.

**Future work:** Represent redirections as ordered operations, support arbitrary file descriptors and descriptor duplication, and apply them with `dup2` semantics before executing each command.

### Background jobs

**Mimics a real shell:** Appending `&` starts an external command asynchronously, returns a prompt immediately, assigns a job number, and exposes status through `jobs`. Completed jobs are reaped before a prompt and job numbers are reused.

**Implementation support:** `Child::spawn`, `Child::try_wait`, a mutex-protected job table, and explicit job-status refresh provide asynchronous tracking without blocking the shell loop.

**Does not yet mimic:** There are no process groups, foreground/background control, `fg`/`bg`, `wait`, job-spec syntax, signal forwarding, terminal ownership, or robust shutdown cleanup. Background built-ins are not independently scheduled.

**Future work:** Use Unix process groups, implement `SIGCHLD`-driven status updates, add `wait`/`fg`/`bg`, and make Ctrl-C/Ctrl-Z affect the foreground job rather than the shell.

### Interactive completion and history

**Mimics a real shell:** Interactive input supports line editing, command-name completion, completion candidate listing, Ctrl-C interruption, Ctrl-D at an empty prompt, and history navigation. Unique completion adds a trailing space; ambiguous completion leaves candidates suitable for further completion.

**Implementation support:** The `rustyline` crate provides terminal editing, readline history, completion hooks, and interruption handling. `src/repl.rs` adapts it to this shell, while `src/command.rs` supplies built-in and executable candidates.

**Does not yet mimic:** Completion only covers the command position, not arguments or filesystem paths. The `history` builtin supports in-memory history plus `history -r file`, `history -w file`, and `history -a file`; setting `HISTFILE` loads history at startup and appends the current session on exit. Reverse search and shell history expansion are not implemented.

**Future work:** Complete paths and arguments contextually, support reverse search and history expansion, and share one history backend between the `history` builtin and readline editor.

## External programs

Commands that are not built in are resolved by searching the directories in `PATH`. The shell executes regular files with at least one executable permission bit and forwards every parsed argument to the child process.

```text
$ printf '<%s>\n' first second
<first>
<second>
```

The child process inherits the terminal unless its output is redirected.

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
- `src/repl.rs` configures readline editing and command completion.
- `tests/shell_cli.rs` exercises the shell through its command-line interface.

## Test

```sh
cargo test --locked
```

The test suite covers built-ins, executable lookup, command-name completion including unique-completion spacing, session history, pipelines, argument parsing, quoting, escaping, EOF handling, `cd ~` without `HOME`, and stdout/stderr overwrite and append redirection.

## Current scope

This project intentionally implements a focused subset of shell behavior. Environment-variable expansion, wildcard expansion, command substitution, input redirection, argument/path completion, and compound commands are not currently supported.
