# Starts an already-built GUI. No downloads, builds, migrations or automatic project selection.
[CmdletBinding()]
param(
    [string]$StudioExecutable,
    [string]$Project,
    [switch]$CheckOnly
)
$ErrorActionPreference = 'Stop'
$launchRoot = $PSScriptRoot
try {
    if (-not $StudioExecutable) { $StudioExecutable = $env:LUMERYN_STUDIO_PATH }
    if (-not $StudioExecutable) {
        $launchCandidates = @(
            (Join-Path $launchRoot 'target/release/lumeryn-studio.exe'),
            (Join-Path $launchRoot 'target/debug/lumeryn-studio.exe')
        )
        $StudioExecutable = $launchCandidates | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } | Select-Object -First 1
    }
    if (-not $StudioExecutable -or -not (Test-Path -LiteralPath $StudioExecutable -PathType Leaf)) {
        throw 'Studio is not built. In the Lumeryn directory run: cargo build --workspace --locked. For an isolated tested build set LUMERYN_STUDIO_PATH to its full lumeryn-studio.exe path.'
    }
    $launchExe = (Resolve-Path -LiteralPath $StudioExecutable).ProviderPath
    if ([IO.Path]::GetFileName($launchExe) -ne 'lumeryn-studio.exe') { throw 'Select the native lumeryn-studio.exe binary.' }
    $launchSibling = Join-Path (Split-Path $launchExe -Parent) 'lumeryn.exe'
    if (-not (Test-Path -LiteralPath $launchSibling -PathType Leaf)) {
        throw 'The matching lumeryn.exe project service is missing beside Studio. Build the workspace together with cargo build --workspace --locked.'
    }
    # Verify the actual linked PE subsystem, rather than trusting a stale source/build filename.
    $launchStream = [IO.File]::OpenRead($launchExe)
    $launchReader = [IO.BinaryReader]::new($launchStream)
    try {
        if ($launchReader.ReadUInt16() -ne 0x5a4d) { throw 'Studio is not a Windows executable.' }
        $launchStream.Position = 0x3c
        $launchPeOffset = $launchReader.ReadUInt32()
        if ($launchPeOffset -gt ($launchStream.Length - 94)) { throw 'Studio has an invalid executable header.' }
        $launchStream.Position = $launchPeOffset
        if ($launchReader.ReadUInt32() -ne 0x4550) { throw 'Studio has an invalid PE signature.' }
        $launchStream.Position = $launchPeOffset + 24 + 68
        if ($launchReader.ReadUInt16() -ne 2) {
            throw 'This Studio executable is an old console build. Rebuild the current source with cargo build --workspace --locked, or select a current GUI build with LUMERYN_STUDIO_PATH.'
        }
    }
    finally { $launchReader.Dispose(); $launchStream.Dispose() }
    $launchArgs = @()
    if ($Project) {
        if (-not (Test-Path -LiteralPath $Project -PathType Leaf)) { throw 'The selected project file does not exist. Omit -Project to use the Welcome screen.' }
        $launchProject = (Resolve-Path -LiteralPath $Project).ProviderPath
        if ([IO.Path]::GetExtension($launchProject) -ne '.lumeryn') { throw 'Select a .lumeryn project file.' }
        $launchArgs = @('--project', $launchProject)
    }
    if ($CheckOnly) {
        [pscustomobject]@{ executable = $launchExe; service = $launchSibling; arguments = $launchArgs; subsystem = 'windows-gui'; creates_project = $false } | ConvertTo-Json -Depth 3
        return
    }
    $launchLogRoot = Join-Path $env:LOCALAPPDATA 'Lumeryn/logs'
    New-Item -ItemType Directory -Path $launchLogRoot -Force | Out-Null
    $launchLogId = [guid]::NewGuid().ToString('N')
    $launchOut = Join-Path $launchLogRoot ($launchLogId + '.stdout.log')
    $launchError = Join-Path $launchLogRoot ($launchLogId + '.stderr.log')
    $launchInfo = [Diagnostics.ProcessStartInfo]::new()
    $launchInfo.FileName = $launchExe
    $launchInfo.WorkingDirectory = $launchRoot
    $launchInfo.UseShellExecute = $false
    $launchInfo.RedirectStandardOutput = $true
    $launchInfo.RedirectStandardError = $true
    if ($launchInfo.PSObject.Properties.Name -contains 'ArgumentList') {
        foreach ($launchArg in $launchArgs) { $launchInfo.ArgumentList.Add($launchArg) }
    }
    elseif ($launchArgs.Count) {
        # Windows PowerShell 5.1 has no ArgumentList. Only a resolved existing file is accepted;
        # Windows filenames cannot contain quotes, and this path cannot end in a backslash.
        $launchInfo.Arguments = '--project "' + $launchProject + '"'
    }
    $launchProcess = [Diagnostics.Process]::new()
    $launchProcess.StartInfo = $launchInfo
    try {
        if (-not $launchProcess.Start()) { throw 'Windows did not start Lumeryn Studio.' }
        $launchOutStream = $null
        $launchErrStream = $null
        try {
            $launchOutStream = [IO.File]::Open($launchOut, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::Read)
            $launchErrStream = [IO.File]::Open($launchError, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::Read)
            $launchOutTask = $launchProcess.StandardOutput.BaseStream.CopyToAsync($launchOutStream)
            $launchErrTask = $launchProcess.StandardError.BaseStream.CopyToAsync($launchErrStream)
            $launchProcess.WaitForExit()
            $launchOutTask.GetAwaiter().GetResult()
            $launchErrTask.GetAwaiter().GetResult()
        }
        finally {
            if ($launchOutStream) { $launchOutStream.Dispose() }
            if ($launchErrStream) { $launchErrStream.Dispose() }
        }
        if ($launchProcess.ExitCode -ne 0) { throw "Studio exited with code $($launchProcess.ExitCode). Diagnostic log: $launchError" }
    }
    finally { $launchProcess.Dispose() }
}
catch {
    if ($CheckOnly) { throw }
    Add-Type -AssemblyName System.Windows.Forms
    [Windows.Forms.MessageBox]::Show($_.Exception.Message, 'Lumeryn Studio launch failed', [Windows.Forms.MessageBoxButtons]::OK, [Windows.Forms.MessageBoxIcon]::Error) | Out-Null
    exit 1
}
