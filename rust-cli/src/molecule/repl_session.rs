use crate::atom::cli_contract::Cli;
use crate::atom::json_contract::{BridgeRequest, ExecutionResult};
use crate::atom::path_ops::repl_history_path;
use crate::atom::process_runner::backend_command_available;
use crate::atom::repl_formatting::{
    format_repl_output, highlight_input_line, sanitize_repl_output,
};
use crate::molecule::backend_client::{
    spawn_bridge_via_backend_command, spawn_bridge_via_module, spawn_bridge_with_project_python,
};
use crate::molecule::repo_resolution::{resolve_python, resolve_repo_root, PROJECT_ROOT_ENV};
use anyhow::{bail, Context, Result};
use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::history::DefaultHistory;
use rustyline::validate::Validator;
use rustyline::{CompletionType, Config, Context as RustyContext, Editor, Helper};
use std::borrow::Cow;
use std::io::{self, BufRead, BufReader, IsTerminal, Write};
use std::process::{Child, ChildStdin, ChildStdout};

struct ReplHelper {
    colorize: bool,
}

impl Helper for ReplHelper {}

impl Completer for ReplHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        _line: &str,
        pos: usize,
        _ctx: &RustyContext<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        Ok((pos, Vec::new()))
    }
}

impl Hinter for ReplHelper {
    type Hint = String;

    fn hint(&self, _line: &str, _pos: usize, _ctx: &RustyContext<'_>) -> Option<String> {
        None
    }
}

impl Validator for ReplHelper {}

impl Highlighter for ReplHelper {
    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
        Cow::Owned(highlight_input_line(line, self.colorize))
    }

    fn highlight_prompt<'b, 's: 'b, 'p: 'b>(
        &'s self,
        prompt: &'p str,
        _default: bool,
    ) -> Cow<'b, str> {
        let rendered = if self.colorize {
            let code = if prompt.starts_with('.') || prompt.starts_with('>') {
                "\x1b[1;36m"
            } else {
                ""
            };
            if code.is_empty() {
                prompt.to_string()
            } else {
                format!("{code}{prompt}\x1b[0m")
            }
        } else {
            prompt.to_string()
        };
        Cow::Owned(rendered)
    }

    fn highlight_hint<'h>(&self, hint: &'h str) -> Cow<'h, str> {
        Cow::Borrowed(hint)
    }

    fn highlight_candidate<'c>(
        &self,
        candidate: &'c str,
        _completion: CompletionType,
    ) -> Cow<'c, str> {
        Cow::Borrowed(candidate)
    }

    fn highlight_char(&self, _line: &str, _pos: usize, _forced: bool) -> bool {
        true
    }
}

struct BridgeClient {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl BridgeClient {
    fn new(child: Child) -> Result<Self> {
        let mut child = child;
        let stdin = child
            .stdin
            .take()
            .context("Backend bridge stdin was not available")?;
        let stdout = child
            .stdout
            .take()
            .context("Backend bridge stdout was not available")?;
        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
        })
    }

    fn execute(
        &mut self,
        code: &str,
        working_dir: Option<&str>,
        timeout: Option<u32>,
    ) -> Result<ExecutionResult> {
        let request = BridgeRequest {
            command: "run".to_string(),
            code: Some(code.to_string()),
            working_dir: working_dir.map(ToOwned::to_owned),
            timeout,
        };
        self.send_request(&request)?;
        self.read_response()
    }

    fn shutdown(&mut self) -> Result<()> {
        let request = BridgeRequest {
            command: "quit".to_string(),
            code: None,
            working_dir: None,
            timeout: None,
        };
        let _ = self.send_request(&request);
        let _ = self.child.wait();
        Ok(())
    }

    fn send_request(&mut self, request: &BridgeRequest) -> Result<()> {
        let rendered = serde_json::to_string(request)?;
        writeln!(self.stdin, "{rendered}")?;
        self.stdin.flush()?;
        Ok(())
    }

    fn read_response(&mut self) -> Result<ExecutionResult> {
        let mut line = String::new();
        let bytes = self.stdout.read_line(&mut line)?;
        if bytes == 0 {
            bail!("Backend bridge exited before returning a response");
        }
        serde_json::from_str(line.trim_end()).context("Backend bridge returned invalid JSON")
    }
}

