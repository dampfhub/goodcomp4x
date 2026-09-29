# Runs the multiplayer interaction test: two `hex.exe --interact-mp` processes, one hosting on
# 127.0.0.1 and one joining it, each with a real window that never takes the focus, each playing a
# few turns of clicks and keys through its own message queue (src/interact.tua). Neither process
# opens a console window. The host's log names the port and join code; the guest is started with
# them. Each process logs a checksum of its game after every turn; this script passes only when
# both exited 0 and the two logged the same checksum for every turn.
#
# Exit codes: 0 both passed and agreed; 1 a process failed (its own code is printed);
# 2 the checksums differ; 3 the host never said where it listens; 200 the watchdog killed them.
#
#   powershell -File tools\interact-mp.ps1 [-Exe hex.exe] [-TimeoutSec 240]
param(
    [string]$Exe = (Join-Path $PSScriptRoot '..\hex.exe'),
    [int]$TimeoutSec = 240
)
$Exe = (Resolve-Path $Exe).Path

# A process with its output read a line at a time as it arrives, so a full pipe never stops it.
function Start-Hex([string]$Arguments) {
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = $Exe
    $psi.Arguments = $Arguments
    $psi.WorkingDirectory = Split-Path $Exe
    $psi.UseShellExecute = $false
    $psi.CreateNoWindow = $true
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $p = [System.Diagnostics.Process]::Start($psi)
    return @{ proc = $p; lines = (New-Object System.Collections.Generic.List[string]); pending = $p.StandardOutput.ReadLineAsync(); err = $p.StandardError.ReadToEndAsync() }
}

# Moves every line that has arrived into the list.
function Read-Lines($h) {
    while ($null -ne $h.pending -and $h.pending.IsCompleted) {
        $line = $h.pending.Result
        if ($null -eq $line) { $h.pending = $null; break }
        $h.lines.Add($line)
        $h.pending = $h.proc.StandardOutput.ReadLineAsync()
    }
}

$watch = [System.Diagnostics.Stopwatch]::StartNew()
$hostH = Start-Hex '--interact-mp --host --players 2 --port 0'
$guestH = $null
try {
    # The host says where it listens once its game exists.
    while ($null -eq $guestH -and $watch.Elapsed.TotalSeconds -lt $TimeoutSec -and -not $hostH.proc.HasExited) {
        Read-Lines $hostH
        foreach ($line in $hostH.lines) {
            if ($null -eq $guestH -and $line -match 'hosting on port (\d+) for more players; they join with code (\w+)') {
                $guestH = Start-Hex "--interact-mp --join 127.0.0.1:$($Matches[1]) --code $($Matches[2])"
            }
        }
        Start-Sleep -Milliseconds 50
    }
    if ($null -eq $guestH) {
        Write-Output 'the host never said where it listens'
        Read-Lines $hostH
        Write-Output ($hostH.lines -join "`n")
        exit 3
    }
    while (($watch.Elapsed.TotalSeconds -lt $TimeoutSec) -and (-not $hostH.proc.HasExited -or -not $guestH.proc.HasExited)) {
        Read-Lines $hostH; Read-Lines $guestH
        Start-Sleep -Milliseconds 50
    }
    if (-not $hostH.proc.HasExited -or -not $guestH.proc.HasExited) {
        Write-Output ('TIMEOUT after {0}s' -f $TimeoutSec)
        exit 200
    }
    $hostH.proc.WaitForExit(); $guestH.proc.WaitForExit()
    # What is left in the pipes.
    foreach ($h in @($hostH, $guestH)) {
        while ($null -ne $h.pending) {
            $line = $h.pending.Result
            if ($null -eq $line) { $h.pending = $null; break }
            $h.lines.Add($line)
            $h.pending = $h.proc.StandardOutput.ReadLineAsync()
        }
        foreach ($line in ($h.err.Result -split "`r?`n")) { if ($line -ne '') { $h.lines.Add($line) } }
    }
    $interesting = { param($lines) $lines | Where-Object { $_ -match 'interaction|hosting|joined|ERROR|WARN|error|state:' } }
    Write-Output '--- host'; Write-Output ((& $interesting $hostH.lines) -join "`n")
    Write-Output '--- guest'; Write-Output ((& $interesting $guestH.lines) -join "`n")
    $sums = { param($lines) $lines | Where-Object { $_ -match 'interaction checksum after turn' } | ForEach-Object { ($_ -replace '^.*interaction checksum', 'interaction checksum').Trim() } }
    $h = @(& $sums $hostH.lines); $g = @(& $sums $guestH.lines)
    Write-Output ("host exit {0}, guest exit {1}, {2} and {3} checksums, {4:N1}s" -f $hostH.proc.ExitCode, $guestH.proc.ExitCode, $h.Count, $g.Count, $watch.Elapsed.TotalSeconds)
    if ($hostH.proc.ExitCode -ne 0 -or $guestH.proc.ExitCode -ne 0) { exit 1 }
    if ($h.Count -eq 0 -or ($h -join "`n") -ne ($g -join "`n")) { Write-Output 'the checksums differ'; exit 2 }
    Write-Output 'both machines played the same game'
    exit 0
} finally {
    foreach ($h in @($hostH, $guestH)) { if ($null -ne $h -and -not $h.proc.HasExited) { try { $h.proc.Kill() } catch {} } }
}
