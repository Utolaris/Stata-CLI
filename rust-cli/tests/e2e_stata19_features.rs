//! End-to-end tests for the Stata 19 feature set (verified on Stata 19.5).
//!
//! One test per headline feature from the Stata 19 release notes
//! (`whatsnew18to19.sthlp`): panel VAR (`xtvar`), high-dimensional fixed
//! effects, CATE (`cate`), weak-instrument-robust inference
//! (`estat weakrobust`), control functions (`cfregress`/`cfprobit`),
//! SVAR via instruments (`ivsvar`), IV local projections (`ivlpirf`),
//! correlated random effects + Mundlak test (`xtreg, cre`), and GMM with
//! panel-style instruments (`gmm ... xtinstruments()`).
//!
//! All tests use synthetic data generated inside Stata, so they are
//! deterministic and need no network access. Skipped when `SKIP_STATA_TESTS`
//! is set (CI) or when no Stata installation is found.

use serde_json::Value;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("rust-cli should live under the repo root")
        .to_path_buf()
}

fn stata_home() -> Option<PathBuf> {
    if env::var_os("SKIP_STATA_TESTS").is_some() {
        return None;
    }
    env::var_os("STATA_PATH").map(PathBuf::from).or_else(|| {
        ["/Applications/StataNow", "/Applications/Stata"]
            .iter()
            .map(PathBuf::from)
            .find(|path| path.is_dir())
    })
}

fn base_command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_stata-cli"));
    command.env("STATA_CLI_PROJECT_ROOT", repo_root());
    if let Some(stata) = stata_home() {
        command.arg("--stata-path").arg(stata);
    }
    command
}

fn run_code(code: &str) -> Value {
    let output = base_command()
        .args(["run", "--code", code])
        .output()
        .expect("run stata-cli");
    assert!(
        output.status.success(),
        "stata-cli failed: stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        result["status"], "success",
        "stata run failed: {}",
        result["error"]
    );
    result
}

fn assert_output_contains(result: &Value, needle: &str) {
    let output = result["output"].as_str().unwrap_or_default();
    assert!(
        output.contains(needle),
        "expected output to contain {needle:?}; got:\n{output}"
    );
}

/// Panel-data vector autoregression (`xtvar`), new in Stata 19.
#[test]
fn e2e_stata19_panel_var_xtvar() {
    let Some(_stata) = stata_home() else {
        eprintln!("skipping e2e xtvar (no Stata)");
        return;
    };
    let result = run_code(
        r#"
        clear
        set seed 42
        set obs 1200
        gen id = ceil(_n/12)
        gen t = mod(_n-1,12)+1
        xtset id t
        gen y1 = rnormal()
        gen y2 = rnormal()
        replace y1 = 0.5*L.y1 + 0.2*L.y2 + rnormal() if t>1
        replace y2 = 0.1*L.y1 + 0.5*L.y2 + rnormal() if t>1
        xtvar y1 y2, lags(1) maxldep(1)
        "#,
    );
    assert_output_contains(&result, "Panel-data vector autoregression");
    assert_output_contains(&result, "Number of groups");
    assert_output_contains(&result, "GMM-type instruments");
}

