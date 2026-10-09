# Generated/trusted fixtures only; this probes installed tools, not Lumeryn backend support.
param([int]$Repetitions = 3)
$ErrorActionPreference = 'Stop'
if ($Repetitions -lt 1 -or $Repetitions -gt 10) { throw 'Repetitions must be 1..10' }
$benchmarkRoot = Split-Path $PSScriptRoot -Parent
$benchmarkDirectory = Join-Path $benchmarkRoot ('.lumeryn-scratch/hardware-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $benchmarkDirectory | Out-Null
$benchmarkFfmpeg = if ($env:LUMERYN_FFMPEG) { $env:LUMERYN_FFMPEG } else { (Get-Command ffmpeg).Source }
$benchmarkProbe = if ($env:LUMERYN_FFPROBE) { $env:LUMERYN_FFPROBE } else { (Get-Command ffprobe).Source }

function Invoke-CodecProbe([string]$Executable, [string[]]$Arguments) {
    $startInfo = [Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = $Executable
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    foreach ($argument in $Arguments) { $startInfo.ArgumentList.Add($argument) }
    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $startInfo
    $watch = [Diagnostics.Stopwatch]::StartNew()
    if (-not $process.Start()) { throw 'Worker launch failed' }
    $stdoutTask = $process.StandardOutput.ReadToEndAsync()
    $stderrTask = $process.StandardError.ReadToEndAsync()
    $peakRam = 0L
    while (-not $process.WaitForExit(25)) {
        try { $process.Refresh(); $peakRam = [Math]::Max($peakRam, $process.WorkingSet64) } catch { }
        if ($watch.Elapsed.TotalSeconds -gt 30) { $process.Kill($true); throw 'Generated hardware probe exceeded 30 seconds' }
    }
    $watch.Stop()
    $stderr = $stderrTask.GetAwaiter().GetResult()
    $stdout = $stdoutTask.GetAwaiter().GetResult()
    $result = [ordered]@{
        exit_code = $process.ExitCode; wall_ms = $watch.Elapsed.TotalMilliseconds
        sampled_peak_ram_bytes = $peakRam; sample_interval_ms = 25
        stdout = $stdout; stderr = $stderr.Substring(0, [Math]::Min(4096, $stderr.Length))
    }
    $process.Dispose()
    return $result
}

$runs = [Collections.Generic.List[object]]::new()
foreach ($encoder in @('mpeg4','h264_nvenc','h264_qsv','h264_amf')) {
    for ($iteration = 1; $iteration -le $Repetitions; $iteration++) {
        $output = Join-Path $benchmarkDirectory ($encoder + '-' + $iteration + '.mp4')
        $arguments = @('-hide_banner','-v','error','-nostdin','-n','-filter_threads','2','-f','lavfi','-i','testsrc2=size=1280x720:rate=30:duration=1','-an','-c:v',$encoder,'-threads','2','-b:v','4M','-pix_fmt','yuv420p',$output)
        $measurement = Invoke-CodecProbe -Executable $benchmarkFfmpeg -Arguments $arguments
        $verification = $null
        $metadata = $null
        if ($measurement.exit_code -eq 0) {
            $verification = Invoke-CodecProbe -Executable $benchmarkFfmpeg -Arguments @('-v','error','-nostdin','-xerror','-i',$output,'-f','null','-')
            $probe = Invoke-CodecProbe -Executable $benchmarkProbe -Arguments @('-v','error','-select_streams','v:0','-count_frames','-show_entries','stream=codec_name,width,height,avg_frame_rate,nb_read_frames','-of','json',$output)
            if ($probe.exit_code -eq 0) { $metadata = $probe.stdout | ConvertFrom-Json }
        }
        $runs.Add([ordered]@{ encoder=$encoder; iteration=$iteration; encode=$measurement; full_cpu_decode=$verification; metadata=$metadata })
        if ($measurement.exit_code -ne 0) { break }
    }
}
$nvencSource = Join-Path $benchmarkDirectory 'h264_nvenc-1.mp4'
$decodeRuns = [Collections.Generic.List[object]]::new()
if (($runs | Where-Object { $_.encoder -eq 'h264_nvenc' -and $_.iteration -eq 1 }).encode.exit_code -eq 0) {
    foreach ($decoder in @('cpu','cuda_download','cuda_resident_transcode')) {
        for ($iteration = 1; $iteration -le $Repetitions; $iteration++) {
            $arguments = @('-v','error','-nostdin','-threads','2')
            if ($decoder -ne 'cpu') { $arguments += @('-hwaccel','cuda','-hwaccel_output_format','cuda') }
            $arguments += @('-i',$nvencSource,'-an')
            if ($decoder -eq 'cuda_download') { $arguments += @('-vf','hwdownload,format=nv12') }
            if ($decoder -eq 'cuda_resident_transcode') { $arguments += @('-c:v','h264_nvenc','-b:v','4M') }
            $arguments += @('-f','null','-')
            $measurement = Invoke-CodecProbe -Executable $benchmarkFfmpeg -Arguments $arguments
            $decodeRuns.Add([ordered]@{mode=$decoder;iteration=$iteration;measurement=$measurement})
            if ($measurement.exit_code -ne 0) { break }
        }
    }
}
$report = [ordered]@{
    date='2026-10-09'; fixture='generated 1 second 1280x720 30fps testsrc2, 4Mbit/s target, 2 CPU threads'
    tools=@{ ffmpeg=$benchmarkFfmpeg; ffprobe=$benchmarkProbe }
    adapter=(Get-CimInstance Win32_VideoController | Select-Object Name,DriverVersion,Status)
    encode_runs=$runs; decode_runs=$decodeRuns
    vram_bytes=$null; gpu_utilization=$null
    limitations=@('Backend-only initialization measurements; no app integration or wgpu texture interop.', 'CPU MPEG4 and H264 hardware outputs differ in codec/quality; these are not equal-quality speed comparisons.', 'Small fixture includes process/device startup; no sustained throughput or percentile claim.', 'RAM sampled every25ms; true peaks and VRAM are unknown.', 'Installed FFmpeg is user-provided and not approved for redistribution.')
}
$reportPath = Join-Path $benchmarkDirectory 'hardware.json'
$report | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $reportPath -Encoding utf8
$reportPath
