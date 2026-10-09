# Opens the real native window and modifies only freshly generated disposable projects.
$ErrorActionPreference = 'Stop'
$nativeRoot = Split-Path $PSScriptRoot -Parent
Push-Location $nativeRoot
try {
    cargo build --workspace --locked -j 4
    if ($LASTEXITCODE -ne 0) { throw 'Native workspace build failed' }
    $nativeDemo = (& (Join-Path $PSScriptRoot 'demo.ps1') -Motion | Out-String) | ConvertFrom-Json
    $nativeDirectory = Join-Path $nativeRoot ('.lumeryn-scratch/studio-' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $nativeDirectory | Out-Null
    $nativeArguments = @('--smoke','--project',('"' + $nativeDemo.project + '"'),'--artifact-dir',('"' + $nativeDirectory + '"'))
    $nativeWatch = [Diagnostics.Stopwatch]::StartNew()
    $nativeProcess = Start-Process -FilePath (Join-Path $nativeRoot 'target/debug/lumeryn-studio.exe') -ArgumentList $nativeArguments -WindowStyle Normal -PassThru -RedirectStandardError (Join-Path $nativeDirectory 'stderr.log') -RedirectStandardOutput (Join-Path $nativeDirectory 'stdout.log')
    $nativeSamples = [Collections.Generic.List[object]]::new()
    $nativeGpuSamples = [Collections.Generic.List[object]]::new()
    $nativeSmi = Get-Command nvidia-smi -ErrorAction SilentlyContinue
    $nativeNextGpuMs = 0.0
    while (-not $nativeProcess.WaitForExit(25)) {
        try {
            $nativeProcess.Refresh()
            $nativeSamples.Add([ordered]@{wall_ms=$nativeWatch.Elapsed.TotalMilliseconds;ram_bytes=$nativeProcess.WorkingSet64;cpu_ms=$nativeProcess.TotalProcessorTime.TotalMilliseconds})
        } catch { }
        if ($nativeSmi -and $nativeWatch.Elapsed.TotalMilliseconds -ge $nativeNextGpuMs) {
            $nativeCounters = & $nativeSmi.Source --query-gpu=name,driver_version,memory.total,memory.used,utilization.gpu --format=csv,noheader,nounits 2>$null
            $nativeGpuSamples.Add([ordered]@{wall_ms=$nativeWatch.Elapsed.TotalMilliseconds;global_adapter_csv=$nativeCounters})
            $nativeNextGpuMs = $nativeWatch.Elapsed.TotalMilliseconds + 200.0
        }
        if ($nativeWatch.Elapsed.TotalSeconds -gt 170) { $nativeProcess.Kill($true); throw 'Disposable Studio smoke timed out' }
    }
    $nativeProcess.WaitForExit()
    $nativeWatch.Stop()
    $nativeReportPath = Join-Path $nativeDirectory 'studio-smoke.json'
    if (-not (Test-Path -LiteralPath $nativeReportPath)) { throw ('No native smoke report; inspect ' + $nativeDirectory) }
    $nativeReport = Get-Content -LiteralPath $nativeReportPath -Raw | ConvertFrom-Json
    $nativeMetrics = [ordered]@{
        process_exit_code=$nativeProcess.ExitCode;wall_ms=$nativeWatch.Elapsed.TotalMilliseconds
        samples=$nativeSamples;gpu_samples=$nativeGpuSamples
        limitations=@('Debug build; process CPU/RAM sampling every25ms may miss peaks.', 'NVIDIA counters represent the whole adapter including other applications, not Studio-exclusive VRAM or utilization.', 'Sampling overhead influences startup; UI update duration is CPU work, not display FPS.', 'Scripted UI commands do not replace manual mouse/keyboard/high-DPI/device-loss tests.')
    }
    $nativeMetrics | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath (Join-Path $nativeDirectory 'process-metrics.json') -Encoding utf8
    if ($nativeProcess.ExitCode -ne 0 -or -not $nativeReport.passed) { throw ('Native smoke failed; inspect ' + $nativeDirectory) }
    & (Join-Path $nativeRoot 'target/debug/lumeryn.exe') project check --project $nativeDemo.project --assets
    if ($LASTEXITCODE -ne 0) { throw 'Post-Studio project integrity failed' }
    $nativeProcess.Dispose()
    [ordered]@{passed=$true;project=$nativeDemo.project;report=$nativeReportPath;screenshot=(Join-Path $nativeDirectory 'studio-native.png');metrics=(Join-Path $nativeDirectory 'process-metrics.json')} | ConvertTo-Json
} finally { Pop-Location }