/// High-dimensional fixed effects: `areg`, `xtreg, fe`, and
/// `ivregress 2sls` absorb multiple categorical variables.
#[test]
fn e2e_stata19_hdfe_absorb() {
    let Some(_stata) = stata_home() else {
        eprintln!("skipping e2e HDFE (no Stata)");
        return;
    };
    let result = run_code(
        r#"
        clear
        set seed 7
        set obs 2000
        gen firm = ceil(_n/100)
        gen year = mod(_n-1,20)+1
        gen ind = mod(_n-1,10)+1
        gen x = rnormal()
        gen u = rnormal()
        gen z = rnormal()
        gen w = 0.8*z + 0.5*u + rnormal()
        gen y = 1.2*x + 0.7*w + u + 0.05*firm + 0.02*year + rnormal()
        areg y x, absorb(firm year ind)
        clear
        set seed 7
        set obs 400
        gen firm = ceil(_n/20)
        gen year = mod(_n-1,20)+1
        gen ind = mod(_n-1,10)+1
        gen x = rnormal()
        gen u = rnormal()
        gen z = rnormal()
        gen w = 0.8*z + 0.5*u + rnormal()
        gen y = 1.2*x + 0.7*w + u + 0.05*firm + 0.02*year + rnormal()
        xtset firm year
        xtreg y x, fe absorb(ind)
        ivregress 2sls y x (w = z), absorb(firm year)
        "#,
    );
    assert_output_contains(&result, "Linear regression, absorbing indicators");
    assert_output_contains(&result, "Absorbed variable");
    assert_output_contains(&result, "Fixed-effects (within) regression");
    assert_output_contains(&result, "Instrumental-variables 2SLS regression");
}

/// Conditional average treatment effects (`cate`), new in Stata 19.
#[test]
fn e2e_stata19_cate() {
    let Some(_stata) = stata_home() else {
        eprintln!("skipping e2e CATE (no Stata)");
        return;
    };
    let result = run_code(
        r#"
        clear
        set seed 11
        set obs 1000
        gen x1 = rnormal()
        gen x2 = rnormal()
        gen g = floor(runiform()*4)+1
        gen treat = rbinomial(1,0.5)
        gen y = 1 + 0.5*treat + 0.3*x1 + 0.2*x2 + 0.4*treat*x1 + rnormal()
        cate po (y x1 x2 i.g) (treat)
        cate, reestimate group(g)
        "#,
    );
    assert_output_contains(&result, "Conditional average treatment effects");
    assert_output_contains(&result, "Estimating GATE");
    assert_output_contains(&result, "ATE");
}

/// Weak-instrument-robust inference after `ivregress`: `estat weakrobust`
/// (Anderson-Rubin / conditional likelihood-ratio).
#[test]
fn e2e_stata19_estat_weakrobust() {
    let Some(_stata) = stata_home() else {
        eprintln!("skipping e2e weakrobust (no Stata)");
        return;
    };
    let result = run_code(
        r#"
        clear
        set seed 5
        set obs 2000
        gen z1 = rnormal()
        gen z2 = rnormal()
        gen x = z1 + z2 + rnormal()
        gen y = 1 + 0.8*x + rnormal()
        ivregress 2sls y (x = z1 z2)
        estat weakrobust
        "#,
    );
    assert_output_contains(&result, "Test robust to weak instruments");
    assert_output_contains(&result, "CLR");
    assert_output_contains(&result, "Cond. likelihood-ratio");
}

/// Control-function linear and probit models (`cfregress`, `cfprobit`).
#[test]
fn e2e_stata19_control_functions() {
    let Some(_stata) = stata_home() else {
        eprintln!("skipping e2e control functions (no Stata)");
        return;
    };
    let result = run_code(
        r#"
        clear
        set seed 13
        set obs 1500
        gen z = rnormal()
        gen u = rnormal()
        gen x = 0.8*z + u + rnormal()
        gen y = 2 + 0.6*x + u + rnormal()
        gen yb = (2 + 0.6*x + u + rnormal() > 0)
        cfregress y (x = z)
        cfprobit yb (x = z)
        "#,
    );
    assert_output_contains(&result, "Control-function linear regression");
    assert_output_contains(&result, "Control-function probit regression");
    assert_output_contains(&result, "Instrument for x: z");
}

/// SVAR identified via instrumental variables (`ivsvar gmm`), new in Stata 19.
#[test]
fn e2e_stata19_svar_with_iv() {
    let Some(_stata) = stata_home() else {
        eprintln!("skipping e2e ivsvar (no Stata)");
        return;
    };
    let result = run_code(
        r#"
        clear
        set seed 17
        set obs 1000
        gen t = _n
        tsset t
        gen z = rnormal()
        gen y1 = rnormal()
        gen y2 = rnormal()
        gen y3 = rnormal()
        replace y1 = 0.5*L.y1 + 0.2*L.y2 + 0.8*z + rnormal() if t>1
        replace y2 = 0.3*L.y1 + 0.5*L.y2 + rnormal() if t>1
        replace y3 = 0.2*L.y1 + 0.8*z + rnormal() if t>1
        ivsvar gmm y1 y2 (y3 = z)
        "#,
    );
    assert_output_contains(&result, "Instrumental-variables SVAR");
    assert_output_contains(&result, "Number of obs");
}

