param([switch]$Media, [switch]$Deny)
$ErrorActionPreference = 'Stop'
$lumerynRoot = Split-Path $PSScriptRoot -Parent
Push-Location $lumerynRoot
try {
    cargo fmt --check
    if ($LASTEXITCODE -ne 0) { throw 'Rust formatting failed' }
    cargo clippy --locked --all-targets -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw 'Clippy failed' }
    cargo test --locked
    if ($LASTEXITCODE -ne 0) { throw 'Tests failed' }
    if ($Media) {
        cargo test --locked --test workflow --test media_regressions -- --include-ignored
        if ($LASTEXITCODE -ne 0) { throw 'Media acceptance failed' }
    }
    if ($Deny) {
        $lumerynDeny = Join-Path $lumerynRoot '.lumeryn-scratch/tools/bin/cargo-deny.exe'
        if (Test-Path -LiteralPath $lumerynDeny) {
            & $lumerynDeny --locked check --hide-inclusion-graph
        } else {
            cargo deny --locked check --hide-inclusion-graph
        }
        if ($LASTEXITCODE -ne 0) { throw 'Dependency/security/license checks failed' }
    }
} finally { Pop-Location }
