# Generated/trusted media only. No downloads or source-media mutation.
param([switch]$Motion, [switch]$SkipBuild)
$ErrorActionPreference = 'Stop'
$lumerynRoot = Split-Path $PSScriptRoot -Parent
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
Push-Location $lumerynRoot
try {
    $lumerynCli = Join-Path $lumerynRoot 'target/debug/lumeryn.exe'
    if (-not $SkipBuild) {
        Wait-LumerynBuildFiles -Paths @($lumerynCli)
        cargo build -p lumeryn-cli --locked -j 4
        if ($LASTEXITCODE -ne 0) { throw 'Build failed' }
    }
    if (-not (Test-Path -LiteralPath $lumerynCli -PathType Leaf)) {
        throw 'CLI binary is missing; build the current source before using -SkipBuild'
    }
    $lumerynRun = Join-Path $lumerynRoot ('.lumeryn-scratch/demo-' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $lumerynRun | Out-Null
    $lumerynSource = Join-Path $lumerynRun 'generated.mkv'
    $lumerynProject = Join-Path $lumerynRun 'project.lumeryn'
    $lumerynOutput = Join-Path $lumerynRun 'trimmed.mp4'
    $lumerynSheet = Join-Path $lumerynRun 'contact-sheet.png'
    $lumerynFfmpeg = if ($env:LUMERYN_FFMPEG) { $env:LUMERYN_FFMPEG } else { 'ffmpeg' }
    if ($Motion) {
        & $lumerynFfmpeg -v error -nostdin -n -f lavfi -i 'testsrc2=size=1280x720:rate=30:duration=3' -f lavfi -i 'sine=frequency=440:sample_rate=48000:duration=3' -threads 2 -c:v ffv1 -c:a pcm_s16le $lumerynSource
        $lumerynWidth = '1280'; $lumerynHeight = '720'; $lumerynTrimFrames = '60'; $lumerynSheetRate = '3'
    } else {
        & $lumerynFfmpeg -v error -nostdin -n -f lavfi -i 'color=red:s=320x180:r=30:d=1' -f lavfi -i 'color=green:s=320x180:r=30:d=1' -f lavfi -i 'color=blue:s=320x180:r=30:d=1' -f lavfi -i 'sine=frequency=440:sample_rate=48000:duration=3' -filter_complex '[0:v][1:v][2:v]concat=n=3:v=1:a=0[v]' -map '[v]' -map '3:a' -c:v ffv1 -c:a pcm_s16le $lumerynSource
        $lumerynWidth = '320'; $lumerynHeight = '180'; $lumerynTrimFrames = '30'; $lumerynSheetRate = '6'
    }
    if ($LASTEXITCODE -ne 0) { throw 'Fixture generation failed' }
    function Invoke-Lumeryn([string[]]$Arguments) {
        $lumerynResponse = & $lumerynCli @Arguments
        if ($LASTEXITCODE -ne 0) { throw ('CLI failed: ' + ($Arguments -join ' ')) }
        ($lumerynResponse | ConvertFrom-Json).result
    }
    $lumerynCreated = Invoke-Lumeryn -Arguments @('project','create','--project',$lumerynProject,'--name','Synthetic acceptance','--width',$lumerynWidth,'--height',$lumerynHeight)
    $lumerynImported = Invoke-Lumeryn -Arguments @('media','import','--project',$lumerynProject,'--path',$lumerynSource,'--expected-revision','0')
    $lumerynAdded = Invoke-Lumeryn -Arguments @('timeline','clip','add','--project',$lumerynProject,'--media-id',$lumerynImported.object_ids[0],'--duration-frames','90','--expected-revision','1')
    $lumerynTrim = Invoke-Lumeryn -Arguments @('timeline','clip','trim','--project',$lumerynProject,'--clip-id',$lumerynAdded.object_ids[0],'--source-in-frame','15','--duration-frames',$lumerynTrimFrames,'--expected-revision','2')
    $lumerynWatch = [System.Diagnostics.Stopwatch]::StartNew()
    $lumerynRender = Invoke-Lumeryn -Arguments @('render','--project',$lumerynProject,'--output',$lumerynOutput,'--expected-revision','3')
    $lumerynWatch.Stop()
    $lumerynReopened = Invoke-Lumeryn -Arguments @('project','open','--project',$lumerynProject)
    $lumerynCheck = Invoke-Lumeryn -Arguments @('project','check','--project',$lumerynProject,'--assets')
    $lumerynHardware = Invoke-Lumeryn -Arguments @('hardware','scan')
    & $lumerynFfmpeg -v error -nostdin -n -i $lumerynOutput -vf ('fps=' + $lumerynSheetRate + ',scale=320:180,tile=3x2') -frames:v 1 $lumerynSheet
    if ($LASTEXITCODE -ne 0) { throw 'Contact sheet failed' }
    $lumerynSummary = [ordered]@{fixture=$(if ($Motion) { 'generated moving 720p testsrc2 and 440Hz sine' } else { 'generated color sequence and 440Hz sine' }); render=$lumerynRender; render_wall_ms=$lumerynWatch.Elapsed.TotalMilliseconds; reopened_revision=$lumerynReopened.revision; integrity=$lumerynCheck; hardware=$lumerynHardware; contact_sheet=$lumerynSheet; project=$lumerynProject}
    $lumerynSummary['build_skipped'] = [bool]$SkipBuild
    $lumerynSummary['cli_binary_sha256'] = (Get-FileHash -LiteralPath $lumerynCli -Algorithm SHA256).Hash
    Write-LumerynJsonArtifact -Path (Join-Path $lumerynRun 'summary.json') -Value $lumerynSummary -Depth 10
    $lumerynSummary | ConvertTo-Json -Depth 10
} finally { Pop-Location }
