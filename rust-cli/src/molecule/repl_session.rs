use crate::atom::cli_contract::Cli;
use crate::atom::completion_cache::CompletionCache;
use crate::atom::json_contract::{BridgeRequest, CompletionContextResult, ExecutionResult};
use crate::atom::path_ops::repl_history_path;
use crate::atom::process_runner::backend_command_available;
use crate::atom::progress_feedback::{
    backend_heartbeat_message, clear_terminal_line, heartbeat_interval, prompt_status_line,
    spinner_interval, spinner_message,
};
use crate::atom::repl_formatting::{
    format_repl_output, highlight_input_line, sanitize_repl_output,
};
use crate::molecule::backend_client::{
    spawn_bridge_via_backend_command, spawn_bridge_via_module, spawn_bridge_with_project_python,
};
use crate::molecule::repl_completion::{
    completion_hint, completion_pairs, update_cache_from_snapshot,
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
use std::collections::HashSet;
use std::io::{self, BufRead, BufReader, IsTerminal, Write};
use std::process::{Child, ChildStdin, ChildStdout};
use std::sync::{Arc, Mutex};
use std::thread;

struct ReplHelper {
    colorize: bool,
    bridge: Arc<Mutex<BridgeClient>>,
    buffer_words: HashSet<String>,
    completion_cache: Mutex<CompletionCache>,
}

impl ReplHelper {
    fn remember_line(&mut self, line: &str) {
        let mut current = String::new();
        for ch in line.chars() {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '.' {
                current.push(ch);
                continue;
            }
            self.push_word(&mut current);
        }
        self.push_word(&mut current);
    }

    fn push_word(&mut self, current: &mut String) {
        let should_keep = current.len() >= 2
            && current
                .chars()
                .next()
                .map(|ch| ch.is_ascii_alphabetic() || ch == '_')
                .unwrap_or(false);
        if should_keep {
            self.buffer_words.insert(current.to_ascii_lowercase());
        }
        current.clear();
    }

    fn invalidate_completion_cache(&mut self) {
        self.completion_cache
            .lock()
            .expect("completion cache mutex poisoned")
            .invalidate();
    }

    fn refresh_completion_cache(&mut self) {
        let snapshot = self
            .bridge
            .lock()
            .expect("bridge mutex poisoned")
            .completion_snapshot();
        match snapshot {
            Ok(snapshot) if snapshot.status == "success" => {
                update_cache_from_snapshot(
                    &mut self
                        .completion_cache
                        .lock()
                        .expect("completion cache mutex poisoned"),
                    &snapshot,
                );
            }
            _ => self
                .completion_cache
                .lock()
                .expect("completion cache mutex poisoned")
                .invalidate(),
        }
    }
}

impl Helper for ReplHelper {}

impl Completer for ReplHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &RustyContext<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let mut completion_cache = self
            .completion_cache
            .lock()
            .expect("completion cache mutex poisoned");
        if !completion_cache.is_valid() {
            if let Ok(snapshot) = self
                .bridge
                .lock()
                .expect("bridge mutex poisoned")
                .completion_snapshot()
            {
                if snapshot.status == "success" {
                    update_cache_from_snapshot(&mut completion_cache, &snapshot);
                }
            }
        }

        Ok(completion_pairs(
            line,
            pos,
            &self.buffer_words,
            &completion_cache,
        ))
    }
}

impl Hinter for ReplHelper {
    type Hint = String;

    fn hint(&self, line: &str, pos: usize, _ctx: &RustyContext<'_>) -> Option<String> {
        let mut completion_cache = self
            .completion_cache
            .lock()
            .expect("completion cache mutex poisoned");
        if !completion_cache.is_valid() {
            if let Ok(snapshot) = self
                .bridge
                .lock()
                .expect("bridge mutex poisoned")
                .completion_snapshot()
            {
                if snapshot.status == "success" {
                    update_cache_from_snapshot(&mut completion_cache, &snapshot);
                }
            }
        }

        completion_hint(line, pos, &self.buffer_words, &completion_cache)
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
        if self.colorize {
            Cow::Owned(format!("\x1b[90m{hint}\x1b[0m"))
        } else {
            Cow::Borrowed(hint)
        }
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
        show_progress: bool,
    ) -> Result<ExecutionResult> {
        let request = BridgeRequest {
            command: "run".to_string(),
            code: Some(code.to_string()),
            working_dir: working_dir.map(ToOwned::to_owned),
            timeout,
            prefix: None,
            context_kind: None,
        };
        self.send_request(&request)?;
        if show_progress {
            self.read_execution_response_with_spinner()
        } else {
            self.read_execution_response()
        }
    }

    fn completion_snapshot(&mut self) -> Result<CompletionContextResult> {
        let request = BridgeRequest {
            command: "complete_context".to_string(),
            code: None,
            working_dir: None,
            timeout: None,
            prefix: None,
            context_kind: None,
        };
        self.send_request(&request)?;
        self.read_completion_response()
    }

