//! Native in-process bridge to Stata's shared library (`libstata-mp.dylib`).
//!
//! # Unsafe FFI (documented exception)
//!
//! This module is the single, deliberate exception to the project's no-unsafe
//! convention. Loading Stata's engine into this process and calling its
//! exported `StataSO_*` C ABI is inherently unsafe in the Rust sense:
//!
//! - `StataSO_Main` initializes a large C codebase inside our process and
//!   installs global state (`_console`, `_stopflag`, `_caller_python`, ...).
//! - `StataSO_Execute` runs arbitrary Stata code supplied by the user, exactly
//!   like the official Python bridge (`pystata`) does.
//! - `StataSO_SetBreak` mutates engine globals and may be called from another
//!   thread while an execution is in flight (the same pattern pystata uses for
//!   its stop monitor).
//!
//! The unsafe surface is intentionally confined to this module and exposed
//! through a small safe API (`StataEngine::new`, `execute`, `run_block`,
//! `set_break`, `shutdown`). No other module in the crate touches raw
//! pointers or `extern "C"` calls. See README.md, "Unsafe FFI" section, for
//! the design decision and risk notes.
//!
//! Notes on the ABI (reverse-engineered from Stata 18 MP, arm64):
//!
//! - `StataSO_Main(argc, argv)` initializes the engine. It accepts
//!   `-pyexec <python>` to attach an embedded Python interpreter; when the
//!   argument is absent the engine runs without any Python (rc = 1). We
//!   deliberately do not pass `-pyexec` so the CLI has zero Python
//!   dependency.
//! - `StataSO_Execute(cmd, echo)` runs one command line and routes output to
//!   an internal output buffer.
//! - `StataSO_GetOutputBuffer()` returns the buffered output (default buffer
//!   size 2 MB; we keep output capture below that by using Stata `log`
//!   files for large runs, mirroring the previous pystata backend).
//! - `StataSO_Shutdown()` shuts the engine down.

#![allow(unsafe_code)]

use anyhow::{bail, Context, Result};
use libloading::{Library, Symbol};
use std::ffi::{CStr, CString};
use std::fs;
use std::os::raw::{c_char, c_int};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

type FnStataSoMain = unsafe extern "C" fn(c_int, *mut *const c_char) -> c_int;
type FnStataSoExecute = unsafe extern "C" fn(*const c_char, c_int) -> c_int;
type FnStataSoGetOutputBuffer = unsafe extern "C" fn() -> *const c_char;
type FnStataSoClearOutputBuffer = unsafe extern "C" fn();
type FnStataSoSetBreak = unsafe extern "C" fn();
type FnStataSoShutdown = unsafe extern "C" fn();

/// Result of a single `StataSO_Execute` call.
#[derive(Debug)]
pub(crate) struct StataOutput {
    pub(crate) rc: i32,
    pub(crate) output: String,
}

/// Loaded Stata engine plus all resolved `StataSO_*` function pointers.
///
/// Raw function pointers are `Send + Sync` and `libloading::Library` is
/// `Send + Sync`, so `Arc<Core>` may be shared with the stop-monitor thread.
struct Core {
    _lib: Library,
    execute: FnStataSoExecute,
    get_output: FnStataSoGetOutputBuffer,
    clear_output: FnStataSoClearOutputBuffer,
    set_break: FnStataSoSetBreak,
    shutdown: FnStataSoShutdown,
}

/// Safe wrapper around one in-process Stata engine.
///
/// A process must host at most one engine (Stata uses process-wide globals),
/// which is why parallel sessions run as separate OS processes in the REPL.
pub(crate) struct StataEngine {
    core: Arc<Core>,
    temp_dir: PathBuf,
    /// Serializes `StataSO_Execute` calls (the C ABI is not reentrant).
    exec_lock: Mutex<()>,
    /// Tracks whether the first-run `set seed` prefix has been applied.
    seed_done: AtomicBool,
    shut_down: AtomicBool,
}

