Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$manifestPath = Join-Path $repoRoot "rust-cli/Cargo.toml"
$binDir = Join-Path $repoRoot "skill/stata-cli/bin"
$targetTriple = if ($env:WINDOWS_TARGET_TRIPLE) { $env:WINDOWS_TARGET_TRIPLE } else { "x86_64-pc-windows-gnu" }
$targetBinary = Join-Path $repoRoot "rust-cli/target/$targetTriple/release/stata-cli.exe"
$cargoBin = if ($env:CARGO_BIN) {
    $env:CARGO_BIN
} elseif (Test-Path (Join-Path $HOME ".cargo/bin/cargo.exe")) {
    Join-Path $HOME ".cargo/bin/cargo.exe"
} elseif (Test-Path (Join-Path $HOME ".cargo/bin/cargo")) {
    Join-Path $HOME ".cargo/bin/cargo"
} else {
    (Get-Command cargo).Source
}
$rustupBin = if ($env:RUSTUP_BIN) {
    $env:RUSTUP_BIN
} elseif (Test-Path (Join-Path $HOME ".cargo/bin/rustup.exe")) {
    Join-Path $HOME ".cargo/bin/rustup.exe"
} elseif (Test-Path (Join-Path $HOME ".cargo/bin/rustup")) {
    Join-Path $HOME ".cargo/bin/rustup"
} else {
    (Get-Command rustup).Source
}
$cargoHomeBin = Join-Path $HOME ".cargo/bin"

if (Test-Path $cargoHomeBin) {
    $env:PATH = "$cargoHomeBin$([IO.Path]::PathSeparator)$env:PATH"
}

New-Item -ItemType Directory -Force -Path $binDir | Out-Null

try {
    & $cargoBin zigbuild --help *> $null
} catch {
    Write-Error "[build_windows_bin] cargo-zigbuild is required. Install it with: cargo install cargo-zigbuild --locked"
}

$installedTargets = & $rustupBin target list --installed
if ($installedTargets -notcontains $targetTriple) {
    Write-Host "[build_windows_bin] Installing Rust target $targetTriple..."
    & $rustupBin target add $targetTriple
}

Write-Host "[build_windows_bin] Building Rust CLI for Windows target $targetTriple..."
& $cargoBin zigbuild --release --target $targetTriple --manifest-path $manifestPath

if (-not (Test-Path $targetBinary)) {
    Write-Error "[build_windows_bin] Expected binary not found: $targetBinary"
}

Copy-Item -Force $targetBinary (Join-Path $binDir "stata-cli.exe")
Write-Host "[build_windows_bin] Updated $(Join-Path $binDir 'stata-cli.exe')"