/// Instrumental-variables local projections (`ivlpirf`), new in Stata 19.
#[test]
fn e2e_stata19_iv_local_projections() {
    let Some(_stata) = stata_home() else {
        eprintln!("skipping e2e ivlpirf (no Stata)");
        return;
    };
    let result = run_code(
        r#"
        clear
        set seed 17
        set obs 1000
        gen t = _n
        tsset t
        gen z = rnormal()
        gen y1 = rnormal()
        gen y2 = rnormal()
        replace y1 = 0.5*L.y1 + 0.2*L.y2 + 0.8*z + rnormal() if t>1
        replace y2 = 0.3*L.y1 + 0.5*L.y2 + rnormal() if t>1
        ivlpirf y1 y2, endogenous(y1 = z) step(4)
        "#,
    );
    assert_output_contains(
        &result,
        "Instrumental-variables local-projection impulse responses",
    );
    assert_output_contains(&result, "IRF");
}

/// Correlated random effects (`xtreg, cre`) plus the Mundlak specification
/// test (`estat mundlak`), both new in Stata 19.
#[test]
fn e2e_stata19_cre_mundlak() {
    let Some(_stata) = stata_home() else {
        eprintln!("skipping e2e CRE/Mundlak (no Stata)");
        return;
    };
    let result = run_code(
        r#"
        clear
        set seed 19
        set obs 1000
        gen id = ceil(_n/10)
        gen t = mod(_n-1,10)+1
        xtset id t
        gen x = rnormal() + 0.4*id
        gen y = 1 + 0.8*x + id + rnormal()
        xtreg y x, re
        estat mundlak
        xtreg y x, cre
        estat mundlak
        "#,
    );
    assert_output_contains(&result, "Mundlak specification test");
    assert_output_contains(&result, "Correlated random-effects regression");
    assert_output_contains(&result, "Mundlak test (xt_means = 0)");
}

/// GMM with panel-style instruments (`gmm ... xtinstruments()`) on a dynamic
/// panel; also checks the Arellano-Bond rho estimate lands near the true 0.5.
#[test]
fn e2e_stata19_gmm_xtinstruments() {
    let Some(_stata) = stata_home() else {
        eprintln!("skipping e2e GMM xtinstruments (no Stata)");
        return;
    };
    let result = run_code(
        r#"
        clear
        set seed 23
        set obs 600
        gen id = ceil(_n/6)
        gen t = mod(_n-1,6)+1
        xtset id t
        gen y = rnormal()
        replace y = 0.5*L.y + rnormal() if t>1
        gmm (D.y - {rho}*LD.y), xtinstruments(y, lags(2/.)) winitial(xt D) onestep
        display "RHO_EST=" %9.6f _b[/rho]
        "#,
    );
    assert_output_contains(&result, "GMM estimation");
    assert_output_contains(&result, "XT-style");
    let output = result["output"].as_str().unwrap_or_default();
    let rho_line = output
        .lines()
        .find(|line| line.trim_start().starts_with("RHO_EST="))
        .unwrap_or_else(|| panic!("expected RHO_EST line in output:\n{output}"));
    let rho: f64 = rho_line
        .split("RHO_EST=")
        .nth(1)
        .unwrap()
        .trim()
        .parse()
        .unwrap_or_else(|_| panic!("could not parse rho from {rho_line:?}"));
    assert!(
        (0.2..0.8).contains(&rho),
        "Arellano-Bond rho estimate {rho} should be near the true 0.5"
    );
}
