# Security Policy

## System and Scope

`stata-cli` is a local command-line tool that runs Stata code, `.do` files,
and `.dta` data by loading Stata's own shared library (`libstata-mp.dylib`)
into the CLI process. It supports Stata 18 and 19 (including StataNow) on
macOS (arm64) and Windows (x86-64), with prebuilt binaries shipped in the
`stata-cli.skill` package under `skill/stata-cli/bin/`.

In scope: the Rust CLI (`rust-cli/`), the skill package (`skill/`), and the CI
workflows that build, sign, and publish the binaries. The Stata application
itself is a commercial third-party product and is out of scope.

## Threat Model and Trust Boundaries

- The CLI runs locally on the user's machine; it is not network-facing and
  serves no remote requests.
- Attacker-controlled inputs include Stata code and `.do` files executed via
  `stata-cli run`/`stata-cli file`, `.dta` files read by `stata-cli data`,
  and CLI arguments (paths, working directories, session IDs).
- Executing user-supplied Stata code is the tool's purpose: the in-process
  engine is not a sandbox, and code runs with the privileges of the invoking
  user. The CLI must never widen that boundary (for example by fetching and
  executing remote code implicitly).
- GitHub tokens and CI credentials are handled by GitHub Actions and must
  never be committed, logged, or embedded in binaries or documentation.

## Security Invariants

- All `unsafe` code is confined to `rust-cli/src/atom/stata_engine.rs` (the
  StataSO FFI bridge); no other module may use `unsafe`.
- Paths and working directories passed to Stata are quoted/escaped so they
  cannot inject commands or break out of the intended argument.
- No secrets, tokens, or personal paths are written to logs, config files, or
  generated artifacts.
- Output capture is bounded (buffer and drain limits) so pathological Stata
  output cannot exhaust memory.
- Release binaries in `skill/stata-cli/bin/` are built by CI from tagged
  source; macOS builds are ad-hoc signed so the OS accepts them.

## Reportable Findings and Severity Context

- Memory-safety or crash issues in the FFI bridge, path handling, or data
  parsing, reachable from untrusted `.dta`/`.do` input (high).
- Command or argument injection via filenames, working directories, or
  session IDs (high if it enables code execution beyond the intended command).
- Secrets or tokens leaked through logs, CI artifacts, or release assets
  (high).
- Supply chain: vulnerable Rust dependencies, tampered binaries in the skill
  package, or CI configuration that could publish untrusted artifacts
  (medium-high).
- Engine crashes caused by unusual-but-valid data are expected behavior of the
  embedded third-party Stata engine, not a CLI defect.

## Out of Scope, Exclusions, and Accepted Risk

- The Stata application, its license, and its DRM.
- Execution of user-supplied Stata code by design; report only boundary
  violations (for example, code running beyond what the user asked for).
- Non-security bugs and performance issues.

## Known Limitations and Compensating Controls

- The engine runs in-process, so a Stata engine crash can take down the CLI;
  parallel sessions run as separate OS processes.
- CI cannot run a licensed Stata, so Stata-dependent tests are skipped there
  and run locally instead.
- The Windows binary is cross-compiled on macOS CI (zigbuild) and smoke-tested
  but not exercised against a licensed Stata in CI.

## Reporting

Report vulnerabilities privately via GitHub Private Vulnerability Reporting:
<https://github.com/Utolaris/Stata-CLI/security/advisories/new>