fn clear_screen() -> io::Result<()> {
    if io::stdout().is_terminal() {
        print!("\x1b[2J\x1b[H");
        io::stdout().flush()?;
    }
    Ok(())
}

fn print_result(result: &ExecutionResult, colorize: bool) {
    let source = if result.output.is_empty() {
        result.error.as_deref().unwrap_or("")
    } else {
        result.output.as_str()
    };
    let text = sanitize_repl_output(source);
    if !text.is_empty() {
        print!("{}", format_repl_output(&text, colorize));
        if !text.ends_with('\n') {
            println!();
        }
    }
}

fn build_editor(colorize: bool) -> Result<Editor<ReplHelper, DefaultHistory>> {
    let config = Config::builder()
        .completion_type(CompletionType::List)
        .auto_add_history(false)
        .build();
    let mut editor = Editor::<ReplHelper, DefaultHistory>::with_config(config)?;
    editor.set_helper(Some(ReplHelper { colorize }));

    if let Some(history_path) = repl_history_path() {
        if let Some(parent) = history_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let _ = editor.load_history(&history_path);
    }
    Ok(editor)
}

fn save_history(editor: &mut Editor<ReplHelper, DefaultHistory>) {
    if let Some(history_path) = repl_history_path() {
        let _ = editor.save_history(&history_path);
    }
}

fn start_bridge(cli: &Cli) -> Result<BridgeClient> {
    if let Some(python) = cli.python.as_deref() {
        return BridgeClient::new(spawn_bridge_via_module(python, cli)?);
    }
    if backend_command_available("stata-cli-backend") {
        return BridgeClient::new(spawn_bridge_via_backend_command("stata-cli-backend", cli)?);
    }
    if let Ok(repo_root) = resolve_repo_root() {
        let python = resolve_python(cli.python.as_deref(), &repo_root.path)?;
        return BridgeClient::new(spawn_bridge_with_project_python(
            &python.path,
            &repo_root.path,
            cli,
        )?);
    }
    bail!(
        "Could not start repl from the current directory. Activate an environment that provides `stata-cli-backend`, pass `--python` to a Python 3.11 interpreter with `stata_cli_backend` installed, or configure {}.",
        PROJECT_ROOT_ENV
    )
}

pub(crate) fn repl_command(cli: &Cli) -> Result<()> {
    let colorize = io::stdout().is_terminal();
    let mut bridge = start_bridge(cli)?;
    let mut editor = build_editor(colorize)?;
    clear_screen()?;

    let mut prompt = ". ";
    let mut buffer: Vec<String> = Vec::new();

    loop {
        match editor.readline(prompt) {
            Ok(line) => {
                let stripped = line.trim().to_string();
                if buffer.is_empty() && stripped.is_empty() {
                    continue;
                }
                if buffer.is_empty() && (stripped == ":exit" || stripped == ":quit") {
                    break;
                }
                if !line.trim().is_empty() {
                    let _ = editor.add_history_entry(line.as_str());
                }
                buffer.push(line);

                if stripped.ends_with("///") {
                    prompt = "> ";
                    continue;
                }

                let code = buffer.join("\n");
                let request_working_dir = cli
                    .working_dir
                    .as_ref()
                    .map(|item| item.to_string_lossy().to_string());
                let result = bridge.execute(&code, request_working_dir.as_deref(), cli.timeout)?;
                print_result(&result, colorize);
                buffer.clear();
                prompt = ". ";
            }
            Err(ReadlineError::Interrupted) => {
                println!();
                buffer.clear();
                prompt = ". ";
                continue;
            }
            Err(ReadlineError::Eof) => {
                println!();
                break;
            }
            Err(error) => {
                let _ = bridge.shutdown();
                save_history(&mut editor);
                return Err(error).context("Rust REPL failed");
            }
        }
    }

    save_history(&mut editor);
    bridge.shutdown()?;
    Ok(())
}
