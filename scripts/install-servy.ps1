# One-time registration of Marketplace Backend as a Servy-managed Windows service.
[CmdletBinding()]
param(
    [string]$InstallDir = "C:\Users\udv\Desktop\MPI",
    [string]$ServiceName = "backend",
    [string]$ServyCli = "servy-cli.exe"
)

$ErrorActionPreference = "Stop"
$InstallDir = [IO.Path]::GetFullPath($InstallDir)
$BackendPath = Join-Path $InstallDir "backend.exe"
$ConfigPath = Join-Path $InstallDir "config.toml"
$LogDir = Join-Path $InstallDir "logs"

if (-not (Get-Command $ServyCli -ErrorAction SilentlyContinue)) { throw "Servy CLI not found: $ServyCli" }
if (-not (Test-Path -LiteralPath $BackendPath -PathType Leaf)) { throw "backend.exe not found: $BackendPath" }
if (-not (Test-Path -LiteralPath $ConfigPath -PathType Leaf)) { throw "Create and configure config.toml first: $ConfigPath" }
New-Item -ItemType Directory -Force -Path $LogDir | Out-Null

& $ServyCli install `
    "--name=$ServiceName" `
    "--displayName=Marketplace Backend" `
    "--description=Marketplace Integrator backend" `
    "--path=$BackendPath" `
    "--startupDir=$InstallDir" `
    "--startupType=AutomaticDelayedStart" `
    "--priority=Normal" `
    "--stdout=$(Join-Path $LogDir 'stdout.log')" `
    "--stderr=$(Join-Path $LogDir 'stderr.log')" `
    --enableSizeRotation `
    --rotationSize=20 `
    --maxRotations=10 `
    --enableHealth `
    --heartbeatInterval=10 `
    --maxFailedChecks=3 `
    --recoveryAction=RestartProcess `
    --recoveryOnCleanExit `
    --maxRestartAttempts=5

if ($LASTEXITCODE -ne 0) { throw "Servy service installation failed (exit code $LASTEXITCODE)." }

& $ServyCli start "--name=$ServiceName" --quiet
if ($LASTEXITCODE -ne 0) { throw "Servy service start failed (exit code $LASTEXITCODE)." }
Write-Host "Servy service '$ServiceName' installed and started." -ForegroundColor Green
