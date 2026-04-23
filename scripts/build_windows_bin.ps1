Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$manifestPath = Join-Path $repoRoot "rust-cli/Cargo.toml"
$binDir = Join-Path $repoRoot "bin"
$targetTriple = if ($env:WINDOWS_TARGET_TRIPLE) { $env:WINDOWS_TARGET_TRIPLE } else { "x86_64-pc-windows-gnu" }
$targetBinary = Join-Path $repoRoot "rust-cli/target/$targetTriple/release/stata-cli.exe"

New-Item -ItemType Directory -Force -Path $binDir | Out-Null

try {
    cargo zigbuild --help *> $null
} catch {
    Write-Error "[build_windows_bin] cargo-zigbuild is required. Install it with: cargo install cargo-zigbuild --locked"
}

Write-Host "[build_windows_bin] Building Rust CLI for Windows target $targetTriple..."
cargo zigbuild --release --target $targetTriple --manifest-path $manifestPath

if (-not (Test-Path $targetBinary)) {
    Write-Error "[build_windows_bin] Expected binary not found: $targetBinary"
}

Copy-Item -Force $targetBinary (Join-Path $binDir "stata-cli.exe")
Write-Host "[build_windows_bin] Updated $(Join-Path $binDir 'stata-cli.exe')"
