# Capture the real redesigned native Studio; every edit targets a fresh project backup.
[CmdletBinding()]
param(
    [switch]$SkipBuild,
    [ValidateRange(980, 2560)][int]$LargeWidth = 1440,
    [ValidateRange(640, 1600)][int]$LargeHeight = 900,
    [ValidateRange(980, 1920)][int]$MediumWidth = 1280,
    [ValidateRange(640, 1200)][int]$MediumHeight = 800,
    [ValidateRange(980, 1600)][int]$CompactWidth = 980,
    [ValidateRange(640, 1000)][int]$CompactHeight = 640
)
$ErrorActionPreference = 'Stop'
$redesignRoot = Split-Path $PSScriptRoot -Parent
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
Push-Location $redesignRoot
try {
    $redesignCli = Join-Path $redesignRoot 'target/debug/lumeryn.exe'
    $redesignStudio = Join-Path $redesignRoot 'target/debug/lumeryn-studio.exe'
    if (-not $SkipBuild) {
        Wait-LumerynBuildFiles -Paths @($redesignCli, $redesignStudio)
        cargo build -p lumeryn-cli -p lumeryn-studio --locked -j 4
        if ($LASTEXITCODE -ne 0) { throw 'Redesign native build failed' }
    }
    if (-not (Test-Path -LiteralPath $redesignCli) -or -not (Test-Path -LiteralPath $redesignStudio)) {
        throw 'Build the CLI and Studio before using -SkipBuild'
    }
    $redesignHelp = (& $redesignStudio --help | Out-String)
    if ($LASTEXITCODE -ne 0 -or $redesignHelp -notmatch '--width' -or $redesignHelp -notmatch '--height') {
        throw 'Studio must support --width/--height logical viewport options; rebuild the updated application'
    }
    $redesignDirectory = Join-Path $redesignRoot ('.lumeryn-scratch/redesign-' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $redesignDirectory | Out-Null
    $redesignDemo = (& (Join-Path $PSScriptRoot 'demo.ps1') -Motion -SkipBuild | Out-String) | ConvertFrom-Json
    $redesignResults = [Collections.Generic.List[object]]::new()
    $redesignCases = @(
        [ordered]@{name='large';width=$LargeWidth;height=$LargeHeight},
        [ordered]@{name='medium';width=$MediumWidth;height=$MediumHeight},
        [ordered]@{name='compact';width=$CompactWidth;height=$CompactHeight}
    )
    foreach ($redesignCase in $redesignCases) {
        $redesignCaseDirectory = Join-Path $redesignDirectory $redesignCase.name
        New-Item -ItemType Directory -Path $redesignCaseDirectory | Out-Null
        $redesignProject = Join-Path $redesignCaseDirectory 'disposable.lumeryn'
        $redesignBackup = & $redesignCli project backup --project $redesignDemo.project --output $redesignProject
        if ($LASTEXITCODE -ne 0) { throw 'Create consistent disposable project backup failed' }
        $null = $redesignBackup | ConvertFrom-Json
        $redesignArguments = @(
            '--smoke','--project',('"' + $redesignProject + '"'),
            '--artifact-dir',('"' + $redesignCaseDirectory + '"'),
            '--width',[string]$redesignCase.width,'--height',[string]$redesignCase.height
        )
        $redesignProcess = $null
        $redesignWatch = [Diagnostics.Stopwatch]::StartNew()
        try {
            # A visible native window is required for this screenshot verification flow.
            $redesignProcess = Start-Process -FilePath $redesignStudio -ArgumentList $redesignArguments -WindowStyle Normal -PassThru -RedirectStandardOutput (Join-Path $redesignCaseDirectory 'stdout.log') -RedirectStandardError (Join-Path $redesignCaseDirectory 'stderr.log')
            while (-not $redesignProcess.WaitForExit(50)) {
                if ($redesignWatch.Elapsed.TotalSeconds -gt 170) {
                    $redesignProcess.Kill()
                    $redesignProcess.WaitForExit()
                    throw ('Redesign smoke timed out; inspect ' + $redesignCaseDirectory)
                }
            }
            $redesignProcess.WaitForExit()
            $redesignReportPath = Join-Path $redesignCaseDirectory 'studio-smoke.json'
            $redesignScreenshot = Join-Path $redesignCaseDirectory 'studio-native.png'
            if (-not (Test-Path -LiteralPath $redesignReportPath) -or -not (Test-Path -LiteralPath $redesignScreenshot)) {
                throw ('Missing native report/screenshot; inspect ' + $redesignCaseDirectory)
            }
            $redesignReport = Get-Content -LiteralPath $redesignReportPath -Raw | ConvertFrom-Json
            if ($redesignProcess.ExitCode -ne 0 -or -not $redesignReport.passed -or -not $redesignReport.screenshot_saved) {
                throw ('Native workflow smoke failed; inspect ' + $redesignCaseDirectory)
            }
            if (-not $redesignReport.viewport -or $redesignReport.viewport.logical_width -le 0 -or $redesignReport.viewport.logical_height -le 0 -or $redesignReport.viewport.pixels_per_point -le 0) {
                throw 'Native report must record positive actual logical viewport size and pixels_per_point'
            }
            $redesignRequired = @('existing_project_loaded','native_gpu_frame','playback_advanced','pause_and_exact_seek','frame_step','studio_trim_committed','cli_edit_live_event','background_export','native_window_screenshot')
            foreach ($redesignCheck in $redesignRequired) {
                if (-not ($redesignReport.checks | Where-Object { $_.check -eq $redesignCheck -and $_.passed })) {
                    throw ('Required workflow evidence missing: ' + $redesignCheck)
                }
            }
            $redesignExportCheck = $redesignReport.checks | Where-Object check -eq 'background_export'
            if ($redesignExportCheck.evidence.ui_updates_while_export -lt 1) { throw 'No native UI progress during export' }
            # PNG IHDR reports actual physical capture dimensions without requiring image tooling.
            $redesignBytes = [IO.File]::ReadAllBytes($redesignScreenshot)
            if ($redesignBytes.Length -lt 24 -or [Convert]::ToHexString($redesignBytes[0..7]) -ne '89504E470D0A1A0A') { throw 'Invalid PNG capture' }
            $redesignPixelWidth = ([uint32]$redesignBytes[16] -shl 24) -bor ([uint32]$redesignBytes[17] -shl 16) -bor ([uint32]$redesignBytes[18] -shl 8) -bor [uint32]$redesignBytes[19]
            $redesignPixelHeight = ([uint32]$redesignBytes[20] -shl 24) -bor ([uint32]$redesignBytes[21] -shl 16) -bor ([uint32]$redesignBytes[22] -shl 8) -bor [uint32]$redesignBytes[23]
            if ($redesignPixelWidth -eq 0 -or $redesignPixelHeight -eq 0) { throw 'Empty PNG dimensions' }
            $redesignIntegrity = & $redesignCli project check --project $redesignProject --assets
            if ($LASTEXITCODE -ne 0) { throw 'Post-smoke project/source integrity failed' }
            $redesignResults.Add([ordered]@{
                name=$redesignCase.name;workflow_passed=$true;visual_review='pending'
                requested_logical_width=$redesignCase.width;requested_logical_height=$redesignCase.height
                screenshot_pixel_width=$redesignPixelWidth;screenshot_pixel_height=$redesignPixelHeight
                viewport=$redesignReport.viewport;report=$redesignReportPath;screenshot=$redesignScreenshot
                screenshot_assets=$redesignReport.screenshot_assets;asset_capture_wait_expired=$redesignReport.asset_capture_wait_expired
                requested_bounds_matched=([Math]::Abs($redesignReport.viewport.logical_width - $redesignCase.width) -le 2 -and [Math]::Abs($redesignReport.viewport.logical_height - $redesignCase.height) -le 2)
                screenshot_sha256=(Get-FileHash -LiteralPath $redesignScreenshot -Algorithm SHA256).Hash
                project=$redesignProject;project_integrity=($redesignIntegrity | ConvertFrom-Json).result
            })
        } finally {
            $redesignWatch.Stop()
            if ($redesignProcess) {
                if (-not $redesignProcess.HasExited) { $redesignProcess.Kill(); $redesignProcess.WaitForExit() }
                $redesignProcess.Dispose()
            }
        }
    }
    $redesignSummary = [ordered]@{
        schema_version=1;workflow_passed=$true;visual_review='pending';approved_reference='docs/design/studio-redesign-v1/B-studio.png'
        cases=$redesignResults
        build_skipped=[bool]$SkipBuild
        binaries=@{cli_sha256=(Get-FileHash -LiteralPath $redesignCli -Algorithm SHA256).Hash;studio_sha256=(Get-FileHash -LiteralPath $redesignStudio -Algorithm SHA256).Hash}
        limitations=@('Actual native screenshots require visual comparison with approved B before design acceptance.', 'Window manager/display/DPI may constrain requested logical size; inspect actual report viewport and PNG dimensions.', 'Scripted commands preserve workflow but do not prove pointer usability, file-dialog cancellation or high-DPI keyboard/focus behavior.', 'Fixture is generated 720p footage. Thumbnail/waveform readiness is reported; effects and multi-track editing remain staged.', 'SkipBuild uses the existing binaries; it does not verify that they match current source or passed the full gate.')
    }
    $redesignSummaryPath = Join-Path $redesignDirectory 'redesign-smoke.json'
    Write-LumerynJsonArtifact -Path $redesignSummaryPath -Value $redesignSummary
    $redesignSummary | ConvertTo-Json -Depth 20
} finally { Pop-Location }
