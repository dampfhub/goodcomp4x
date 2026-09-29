# Runs the interaction test: `hex.exe --interact` opens the real window, which never takes the
# focus, and plays a script of clicks and keys through its own message queue (src/interact.tua).
# The exe runs without a console window and its output is echoed. The exit code is the game's:
# 0 passed; 100 + N step N's check failed; 94 the frame loop froze; 95 the script overran;
# 96 no frame was ever drawn; 200 the harness's own watchdog killed it.
#
#   powershell -File tools\interact.ps1 [-Exe hex.exe] [-TimeoutSec 150] [-Extra "--scenario cities"]
param(
    [string]$Exe = (Join-Path $PSScriptRoot '..\hex.exe'),
    [int]$TimeoutSec = 150,
    [string]$Extra = ''
)
$Exe = (Resolve-Path $Exe).Path
$psi = New-Object System.Diagnostics.ProcessStartInfo
$psi.FileName = $Exe
$psi.Arguments = ('--interact ' + $Extra).Trim()
$psi.WorkingDirectory = Split-Path $Exe
$psi.UseShellExecute = $false
$psi.CreateNoWindow = $true
$psi.RedirectStandardOutput = $true
$psi.RedirectStandardError = $true
$p = [System.Diagnostics.Process]::Start($psi)
$out = $p.StandardOutput.ReadToEndAsync()
$err = $p.StandardError.ReadToEndAsync()
$watch = [System.Diagnostics.Stopwatch]::StartNew()
if (-not $p.WaitForExit($TimeoutSec * 1000)) {
    try { $p.Kill() } catch {}
    $p.WaitForExit()
    Write-Output ("TIMEOUT after {0}s" -f $TimeoutSec)
    exit 200
}
$p.WaitForExit()
Write-Output $out.Result
Write-Output $err.Result
Write-Output ("exit {0} in {1:N1}s" -f $p.ExitCode, $watch.Elapsed.TotalSeconds)
exit $p.ExitCode
