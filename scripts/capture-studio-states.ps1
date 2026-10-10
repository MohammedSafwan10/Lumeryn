# Capture related B-native states without modifying their project.
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Project,
    [switch]$SkipBuild,
    [ValidateRange(0, [long]::MaxValue)][long]$CaptureFrame = 720
)
$ErrorActionPreference = 'Stop'
$captureRoot = Split-Path $PSScriptRoot -Parent
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
Push-Location $captureRoot
try {
    $captureCli = Join-Path $captureRoot 'target/debug/lumeryn.exe'
    $captureStudio = Join-Path $captureRoot 'target/debug/lumeryn-studio.exe'
    if (-not $SkipBuild) {
        Wait-LumerynBuildFiles -Paths @($captureCli, $captureStudio)
        cargo build -p lumeryn-cli -p lumeryn-studio --locked -j 4
        if ($LASTEXITCODE -ne 0) { throw 'Capture CLI/Studio build failed' }
    }
    if (-not (Test-Path -LiteralPath $captureCli -PathType Leaf) -or -not (Test-Path -LiteralPath $captureStudio -PathType Leaf)) {
        throw 'CLI/Studio binaries are missing; build the current source before using -SkipBuild'
    }
    $captureHelp = & $captureStudio --help | Out-String
    if ($LASTEXITCODE -ne 0 -or $captureHelp -notmatch '--capture-only' -or $captureHelp -notmatch '--capture-state') {
        throw 'Studio must support capture-only states; rebuild the current application'
    }
    $captureProject = (Resolve-Path -LiteralPath $Project).Path
    function Invoke-CaptureCli([string[]]$Arguments) {
        $captureJson = & $captureCli @Arguments
        if ($LASTEXITCODE -ne 0) { throw ('Capture integrity/read failed: ' + ($Arguments -join ' ')) }
        ($captureJson | ConvertFrom-Json).result
    }
    $captureInitial = Invoke-CaptureCli -Arguments @('project','inspect','--project',$captureProject)
    $captureAssetsBefore = Invoke-CaptureCli -Arguments @('project','check','--project',$captureProject,'--assets')
    $captureDirectory = Join-Path $captureRoot ('.lumeryn-scratch/b-states-' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $captureDirectory | Out-Null
    Write-LumerynJsonArtifact -Path (Join-Path $captureDirectory 'before.json') -Value @{project=$captureInitial;integrity=$captureAssetsBefore}
    $captureCases = @(
        @{name='active';state='active';width=1440;height=900},
        @{name='compact';state='inspector';width=980;height=640},
        @{name='project';state='project';width=1440;height=900},
        @{name='export-compact';state='export';width=980;height=640},
        @{name='export';state='export';width=1440;height=900},
        @{name='tools';state='tools';width=1280;height=800},
        @{name='empty';state='empty';width=1280;height=800},
        @{name='library';state='library';width=1440;height=900},
        @{name='library-audio';state='library-audio';width=1440;height=900},
        @{name='collapsed';state='collapsed';width=980;height=640},
        @{name='fullscreen';state='fullscreen';width=1440;height=900}
    )
    $captureResults = [Collections.Generic.List[object]]::new()
    foreach ($captureCase in $captureCases) {
        $captureBefore = Invoke-CaptureCli -Arguments @('project','inspect','--project',$captureProject)
        if ($captureBefore.revision -ne $captureInitial.revision) { throw 'Project changed concurrently before capture; retry from a stable project revision' }
        $captureDestination = Join-Path $captureDirectory $captureCase.name
        New-Item -ItemType Directory -Path $captureDestination | Out-Null
        $captureArguments = @('--capture-only','--capture-state',$captureCase.state,'--capture-frame',[string]$CaptureFrame,'--artifact-dir',('"' + $captureDestination + '"'),'--width',[string]$captureCase.width,'--height',[string]$captureCase.height)
        if ($captureCase.state -ne 'empty') { $captureArguments += @('--project',('"' + $captureProject + '"')) }
        $captureProcess = $null
        try {
            # Real native pixels require a visible window during this explicit verification flow.
            $captureProcess = Start-Process -FilePath $captureStudio -ArgumentList $captureArguments -WindowStyle Normal -PassThru -RedirectStandardOutput (Join-Path $captureDestination 'stdout.log') -RedirectStandardError (Join-Path $captureDestination 'stderr.log')
            if (-not $captureProcess.WaitForExit(40000)) {
                $captureProcess.Kill()
                $captureProcess.WaitForExit()
                throw ('Disposable capture timed out: ' + $captureCase.name)
            }
            $captureProcess.WaitForExit()
            $captureReportPath = Join-Path $captureDestination 'studio-capture.json'
            $captureScreenshot = Join-Path $captureDestination 'screenshot-capture.png'
            if (-not (Test-Path -LiteralPath $captureReportPath -PathType Leaf) -or -not (Test-Path -LiteralPath $captureScreenshot -PathType Leaf)) {
                throw ('Native capture artifacts missing: ' + $captureDestination)
            }
            $captureReport = Get-Content -LiteralPath $captureReportPath -Raw | ConvertFrom-Json
            if ($captureProcess.ExitCode -ne 0 -or -not $captureReport.passed -or -not $captureReport.capture_only -or -not $captureReport.screenshot_saved) {
                throw ('Native capture failed: ' + $captureDestination)
            }
            if ($captureReport.viewport.logical_width -le 0 -or $captureReport.viewport.logical_height -le 0 -or $captureReport.viewport.pixels_per_point -le 0) {
                throw 'Capture must report positive native viewport/DPI evidence'
            }
            if ($captureCase.state -ne 'empty' -and ($captureReport.project_revision -ne $captureBefore.revision -or $captureReport.capture.project_revision -ne $captureBefore.revision)) {
                throw 'Captured scene revision differs from the inspected project revision'
            }
            if ($captureCase.state -eq 'empty' -and $null -ne $captureReport.project_revision) {
                throw 'Empty capture unexpectedly loaded a project'
            }
            if ($captureCase.state -eq 'fullscreen' -and $captureReport.capture.native_fullscreen -ne $true) { throw 'OS native fullscreen was not confirmed' }
            $captureBytes = [IO.File]::ReadAllBytes($captureScreenshot)
            if ($captureBytes.Length -lt 24 -or [Convert]::ToHexString($captureBytes[0..7]) -ne '89504E470D0A1A0A') { throw 'Invalid native PNG capture' }
            $capturePixelWidth = ([uint32]$captureBytes[16] -shl 24) -bor ([uint32]$captureBytes[17] -shl 16) -bor ([uint32]$captureBytes[18] -shl 8) -bor [uint32]$captureBytes[19]
            $capturePixelHeight = ([uint32]$captureBytes[20] -shl 24) -bor ([uint32]$captureBytes[21] -shl 16) -bor ([uint32]$captureBytes[22] -shl 8) -bor [uint32]$captureBytes[23]
            if ($capturePixelWidth -le 0 -or $capturePixelHeight -le 0 -or $capturePixelWidth -ne $captureReport.capture.physical_width -or $capturePixelHeight -ne $captureReport.capture.physical_height) {
                throw 'PNG dimensions do not match native capture evidence'
            }
            $captureAfter = Invoke-CaptureCli -Arguments @('project','inspect','--project',$captureProject)
            if ($captureAfter.revision -ne $captureBefore.revision) { throw 'Project revision changed during capture; capture-only acceptance rejected' }
            $captureResults.Add([ordered]@{
                name=$captureCase.name;state=$captureCase.state;capture_passed=$true;visual_review='pending'
                revision_before=$captureBefore.revision;revision_after=$captureAfter.revision;revision_unchanged=$true
                requested_frame=$CaptureFrame;requested_logical_width=$captureCase.width;requested_logical_height=$captureCase.height
                actual_viewport=$captureReport.viewport;physical_width=$capturePixelWidth;physical_height=$capturePixelHeight
                requested_bounds_matched=([Math]::Abs($captureReport.viewport.logical_width - $captureCase.width) -le 2 -and [Math]::Abs($captureReport.viewport.logical_height - $captureCase.height) -le 2)
                report_path=$captureReportPath;report=$captureReport;screenshot=$captureScreenshot
                screenshot_sha256=(Get-FileHash -LiteralPath $captureScreenshot -Algorithm SHA256).Hash
            })
        } finally {
            if ($captureProcess) {
                if (-not $captureProcess.HasExited) { $captureProcess.Kill(); $captureProcess.WaitForExit() }
                $captureProcess.Dispose()
            }
        }
    }
    $captureFinal = Invoke-CaptureCli -Arguments @('project','inspect','--project',$captureProject)
    $captureAssetsAfter = Invoke-CaptureCli -Arguments @('project','check','--project',$captureProject,'--assets')
    if ($captureFinal.revision -ne $captureInitial.revision) { throw 'Project changed during state capture; no nonmutation acceptance recorded' }
    Write-LumerynJsonArtifact -Path (Join-Path $captureDirectory 'after.json') -Value @{project=$captureFinal;integrity=$captureAssetsAfter}
    $captureSummary = [ordered]@{
        schema_version=1;capture_passed=$true;visual_review='pending';directory=$captureDirectory
        project=$captureProject;revision_before=$captureInitial.revision;revision_after=$captureFinal.revision;revision_unchanged=$true
        assets_before=$captureAssetsBefore;assets_after=$captureAssetsAfter;states=$captureResults
        build_skipped=[bool]$SkipBuild
        binaries=@{cli_sha256=(Get-FileHash -LiteralPath $captureCli -Algorithm SHA256).Hash;studio_sha256=(Get-FileHash -LiteralPath $captureStudio -Algorithm SHA256).Hash}
        limitations=@('Native captures require visual comparison with approved B; success does not prove pointer/dialog usability.', 'Capture-only submits no edits; unchanged revisions and source checks detect ordinary concurrent changes, not hostile change-and-restore races.', 'SkipBuild uses existing binaries and does not prove current source correspondence or full-gate verification.', 'DPI/window-manager constraints can change actual viewport bounds. Inspect recorded dimensions and assets instead of inferring requested-size acceptance.')
    }
    Write-LumerynJsonArtifact -Path (Join-Path $captureDirectory 'states.json') -Value $captureSummary
    $captureSummary | ConvertTo-Json -Depth 20
} finally { Pop-Location }