    fn shutdown(&mut self) -> Result<()> {
        let request = BridgeRequest {
            command: "quit".to_string(),
            code: None,
            working_dir: None,
            timeout: None,
            prefix: None,
            context_kind: None,
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

    fn read_response_line(&mut self) -> Result<String> {
        let mut line = String::new();
        let bytes = self.stdout.read_line(&mut line)?;
        if bytes == 0 {
            bail!("Backend bridge exited before returning a response");
        }
        Ok(line)
    }

    fn read_execution_response(&mut self) -> Result<ExecutionResult> {
        read_execution_response_from(&mut self.stdout)
    }

    fn read_execution_response_with_spinner(&mut self) -> Result<ExecutionResult> {
        let child = &mut self.child;
        let stdout = &mut self.stdout;
        thread::scope(|scope| {
            let response_reader = scope.spawn(move || read_execution_response_from(stdout));
            let started = std::time::Instant::now();
            let heartbeat = heartbeat_interval();
            let mut next_heartbeat = heartbeat;
            let mut frame_index = 0usize;

            while !response_reader.is_finished() {
                if let Some(status) = child.try_wait()? {
                    print!("{}", clear_terminal_line());
                    io::stdout().flush()?;
                    bail!("Backend bridge exited before returning a response: {status}");
                }

                let elapsed = started.elapsed();
                print!(
                    "{}{}",
                    clear_terminal_line(),
                    spinner_message(elapsed, frame_index, true)
                );
                io::stdout().flush()?;
                frame_index = frame_index.wrapping_add(1);

                if elapsed >= next_heartbeat {
                    print!(
                        "{}{}",
                        clear_terminal_line(),
                        backend_heartbeat_message(elapsed)
                    );
                    io::stdout().flush()?;
                    next_heartbeat += heartbeat;
                }

                thread::sleep(spinner_interval());
            }

            print!("{}", clear_terminal_line());
            io::stdout().flush()?;
            response_reader
                .join()
                .map_err(|_| anyhow::anyhow!("Backend bridge response reader panicked"))?
        })
    }

    fn read_completion_response(&mut self) -> Result<CompletionContextResult> {
        let line = self.read_response_line()?;
        serde_json::from_str(line.trim_end())
            .context("Backend bridge returned invalid completion JSON")
    }
}

fn read_execution_response_from(stdout: &mut BufReader<ChildStdout>) -> Result<ExecutionResult> {
    let mut line = String::new();
    let bytes = stdout.read_line(&mut line)?;
    if bytes == 0 {
        bail!("Backend bridge exited before returning a response");
    }
    serde_json::from_str(line.trim_end()).context("Backend bridge returned invalid execution JSON")
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

fn mark_submitted_prompts(lines: &[String], success: bool, colorize: bool) -> io::Result<()> {
    if !colorize || lines.is_empty() {
        return Ok(());
    }

    for (index, line) in lines.iter().enumerate() {
        let distance_from_bottom = lines.len() - index;
        let prefix = if index == 0 { "." } else { ">" };
        print!(
            "\x1b[{distance_from_bottom}A\r\x1b[2K{}\x1b[{distance_from_bottom}B\r",
            prompt_status_line(prefix, line, success, colorize)
        );
    }
    io::stdout().flush()
}

fn build_editor(
    colorize: bool,
    bridge: Arc<Mutex<BridgeClient>>,
) -> Result<Editor<ReplHelper, DefaultHistory>> {
    let config = Config::builder()
        .completion_type(CompletionType::List)
        .auto_add_history(false)
        .build();
    let mut editor = Editor::<ReplHelper, DefaultHistory>::with_config(config)?;
    editor.set_helper(Some(ReplHelper {
        colorize,
        bridge,
        buffer_words: HashSet::new(),
        completion_cache: Mutex::new(CompletionCache::default()),
    }));

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
    let bridge = Arc::new(Mutex::new(start_bridge(cli)?));
    let mut editor = build_editor(colorize, Arc::clone(&bridge))?;
    if let Some(helper) = editor.helper_mut() {
        helper.refresh_completion_cache();
    }
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
                    if let Some(helper) = editor.helper_mut() {
                        helper.remember_line(line.as_str());
                    }
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
                let result = match bridge.lock().expect("bridge mutex poisoned").execute(
                    &code,
                    request_working_dir.as_deref(),
                    cli.timeout,
                    colorize,
                ) {
                    Ok(result) => result,
                    Err(error) => {
                        mark_submitted_prompts(&buffer, false, colorize)?;
                        eprintln!("stata-cli repl: {error}");
                        return Err(error);
                    }
                };
                mark_submitted_prompts(&buffer, result.status == "success", colorize)?;
                print_result(&result, colorize);
                if let Some(helper) = editor.helper_mut() {
                    helper.invalidate_completion_cache();
                    helper.refresh_completion_cache();
                }
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
                let _ = bridge.lock().expect("bridge mutex poisoned").shutdown();
                save_history(&mut editor);
                return Err(error).context("Rust REPL failed");
            }
        }
    }

    save_history(&mut editor);
    bridge.lock().expect("bridge mutex poisoned").shutdown()?;
    Ok(())
}
