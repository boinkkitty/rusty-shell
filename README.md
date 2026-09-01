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
| `declare [name[=value]...]` | Creates or updates shell variables, or prints their values when given names. | `declare Item=widget` |
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

### Variables and parameter expansion

**Mimics a real shell:** `declare NAME=value` stores a shell variable, and `$NAME` or `${NAME}` expands it in builtin and external-command arguments. Missing variables expand to an empty string, and `$` inside single quotes remains literal.

```sh
declare greeting=hello
echo "$greeting world"
echo '${greeting}'
```

The first command prints `hello world`; the second prints the literal text `${greeting}`. Braced names can be embedded next to text, for example `${greeting}_id`.

**Implementation support:** A mutex-protected Rust `HashMap` stores variables. The parser marks single-quoted dollar signs with a private sentinel so expansion can distinguish literal `$` from expandable parameter syntax.

**Does not yet mimic:** Variables are shell-local and are not exported to child processes. There is no positional parameter support, `$?`, command substitution, default-value syntax, special parameters, or expansion field splitting. An argument that expands to an empty string is removed rather than preserved as an empty argument.

**Future work:** Track quote context through an expansion AST, preserve empty fields correctly, implement special and positional parameters, add `export`, and define environment inheritance explicitly.

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

- `src/parser.rs` parses arguments, quoting, escaping, variables, and redirection targets.
- `src/command.rs` dispatches built-ins and external programs, expands variables, and configures output streams.
- `src/shell.rs` owns the interactive read-execute loop.
- `src/repl.rs` configures readline editing and command completion.
- `tests/shell_cli.rs` exercises the shell through its command-line interface.

## Test

```sh
cargo test --locked
```

The test suite covers built-ins, executable lookup, command-name completion including unique-completion spacing, session history and persistence, variable declaration and parameter expansion, pipelines, argument parsing, quoting, escaping, EOF handling, `cd ~` without `HOME`, and stdout/stderr overwrite and append redirection.

## Current scope

This project intentionally implements a focused subset of shell behavior. Wildcard expansion, command substitution, input redirection, argument/path completion, exported variables, and compound commands are not currently supported.

## Feature Status

| Feature | Status | Current implementation | Real-shell behaviour still missing |
| --- | --- | --- | --- |
| Navigation | Partial | `pwd`, `cd`, and `cd ~` in the parent shell | `CDPATH`, `cd -`, directory stacks, logical/physical modes, `PWD` and `OLDPWD` |
| Quoting and escaping | Partial | Single/double quotes, unquoted backslash escapes, adjacent quoted segments | Full grammar, comments, multiline/error handling, expansion-aware context |
| Redirection | Partial | stdout/stderr overwrite and append using `>`, `>>`, `1>`, `1>>`, `2>`, `2>>` | `<`, descriptor duplication/closing, ordering, arbitrary descriptors, heredocs |
| Command completion | Partial | Builtin and executable names from `PATH` via `rustyline` | Argument-aware, option-aware, file/directory, and programmable completion |
| File/path completion | Not implemented | No file or directory candidate provider | File, directory, tilde, glob, and context-aware path completion |
| Programmable completion | Not implemented | No completion functions or scripts | Bash/Zsh completion functions and context-aware candidates |
| Background jobs | Partial | Async external commands, IDs, `jobs`, prompt-time reaping | Process groups, `fg`, `bg`, `wait`, signals, terminal control, pipeline jobs |
| Pipelines | Partial | Any number of stages, builtin stages, streaming pipes | Pipeline status, `pipefail`, process groups, signals, background pipelines |
| Command history | Partial | In-memory `history`, numeric suffix, `-r/-w/-a` | Reverse search, history expansion, shared editor backend, policy controls |
| History persistence | Partial | `HISTFILE` startup read and session append on exit/EOF | Concurrent-session locking/merging, atomic writes, deduplication, limits |
| Parameter expansion | Partial | `$NAME` and `${NAME}` from shell-local `declare` variables | `$?`, positional/special parameters, word splitting, globbing, command substitution |

