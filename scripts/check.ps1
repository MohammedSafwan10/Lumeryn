param([switch]$Media, [switch]$Deny)
$ErrorActionPreference = 'Stop'
# Honor isolated Cargo output so acceptance never replaces a running user editor.
$lumerynTarget = if ($env:CARGO_TARGET_DIR) { [IO.Path]::GetFullPath($env:CARGO_TARGET_DIR, (Split-Path $PSScriptRoot -Parent)) } else { Join-Path (Split-Path $PSScriptRoot -Parent) 'target' }
$lumerynRoot = Split-Path $PSScriptRoot -Parent
function Wait-LumerynTestService {
    # Integration tests start the real CLI service, which exits after ten idle seconds.
    # Windows prevents Cargo replacing that executable until the service releases it.
    $testWatch = [Diagnostics.Stopwatch]::StartNew()
    $testBinary = Join-Path $lumerynTarget 'debug/lumeryn.exe'
    while (Test-Path -LiteralPath $testBinary) {
        $testHandle = $null
        try {
            $testHandle = [IO.File]::Open($testBinary, [IO.FileMode]::Open, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
            return
        } catch [IO.IOException] {
            if ($testWatch.Elapsed.TotalSeconds -ge 12) {
                throw 'CLI executable remains in use. Close your own active session before rebuilding; no process was terminated.'
            }
            Start-Sleep -Milliseconds 100
        } finally { if ($testHandle) { $testHandle.Dispose() } }
    }
}
Push-Location $lumerynRoot
try {
    cargo fmt --check
    if ($LASTEXITCODE -ne 0) { throw 'Rust formatting failed' }
    Wait-LumerynTestService
    cargo clippy --locked --all-targets -j 4 -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw 'Clippy failed' }
    Wait-LumerynTestService
    cargo test --locked -j 4
    if ($LASTEXITCODE -ne 0) { throw 'Tests failed' }
    if ($Media) {
        cargo test -p lumeryn-core --lib --locked -j 4 -- --include-ignored
        if ($LASTEXITCODE -ne 0) { throw 'Core audio scheduling acceptance failed' }
        Wait-LumerynTestService
        cargo test --locked --test workflow --test media_regressions --test live_service --test uhd_export --test history_workflow --test undo_redo --test media_management --test audio_probe --test audio_graph --test audio_migration --test audio_composition --test audio_workflow -j 4 -- --include-ignored
        if ($LASTEXITCODE -ne 0) { throw 'Media acceptance failed' }
        Wait-LumerynTestService
        cargo test -p lumeryn-preview -p lumeryn-service --locked -j 4 -- --include-ignored --test-threads=1
        if ($LASTEXITCODE -ne 0) { throw 'Native preview/service media acceptance failed' }
        Wait-LumerynTestService
        cargo test -p lumeryn-studio --locked -j 4 -- --include-ignored --test-threads=1
        if ($LASTEXITCODE -ne 0) { throw 'Studio media presentation acceptance failed' }
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
