# Builds Side Chess and installs it on Windows: the program goes in
# %LOCALAPPDATA%\Programs\Side Chess and a Start Menu shortcut is added.
# Needs Rust (https://rustup.rs) and the Visual Studio C++ Build Tools.
# Run:  powershell -ExecutionPolicy Bypass -File app\install.ps1
$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
  Write-Error 'Side Chess needs Rust to build. Install it from https://rustup.rs, then run this again.'
  exit 1
}
$env:CARGO_TARGET_DIR = Join-Path $env:USERPROFILE '.side-chess\build'
cargo build --release
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
$dir = Join-Path $env:LOCALAPPDATA 'Programs\Side Chess'
New-Item -ItemType Directory -Force $dir | Out-Null
$exe = Join-Path $dir 'side-chess.exe'
Copy-Item (Join-Path $env:CARGO_TARGET_DIR 'release\side-chess.exe') $exe -Force
$lnk = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\Side Chess.lnk'
$sh = (New-Object -ComObject WScript.Shell).CreateShortcut($lnk)
$sh.TargetPath = $exe
$sh.WorkingDirectory = $dir
$sh.Description = 'Play chess against your Claude Code session'
$sh.Save()
Write-Host "Installed: $exe"
Write-Host "Start Menu shortcut: $lnk"