## Comparison With Bash and Zsh

`rusty-shell` follows the shape of a Unix shell but implements a deliberately narrow subset.

| Area | `rusty-shell` | Bash/Zsh comparison |
| --- | --- | --- |
| Grammar | Words, quotes, redirects, `|`, and trailing `&` | Lists, conditionals, functions, loops, subshells, comments, and richer diagnostics are missing |
| Quotes and escaping | Common single/double quote and backslash cases | Simple examples are similar; edge cases and expansion context are incomplete |
| Expansion | `$NAME` and `${NAME}` only | Bash/Zsh also perform ordered tilde, command, arithmetic, splitting, globbing, and `$?` expansion |
| Word splitting/globbing | Not implemented | Core post-expansion behavior |
| Redirection | stdout/stderr overwrite and append | Missing input, descriptor duplication/closing, ordering, and heredocs |
| Pipelines | Multi-stage streaming, including builtins | Missing pipeline status, `pipefail`, process groups, and terminal semantics |
| Jobs | In-memory IDs, `jobs`, polling | Bash/Zsh provide `fg`, `bg`, `wait`, notifications, process groups, and terminal ownership |
| Signals | Readline interruption only | Bash/Zsh coordinate signals across foreground process groups |
| Completion | Builtin/executable command names from `PATH` | Bash/Zsh also support file, directory, option, argument, and programmable completion |
| History | Memory, explicit file operations, optional `HISTFILE` | Richer search, expansion, limits, configuration, locking, and concurrent-session handling |

Parsing a syntax token is not evidence that its complete Bash/Zsh runtime semantics are implemented.

## Roadmap

### Shell correctness

1. Build a quote-aware token and AST pipeline with correct expansion ordering.
2. Add exit statuses, `$?`, pipeline status rules, and optional `pipefail`.
3. Implement input redirection, ordered descriptor operations, duplication, closing, and heredocs.
4. Add process groups, `fg`, `bg`, `wait`, signal forwarding, and terminal ownership.
5. Add lists, conditionals, subshells, command substitution, globbing, word splitting, positional parameters, and exported variables.
6. Make history persistence safe for concurrent sessions with locking, atomic writes, limits, and deduplication.

### Completion and quality of life

1. Add file and directory completion for relative paths, absolute paths, `~`, quoting, and filtering.
2. Add programmable/context-aware completion for options, arguments, descriptions, and user-provided functions.
3. Add reverse history search, history expansion, aliases, startup files, and configurable editor behavior.
4. Add command lookup caching, incremental completion indexes, bounded history storage, and event-driven job notifications.

## Architecture

1. `src/shell.rs` selects terminal input through `repl::ShellEditor` or line reads for non-interactive input.
2. `src/repl.rs` configures `rustyline` editing, command-name completion, Ctrl-C/Ctrl-D handling, and readline history.
3. `src/parser.rs` scans quote, redirection, and pipeline state into `ParsedCommand` and `CommandStage` values, marking trailing `&` and literal single-quoted dollar signs.
4. `src/command.rs` records history, expands shell-local variables, opens redirection files, and dispatches builtins, external programs, pipelines, or background jobs.
5. Pipelines connect `ChildStdout` to the next process with `Stdio::piped`; builtin stages re-enter `src/main.rs` with `--pipeline-builtin`.
6. Jobs use `Mutex`, `OnceLock`, and `Child::try_wait`; `HISTFILE` persistence reads and appends line-oriented history files.

## Running and Testing

The crate requires a Unix-like operating system and Rust 1.96 or newer. Its package name is `rusty_shell`.

```sh
cargo run
cargo test --locked
```

The tests cover all registered builtins, navigation, quoting, escaping, redirection, command-name completion, parameter expansion, pipelines, background jobs, session history, and `HISTFILE` persistence. The Codecrafters entrypoint is also available:

```sh
./your_program.sh
```
