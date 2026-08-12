# ==============================================================================
# build-release.ps1 - Build and package release for server deployment
#
# Creates deploy/ folder in project root with everything needed to copy.
# Run from any location: .\scripts\build-release.ps1
# ==============================================================================

$ProjectRoot = Split-Path $PSScriptRoot -Parent
$DeployDir   = "$ProjectRoot\deploy"

function Write-Step { param($msg) Write-Host "`n>>> $msg" -ForegroundColor Cyan }
function Write-Ok   { param($msg) Write-Host "  OK   $msg" -ForegroundColor Green }
function Write-Fail { param($msg) Write-Host "  FAIL $msg" -ForegroundColor Red }
function Write-Info { param($msg) Write-Host "       $msg" -ForegroundColor Gray }

Set-Location $ProjectRoot
Write-Host ""
Write-Host "============================================" -ForegroundColor Yellow
Write-Host "  MARKETPLACE - BUILD RELEASE"               -ForegroundColor Yellow
Write-Host "============================================" -ForegroundColor Yellow

# ------------------------------------------------------------------------------
# 1. Build backend
# ------------------------------------------------------------------------------
Write-Step "Building backend (cargo build --release)"
cargo build --release --bin backend
if ($LASTEXITCODE -ne 0) {
    Write-Fail "cargo build failed ($LASTEXITCODE)"
    exit 1
}
Write-Ok "backend.exe built"

# ------------------------------------------------------------------------------
# 2. Build frontend
# ------------------------------------------------------------------------------
Write-Step "Building frontend (trunk build --release)"
trunk build --release
if ($LASTEXITCODE -ne 0) {
    Write-Fail "trunk build failed ($LASTEXITCODE)"
    exit 1
}
Write-Ok "dist/ built"

# ------------------------------------------------------------------------------
# 3. Prepare deploy/ folder
# ------------------------------------------------------------------------------
Write-Step "Preparing deploy/ folder"

if (Test-Path $DeployDir) {
    Remove-Item -Recurse -Force $DeployDir
}
New-Item -ItemType Directory -Path $DeployDir | Out-Null
Write-Ok "Folder $DeployDir created"

# 3.1 backend.exe
Copy-Item "$ProjectRoot\target\release\backend.exe" "$DeployDir\backend.exe"
$ExeSize = [math]::Round((Get-Item "$DeployDir\backend.exe").Length / 1MB, 1)
Write-Ok "backend.exe  ($ExeSize MB)"

# 3.2 dist/ (frontend WASM)
Copy-Item -Recurse "$ProjectRoot\dist" "$DeployDir\dist"
$DistCount = (Get-ChildItem "$DeployDir\dist" -Recurse -File).Count
$DistSize  = [math]::Round((Get-ChildItem "$DeployDir\dist" -Recurse | Measure-Object -Property Length -Sum).Sum / 1MB, 1)
Write-Ok "dist/  ($DistCount files, $DistSize MB)"

# 3.3 knowledge/ (LLM knowledge base, if exists)
$KnowledgeSrc = "$ProjectRoot\knowledge"
if (Test-Path $KnowledgeSrc) {
    Copy-Item -Recurse $KnowledgeSrc "$DeployDir\knowledge"
    $KbCount = (Get-ChildItem "$DeployDir\knowledge" -Recurse -File).Count
    Write-Ok "knowledge/  ($KbCount files)"
} else {
    Write-Info "knowledge/ not found - skipping"
}

# 3.4 config.toml template (without developer-specific paths)
@'
# Marketplace Integrator - server configuration
# Edit paths before first run, then rename to config.toml

[database]
path = 'C:\Users\udv\Desktop\MPI\data\app.db'

[scheduled_tasks]
enabled = true

[llm]
knowledge_base_path = 'C:\Users\udv\Desktop\MPI\knowledge'
'@ | Out-File -FilePath "$DeployDir\config.toml.template" -Encoding utf8
Write-Ok "config.toml.template"

# 3.5 DEPLOY.md
@'
# Deployment Guide

## Files in this folder

| File / Folder        | Description                        |
|----------------------|------------------------------------|
| backend.exe          | Application server                 |
| dist/                | Frontend (WASM + CSS + JS)         |
| knowledge/           | LLM knowledge base (MD files)      |
| config.toml.template | Configuration template             |

## First installation

1. Copy all files to server: C:\Users\udv\Desktop\MPI\
2. Rename config.toml.template -> config.toml
3. Edit paths in config.toml for your server
4. Run backend.exe

## Update (subsequent deployments)

1. Stop service:   Stop-Service MarketplaceBackend
2. Replace files:  backend.exe (always), dist/ and knowledge/ (if changed)
3. Start service:  Start-Service MarketplaceBackend

DB migrations are applied automatically on startup.

## Automatic restart after database restore

The backend exits cleanly after staging a restored database. NSSM must restart it:

```powershell
nssm set MarketplaceBackend AppExit Default Restart
nssm set MarketplaceBackend AppRestartDelay 2000
```
'@ | Out-File -FilePath "$DeployDir\DEPLOY.md" -Encoding utf8
Write-Ok "DEPLOY.md"

# ------------------------------------------------------------------------------
# 4. Summary
# ------------------------------------------------------------------------------
$TotalSize = [math]::Round((Get-ChildItem $DeployDir -Recurse | Measure-Object -Property Length -Sum).Sum / 1MB, 1)

Write-Host ""
Write-Host "============================================" -ForegroundColor Green
Write-Host "  DONE!"                                      -ForegroundColor Green
Write-Host "============================================" -ForegroundColor Green
Write-Host ""
Write-Host "  Folder: $DeployDir" -ForegroundColor White
Write-Host "  Total:  $TotalSize MB" -ForegroundColor White
Write-Host ""
Write-Host "  Contents:" -ForegroundColor White
Get-ChildItem $DeployDir | ForEach-Object {
    $size = if ($_.PSIsContainer) {
        $sub = (Get-ChildItem $_.FullName -Recurse | Measure-Object -Property Length -Sum).Sum
        "[" + [math]::Round($sub / 1MB, 1) + " MB]"
    } else {
        "[" + [math]::Round($_.Length / 1MB, 1) + " MB]"
    }
    Write-Host ("    {0,-30} {1}" -f $_.Name, $size) -ForegroundColor Gray
}
Write-Host ""
Write-Host "  Next: copy deploy\ folder to server." -ForegroundColor Yellow
Write-Host ""
