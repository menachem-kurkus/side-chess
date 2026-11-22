# Windows twin of check-memory.sh: launches Side Chess, makes Claude play a move,
# saves a screenshot, and reports memory for a minute.
# The figure is the process's private working set: Task Manager's "Memory" column.
# Fails if it reaches the limit. Usage: scripts\check-memory.ps1 [exe] [limit MB]
param(
  [string]$Exe = "$env:USERPROFILE\.side-chess\build\release\side-chess.exe",
  [int]$Limit = 100,
  [int]$Seconds = 60
)
$ErrorActionPreference = 'Stop'
if (-not (Test-Path $Exe)) { Write-Error "No exe at $Exe : run app\install.ps1 first"; exit 2 }
$home_ = Join-Path ([IO.Path]::GetTempPath()) ("side-chess-mem-" + [guid]::NewGuid())
$live = Join-Path $home_ '.side-chess'
New-Item -ItemType Directory -Force $live | Out-Null
$env:HOME = $home_; $env:USERPROFILE = $home_
$env:CHESS_FEN = 'rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1'
$p = Start-Process $Exe -PassThru
$worst = 0; $worstWs = 0
try {
  for ($i = 1; $i -le $Seconds / 5; $i++) {
    Start-Sleep 5
    if ($p.HasExited) { Write-Host 'Side Chess exited early'; exit 1 }
    if ($i -eq 2) { Set-Content (Join-Path $live 'claude_move.txt') 'c5' -NoNewline }
    if ($i -eq 3) { New-Item -ItemType File (Join-Path $live 'shot') | Out-Null }
    $c = Get-CimInstance Win32_PerfFormattedData_PerfProc_Process -Filter "IDProcess=$($p.Id)"
    $mb = [int][math]::Round($c.WorkingSetPrivate / 1MB)
    $ws = [int][math]::Round($c.WorkingSet / 1MB)
    Write-Host "after $($i * 5)s: $mb MB private ($ws MB working set)"
    if ($mb -gt $worst) { $worst = $mb }
    if ($ws -gt $worstWs) { $worstWs = $ws }
  }
  if (-not (Select-String -Path (Join-Path $live 'state.txt') -Pattern '^moves: .*c5' -Quiet)) {
    Write-Host 'FAIL: the move file was not played'; exit 1
  }
  if (Test-Path (Join-Path $live 'screen.png')) {
    $b = [IO.File]::ReadAllBytes((Join-Path $live 'screen.png'))
    $w = (($b[16] * 16777216) + ($b[17] * 65536) + ($b[18] * 256) + $b[19])
    Write-Host "screen scale: $w px wide"
  }
  if ($worst -ge $Limit) { Write-Host "FAIL: peaked at $worst MB (limit $Limit MB)"; exit 1 }
  Write-Host "PASS: peaked at $worst MB (limit $Limit MB)"
} finally {
  if (-not $p.HasExited) { $p.Kill() }
  Remove-Item -Recurse -Force $home_ -ErrorAction SilentlyContinue
}