impl StataEngine {
    /// Load `libstata-{edition}.dylib` from `stata_home` and initialize the
    /// engine. No Python is involved: `-pyexec` is intentionally omitted.
    pub(crate) fn new(stata_home: &Path, edition: &str) -> Result<StataEngine> {
        let lib_path = resolve_library_path(stata_home, edition)?;
        std::env::set_var("SYSDIR_STATA", stata_home);
        // Stata MP links its own OpenMP runtime; keep it from colliding with
        // any other OpenMP runtime that might be loaded.
        std::env::set_var("KMP_DUPLICATE_LIB_OK", "True");

        let temp_dir = std::env::temp_dir().join(format!("stata_cli_{}", std::process::id()));
        fs::create_dir_all(&temp_dir).with_context(|| {
            format!(
                "Failed to create Stata temp directory {}",
                temp_dir.display()
            )
        })?;
        std::env::set_var("STATATMP", &temp_dir);

        let core = unsafe {
            let lib = Library::new(&lib_path).with_context(|| {
                format!("Failed to load Stata shared library {}", lib_path.display())
            })?;
            let main: Symbol<FnStataSoMain> = lib
                .get(b"StataSO_Main\0")
                .context("StataSO_Main symbol not found")?;
            let execute: Symbol<FnStataSoExecute> = lib
                .get(b"StataSO_Execute\0")
                .context("StataSO_Execute symbol not found")?;
            let get_output: Symbol<FnStataSoGetOutputBuffer> = lib
                .get(b"StataSO_GetOutputBuffer\0")
                .context("StataSO_GetOutputBuffer symbol not found")?;
            let clear_output: Symbol<FnStataSoClearOutputBuffer> = lib
                .get(b"StataSO_ClearOutputBuffer\0")
                .context("StataSO_ClearOutputBuffer symbol not found")?;
            let set_break: Symbol<FnStataSoSetBreak> = lib
                .get(b"StataSO_SetBreak\0")
                .context("StataSO_SetBreak symbol not found")?;
            let shutdown: Symbol<FnStataSoShutdown> = lib
                .get(b"StataSO_Shutdown\0")
                .context("StataSO_Shutdown symbol not found")?;

            let main = *main;
            let execute = *execute;
            let get_output = *get_output;
            let clear_output = *clear_output;
            let set_break = *set_break;
            let shutdown = *shutdown;

            let argv: Vec<CString> = ["", "-q"]
                .into_iter()
                .map(|s| CString::new(s).expect("static argv has no NUL"))
                .collect();
            let mut argv_ptrs: Vec<*const c_char> = argv.iter().map(|c| c.as_ptr()).collect();
            let rc = main(argv.len() as c_int, argv_ptrs.as_mut_ptr());
            if rc < 0 {
                bail!("Stata engine initialization failed (rc={rc})");
            }

            Core {
                _lib: lib,
                execute,
                get_output,
                clear_output,
                set_break,
                shutdown,
            }
        };

        Ok(StataEngine {
            core: Arc::new(core),
            temp_dir,
            exec_lock: Mutex::new(()),
            seed_done: AtomicBool::new(false),
            shut_down: AtomicBool::new(false),
        })
    }

    /// Directory used for scratch do-files, logs and CSV exports.
    pub(crate) fn temp_dir(&self) -> &Path {
        &self.temp_dir
    }

    /// Execute a single-line Stata command and return its return code plus
    /// the captured output buffer.
    pub(crate) fn execute(&self, cmd: &str) -> StataOutput {
        let _guard = self
            .exec_lock
            .lock()
            .expect("stata engine exec lock poisoned");
        let c = CString::new(cmd).expect("command contains NUL byte");
        unsafe {
            (self.core.clear_output)();
            let rc = (self.core.execute)(c.as_ptr(), 0);
            let out = CStr::from_ptr((self.core.get_output)())
                .to_string_lossy()
                .into_owned();
            StataOutput { rc, output: out }
        }
    }

