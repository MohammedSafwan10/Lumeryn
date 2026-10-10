# Opens the real native window and modifies only freshly generated disposable projects.
param([switch]$SkipBuild, [switch]$Uhd, [switch]$Portrait, [switch]$History)
$ErrorActionPreference = 'Stop'
if ($Portrait -and -not $Uhd) { throw '-Portrait requires -Uhd in this acceptance script' }
$nativeRoot = Split-Path $PSScriptRoot -Parent
function Wait-LumerynBuildFiles([string[]]$Paths) {
    $buildWatch = [Diagnostics.Stopwatch]::StartNew()
    do {
        $buildLocked = [Collections.Generic.List[string]]::new()
        foreach ($buildPath in $Paths) {
            if (-not (Test-Path -LiteralPath $buildPath -PathType Leaf)) { continue }
            $buildHandle = $null
            try { $buildHandle = [IO.File]::Open($buildPath, [IO.FileMode]::Open, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None) }
            catch [IO.IOException] { $buildLocked.Add($buildPath) }
            finally { if ($buildHandle) { $buildHandle.Dispose() } }
        }
        if ($buildLocked.Count -eq 0) { return }
        if ($buildWatch.Elapsed.TotalSeconds -ge 10) {
            throw ('Build executable is still in use after a bounded wait: ' + ($buildLocked -join ', ') + '. Close your own Studio/service session or use -SkipBuild with the current tested binaries.')
        }
        Start-Sleep -Milliseconds 100
    } while ($true)
}
function Write-LumerynJsonArtifact([string]$Path, [object]$Value, [int]$Depth = 20) {
    $artifactBytes = [Text.UTF8Encoding]::new($false).GetBytes((ConvertTo-Json -InputObject $Value -Depth $Depth))
    $artifactFile = [IO.File]::Open($Path, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    try { $artifactFile.Write($artifactBytes, 0, $artifactBytes.Length); $artifactFile.Flush($true) }
    finally { $artifactFile.Dispose() }
}
Push-Location $nativeRoot
$nativeProcess = $null
try {
    $nativeCli = Join-Path $nativeRoot 'target/debug/lumeryn.exe'
    $nativeStudio = Join-Path $nativeRoot 'target/debug/lumeryn-studio.exe'
    if (-not $SkipBuild) {
        Wait-LumerynBuildFiles -Paths @($nativeCli, $nativeStudio)
        cargo build --workspace --locked -j 4
        if ($LASTEXITCODE -ne 0) { throw 'Native workspace build failed' }
    }
    if (-not (Test-Path -LiteralPath $nativeCli -PathType Leaf) -or -not (Test-Path -LiteralPath $nativeStudio -PathType Leaf)) {
        throw 'CLI/Studio binaries are missing; build the current source before using -SkipBuild'
    }
    $nativeDemo = (& (Join-Path $PSScriptRoot 'demo.ps1') -Motion -SkipBuild | Out-String) | ConvertFrom-Json
    $nativeDirectory = Join-Path $nativeRoot ('.lumeryn-scratch/studio-' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $nativeDirectory | Out-Null
    if ($Portrait) {
        $portraitBefore = (& $nativeCli project inspect --project $nativeDemo.project | ConvertFrom-Json).result
        if ($LASTEXITCODE -ne 0) { throw 'Portrait fixture inspection failed' }
        $portraitEdit = Join-Path $nativeDirectory 'portrait-edit.json'
        Write-LumerynJsonArtifact -Path $portraitEdit -Value @{operations=@(@{operation='set_canvas';width=720;height=1280})}
        $portraitReceipt = & $nativeCli project batch --project $nativeDemo.project --file $portraitEdit --expected-revision $portraitBefore.revision
        if ($LASTEXITCODE -ne 0) { throw 'Portrait fixture canvas edit failed' }
        Write-LumerynJsonArtifact -Path (Join-Path $nativeDirectory 'portrait-receipt.json') -Value ($portraitReceipt | ConvertFrom-Json)
    }
    $nativeArguments = @('--smoke','--project',('"' + $nativeDemo.project + '"'),'--artifact-dir',('"' + $nativeDirectory + '"'))
    if ($Uhd) { $nativeArguments += '--smoke-uhd' }
    if ($History) { $nativeArguments += '--smoke-history' }
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
        if ($nativeWatch.Elapsed.TotalSeconds -gt 170) { $nativeProcess.Kill(); $nativeProcess.WaitForExit(); throw 'Disposable Studio smoke timed out' }
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
    $nativeMetrics['build_skipped'] = [bool]$SkipBuild
    $nativeMetrics['binaries'] = @{cli_sha256=(Get-FileHash -LiteralPath $nativeCli -Algorithm SHA256).Hash;studio_sha256=(Get-FileHash -LiteralPath $nativeStudio -Algorithm SHA256).Hash}
    Write-LumerynJsonArtifact -Path (Join-Path $nativeDirectory 'process-metrics.json') -Value $nativeMetrics -Depth 10
    if ($nativeProcess.ExitCode -ne 0 -or -not $nativeReport.passed) { throw ('Native smoke failed; inspect ' + $nativeDirectory) }
    if ($History -and (-not $nativeReport.history_clipboard_acceptance -or @($nativeReport.checks | Where-Object check -eq 'native_shared_history_step').Count -ne 12)) { throw 'Twelve native history/clipboard/media-name transitions were not verified' }
    if ($Uhd) {
        $expectedWidth = if ($Portrait) { 2160 } else { 3840 }
        $expectedHeight = if ($Portrait) { 3840 } else { 2160 }
        $rendered = $nativeReport.render_result
        if ($rendered.width -ne $expectedWidth -or $rendered.height -ne $expectedHeight -or $rendered.frame_rate -ne '60/1' -or -not $rendered.verified_full_decode -or $rendered.options.codec -ne 'h264') { throw 'Studio UHD settings/result were not verified' }
        $nativeProbe = if ($env:LUMERYN_FFPROBE) { $env:LUMERYN_FFPROBE } else { 'ffprobe' }
        $probeJson = & $nativeProbe -v error -show_streams -show_format -of json $rendered.path
        if ($LASTEXITCODE -ne 0) { throw 'Native UHD independent FFprobe failed' }
        $probe = $probeJson | ConvertFrom-Json
        $video = @($probe.streams | Where-Object codec_type -eq 'video')[0]
        $audio = @($probe.streams | Where-Object codec_type -eq 'audio')[0]
        if ($video.width -ne $expectedWidth -or $video.height -ne $expectedHeight -or $video.r_frame_rate -ne '60/1' -or [long]$video.nb_frames -ne [long]$rendered.frames -or $video.codec_name -ne 'h264') { throw 'Actual native UHD file video parameters mismatch' }
        if ($audio.sample_rate -ne '48000' -or $audio.channels -ne 2 -or $audio.codec_name -ne 'aac' -or [Math]::Abs([double]$audio.duration - [double]$rendered.duration_seconds) -gt 0.05) { throw 'Actual native UHD file audio/duration mismatch' }
        Write-LumerynJsonArtifact -Path (Join-Path $nativeDirectory 'uhd-file-verified.json') -Value @{passed=$true;portrait=[bool]$Portrait;probe=$probe;render_result=$rendered;limits='720p synthetic source is upscaled; source-detail and marker-sync acceptance belongs to uhd_export tests. Physical speaker/display synchronization is untested.'}
    }
    & (Join-Path $nativeRoot 'target/debug/lumeryn.exe') project check --project $nativeDemo.project --assets
    if ($LASTEXITCODE -ne 0) { throw 'Post-Studio project integrity failed' }
    [ordered]@{passed=$true;project=$nativeDemo.project;report=$nativeReportPath;screenshot=(Join-Path $nativeDirectory 'studio-native.png');metrics=(Join-Path $nativeDirectory 'process-metrics.json')} | ConvertTo-Json
} finally {
    if ($nativeProcess) {
        if (-not $nativeProcess.HasExited) { $nativeProcess.Kill(); $nativeProcess.WaitForExit() }
        $nativeProcess.Dispose()
    }
    Pop-Location
}
