# Generated local feasibility fixture only; does not change or certify the Lumeryn engine.
$ErrorActionPreference = 'Stop'
$spikeRoot = Join-Path (Split-Path $PSScriptRoot -Parent) ('.lumeryn-scratch/composition-spike-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $spikeRoot | Out-Null
function Invoke-CheckedFfmpeg([string[]]$FfmpegArguments) {
    $timer = [Diagnostics.Stopwatch]::StartNew()
    & ffmpeg @FfmpegArguments 2> (Join-Path $spikeRoot 'last-ffmpeg.log')
    if ($LASTEXITCODE -ne 0) { throw (Get-Content (Join-Path $spikeRoot 'last-ffmpeg.log') -Raw) }
    $timer.Stop()
    return $timer.Elapsed.TotalMilliseconds
}
$source = Join-Path $spikeRoot 'source.mkv'
$still = Join-Path $spikeRoot 'still.png'
$output = Join-Path $spikeRoot 'composition.mkv'
$common = @('-hide_banner','-v','error','-nostdin','-threads','2','-filter_complex_threads','2')
$null = Invoke-CheckedFfmpeg ($common + @('-f','lavfi','-i','testsrc2=size=1280x720:rate=30:duration=2','-frames:v','60','-c:v','ffv1','-threads','2','-y',$source))
$null = Invoke-CheckedFfmpeg ($common + @('-f','lavfi','-i','color=c=0x67ffcd@0.65:s=320x180,format=rgba','-frames:v','1','-c:v','png','-threads','2','-y',$still))
$baseline = '[0:v]format=yuv420p[v]'
$composition = 'color=c=black:s=1280x720:r=30:d=2,format=rgba[canvas];[0:v]trim=end_frame=30,setpts=PTS-STARTPTS,format=rgba[video];[canvas][video]overlay=eof_action=pass:repeatlast=0:enable=lt(n\,30)[under];[1:v]crop=280:160:20:10,scale=280:160,format=rgba,rotate=PI/6:ow=rotw(PI/6):oh=roth(PI/6):c=none[graphic];[under][graphic]overlay=x=850:y=380:eof_action=pass:repeatlast=0:enable=gte(n\,10)*lt(n\,45),format=yuv420p[v]'
$oneTimes = @()
$layerTimes = @()
foreach ($run in 1..3) {
    $oneTimes += Invoke-CheckedFfmpeg ($common + @('-i',$source,'-filter_complex',$baseline,'-map','[v]','-frames:v','60','-an','-f','null','-'))
    $layerTimes += Invoke-CheckedFfmpeg ($common + @('-i',$source,'-loop','1','-framerate','30','-i',$still,'-filter_complex',$composition,'-map','[v]','-frames:v','60','-an','-f','null','-'))
}
$null = Invoke-CheckedFfmpeg ($common + @('-i',$source,'-loop','1','-framerate','30','-i',$still,'-filter_complex',$composition,'-map','[v]','-frames:v','60','-an','-c:v','ffv1','-threads','2','-y',$output))
$probe = (& ffprobe -v error -select_streams v:0 -count_frames -show_entries stream=width,height,r_frame_rate,nb_read_frames:format=duration -of json $output) | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) { throw 'FFprobe failed' }
if ($probe.streams[0].width -ne 1280 -or $probe.streams[0].height -ne 720 -or $probe.streams[0].r_frame_rate -ne '30/1' -or $probe.streams[0].nb_read_frames -ne '60' -or [math]::Abs([double]$probe.format.duration - 2.0) -gt 0.001) { throw 'Output metadata mismatch' }
$null = Invoke-CheckedFfmpeg ($common + @('-xerror','-i',$output,'-f','null','-'))
foreach ($frame in @(24,50)) {
    $null = Invoke-CheckedFfmpeg ($common + @('-i',$output,'-vf',"select=eq(n\,$frame)",'-frames:v','1','-c:v','png','-threads','2','-y',(Join-Path $spikeRoot "frame-$frame.png")))
}
$null = Invoke-CheckedFfmpeg (@('-hide_banner','-v','info','-nostdin','-i',$output,'-vf','blackdetect=d=0.1:pix_th=0.02:pic_th=0.99','-an','-f','null','-'))
$blackLines = @(Get-Content (Join-Path $spikeRoot 'last-ffmpeg.log') | Select-String 'black_start:' | ForEach-Object { $_.Line })
if (-not ($blackLines -match 'black_start:1.5')) { throw 'Final gap did not begin at expected frame45/30' }
$report = [ordered]@{ date='2026-10-10'; scope='External FFmpeg feasibility only; not Lumeryn compositor acceptance'; ffmpeg=(& ffmpeg -version 2>$null | Select-Object -First 1); fixture='synthetic1280x720/30,60frames; moving-video0..29; rotated alpha PNG10..44; final black gap45..59'; filter_threads=2; baseline_ms=$oneTimes; composition_ms=$layerTimes; baseline_median_ms=($oneTimes | Sort-Object)[1]; composition_median_ms=($layerTimes | Sort-Object)[1]; metadata=$probe; full_decode=$true; black_gap_lines=$blackLines; graph=$composition; hashes=@(Get-FileHash -Algorithm SHA256 $source,$still,$output | Select-Object Path,Hash) }
$report | ConvertTo-Json -Depth 8 | Set-Content (Join-Path $spikeRoot 'report.json') -Encoding utf8
[pscustomobject]$report | Select-Object date,baseline_median_ms,composition_median_ms,full_decode,black_gap_lines | ConvertTo-Json -Depth 3
Write-Output "Evidence: $spikeRoot"