    /// Execute a multi-line Stata block through a temporary do-file
    /// (`include`), mirroring pystata's multi-line `stata.run` path.
    pub(crate) fn run_block(&self, code: &str) -> StataOutput {
        let do_file = self.temp_dir.join(format!(
            "stata_cli_{}_{}.do",
            std::process::id(),
            now_nanos()
        ));
        if let Err(error) = fs::write(&do_file, code) {
            return StataOutput {
                rc: -1,
                output: format!("failed to write temporary do-file: {error}"),
            };
        }
        let include_cmd = format!("include \"{}\"", do_file.display());
        let result = self.execute(&include_cmd);
        let _ = fs::remove_file(&do_file);
        result
    }

    /// Ask Stata to break out of a long-running execution. May be called from
    /// a monitor thread while another thread is blocked in `execute`.
    ///
    /// Safety: `StataSO_SetBreak` only writes engine break flags; this matches
    /// how pystata's stop monitor works. Call it at most once per execution.
    #[allow(dead_code)] // reserved for a future stop/timeout monitor thread
    pub(crate) fn set_break(&self) {
        unsafe {
            (self.core.set_break)();
        }
    }

    /// Shut the engine down. Safe to call multiple times.
    ///
    /// Note: `StataSO_Shutdown` calls Stata's `_sexit`, which terminates the
    /// current process. Call it only after all output has been emitted (the
    /// REPL does this at exit); ordinary one-shot commands simply let the
    /// process end and do not call it.
    pub(crate) fn shutdown(&self) {
        if !self.shut_down.swap(true, Ordering::SeqCst) {
            unsafe {
                (self.core.shutdown)();
            }
        }
    }

    /// Seed prefix for the first (or still-unconfirmed) execution, mirroring
    /// the old backend's `seed_confirmed` behavior. The seed is only marked
    /// done after a successful run.
    pub(crate) fn seed_prefix(&self) -> String {
        if self.seed_done.load(Ordering::SeqCst) {
            String::new()
        } else {
            format!("quietly set seed {}\n", seed_hash())
        }
    }

    /// Mark the session seed as applied (after a successful execution).
    pub(crate) fn mark_seed_done(&self) {
        self.seed_done.store(true, Ordering::SeqCst);
    }

    /// Fresh per-run seed for `.do` file executions, mirroring the old
    /// backend's `execute_stata_file` behavior.
    pub(crate) fn fresh_seed() -> u64 {
        seed_hash()
    }
}

fn now_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

fn seed_hash() -> u64 {
    // Mirrors the previous Python backend: a stable per-engine seed derived
    // from worker id + pid, masked to 31 bits as Stata requires.
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    format!("{}", std::process::id()).hash(&mut hasher);
    hasher.finish() % 2_147_483_647
}

fn resolve_library_path(stata_home: &Path, edition: &str) -> Result<PathBuf> {
    if !cfg!(target_os = "macos") {
        bail!(
            "Native Stata engine currently supports macOS only (found {})",
            std::env::consts::OS
        );
    }
    let edition = edition.to_lowercase();
    let (app_name, lib_name) = match edition.as_str() {
        "be" => ("StataBE", "libstata-be.dylib"),
        "se" => ("StataSE", "libstata-se.dylib"),
        _ => ("StataMP", "libstata-mp.dylib"),
    };
    if !stata_home.is_dir() {
        bail!("Stata home is not a directory: {}", stata_home.display());
    }
    let lib_path = stata_home
        .join(format!("{app_name}.app"))
        .join("Contents")
        .join("MacOS")
        .join(lib_name);
    if !lib_path.is_file() {
        bail!(
            "Stata shared library not found at {}. Check --stata-path / STATA_PATH.",
            lib_path.display()
        );
    }
    Ok(lib_path)
}
