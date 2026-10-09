param([switch]$Media, [switch]$Deny)
$ErrorActionPreference = 'Stop'
$lumerynRoot = Split-Path $PSScriptRoot -Parent
Push-Location $lumerynRoot
try {
    cargo fmt --check
    if ($LASTEXITCODE -ne 0) { throw 'Rust formatting failed' }
    cargo clippy --locked --all-targets -j 4 -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw 'Clippy failed' }
    cargo test --locked -j 4
    if ($LASTEXITCODE -ne 0) { throw 'Tests failed' }
    if ($Media) {
        cargo test --locked --test workflow --test media_regressions --test live_service -j 4 -- --include-ignored
        if ($LASTEXITCODE -ne 0) { throw 'Media acceptance failed' }
        cargo test -p lumeryn-preview -p lumeryn-service --locked -j 4 -- --include-ignored --test-threads=1
        if ($LASTEXITCODE -ne 0) { throw 'Native preview/service media acceptance failed' }
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
