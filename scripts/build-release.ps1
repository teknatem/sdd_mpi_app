# ==============================================================================
# build-release.ps1 - Build and package release for server deployment
#
# Creates deploy/ folder in project root with everything needed to copy.
# Run from any location: .\scripts\build-release.ps1
# ==============================================================================

param(
    [ValidateSet("release", "release-max")]
    [string]$CargoProfile = "release",
    [switch]$SkipBuild
)

$ProjectRoot = Split-Path $PSScriptRoot -Parent
$DeployDir   = "$ProjectRoot\deploy"
$PayloadDir  = "$DeployDir\.payload"
$TargetDir   = "$ProjectRoot\target\$CargoProfile"
$ArchivePath = "$DeployDir\marketplace-deploy.zip"

function Write-Step { param($msg) Write-Host "`n>>> $msg" -ForegroundColor Cyan }
function Write-Ok   { param($msg) Write-Host "  OK   $msg" -ForegroundColor Green }
function Write-Fail { param($msg) Write-Host "  FAIL $msg" -ForegroundColor Red }
function Write-Info { param($msg) Write-Host "       $msg" -ForegroundColor Gray }

Set-Location $ProjectRoot
Write-Host ""
Write-Host "============================================" -ForegroundColor Yellow
Write-Host "  MARKETPLACE - BUILD $($CargoProfile.ToUpper())" -ForegroundColor Yellow
Write-Host "============================================" -ForegroundColor Yellow

# ------------------------------------------------------------------------------
# 1. Build backend
# ------------------------------------------------------------------------------
if (-not $SkipBuild) {
    Write-Step "Building backend (cargo build --profile $CargoProfile)"
    cargo build --profile $CargoProfile --bin backend
    if ($LASTEXITCODE -ne 0) {
        Write-Fail "cargo build failed ($LASTEXITCODE)"
        exit 1
    }
    Write-Ok "backend.exe built"
} else {
    Write-Info "Backend build skipped; using existing $TargetDir\backend.exe"
}

# ------------------------------------------------------------------------------
# 2. Build frontend
# ------------------------------------------------------------------------------
if (-not $SkipBuild) {
    Write-Step "Building frontend (trunk, cargo profile $CargoProfile)"
    if ($CargoProfile -eq "release") {
        # Профиль передаётся явно: `--release` перебивается ключом cargo_profile
        # из Trunk.toml (там стоит wasm-dev для dev-цикла) и молча собрал бы
        # неоптимизированный wasm на 66 МБ под видом релиза.
        trunk build --cargo-profile release
    } else {
        trunk build --cargo-profile $CargoProfile
    }
    if ($LASTEXITCODE -ne 0) {
        Write-Fail "trunk build failed ($LASTEXITCODE)"
        exit 1
    }
    Write-Ok "dist/ built"
} else {
    Write-Info "Frontend build skipped; using existing $ProjectRoot\dist"
}

# ------------------------------------------------------------------------------
# 3. Prepare deploy/ folder
# ------------------------------------------------------------------------------
Write-Step "Preparing deploy/ folder"

if (Test-Path $DeployDir) {
    Remove-Item -Recurse -Force $DeployDir
}
New-Item -ItemType Directory -Path $PayloadDir | Out-Null
Write-Ok "Folder $DeployDir created"

# 3.1 backend.exe
Copy-Item "$TargetDir\backend.exe" "$PayloadDir\backend.exe"
$ExeSize = [math]::Round((Get-Item "$PayloadDir\backend.exe").Length / 1MB, 1)
Write-Ok "backend.exe  ($ExeSize MB)"

# 3.2 dist/ (frontend WASM)
Copy-Item -Recurse "$ProjectRoot\dist" "$PayloadDir\dist"
$DistCount = (Get-ChildItem "$PayloadDir\dist" -Recurse -File).Count
$DistSize  = [math]::Round((Get-ChildItem "$PayloadDir\dist" -Recurse | Measure-Object -Property Length -Sum).Sum / 1MB, 1)
Write-Ok "dist/  ($DistCount files, $DistSize MB)"

# 3.3 migrations/ (required at backend startup)
Copy-Item -Recurse "$ProjectRoot\migrations" "$PayloadDir\migrations"
$MigrationCount = (Get-ChildItem "$PayloadDir\migrations" -File).Count
Write-Ok "migrations/  ($MigrationCount files)"

# 3.4 knowledge/ (LLM knowledge base, if exists)
$KnowledgeSrc = "$ProjectRoot\knowledge"
if (Test-Path $KnowledgeSrc) {
    Copy-Item -Recurse $KnowledgeSrc "$PayloadDir\knowledge"
    $KbCount = (Get-ChildItem "$PayloadDir\knowledge" -Recurse -File).Count
    Write-Ok "knowledge/  ($KbCount files)"
} else {
    Write-Info "knowledge/ not found - skipping"
}

# 3.5 Current full config template (never overwrites config.toml on update)
Copy-Item "$ProjectRoot\config.toml.example" "$PayloadDir\config.toml.template"
Write-Ok "config.toml.template"

# 3.6 One-time service installer belongs to the payload; the stable updater does not.
Copy-Item "$PSScriptRoot\install-servy.ps1" "$PayloadDir\install-servy.ps1"
Write-Ok "install-servy.ps1"

# 3.7 DEPLOY.md
@'
# Deployment Guide

## Files in this folder

| File / Folder        | Description                        |
|----------------------|------------------------------------|
| backend.exe          | Application server                 |
| dist/                | Frontend (WASM + CSS + JS)         |
| migrations/          | SQL migrations (applied on startup)|
| knowledge/           | LLM knowledge base (MD files)      |
| config.toml.template | Configuration template             |
| install-servy.ps1    | One-time Servy service installer    |
| release-manifest.json| Build metadata and SHA-256 hashes   |

## First installation

1. Extract the release archive to a temporary folder.
2. Copy all files to server: C:\Users\udv\Desktop\MPI\
3. Rename config.toml.template -> config.toml
4. Edit paths in config.toml for your server
5. Run as Administrator: `.\install-servy.ps1 -InstallDir 'C:\Users\udv\Desktop\MPI'`.

The Servy installer sets `backend.exe` as the executable and the installation
folder as `startupDir` (required because the backend serves `dist/` relatively).

## Update (subsequent deployments)

Place the ZIP and `update-release.ps1` in one folder and run as Administrator:

```powershell
.\update-release.ps1
```

The script extracts the ZIP, validates hashes, stages files, stops `backend` via Servy, replaces
backend.exe/dist/migrations, starts it, checks /health and rolls back on failure.
It does not overwrite config.toml or application data.

DB migrations are applied automatically on startup.
Before a release with schema changes, create a database backup. File rollback
does not reverse migrations that were already committed to the database.

Servy can additionally be configured in its Recovery tab to restart the process
after an unexpected exit. Keep the application-level `/health` check enabled in
your external monitoring as well.
'@ | Out-File -FilePath "$PayloadDir\DEPLOY.md" -Encoding utf8
Write-Ok "DEPLOY.md"

# 3.8 Payload integrity manifest (the manifest itself is intentionally excluded)
$GitCommit = (git rev-parse --short HEAD 2>$null)
if (-not $GitCommit) { $GitCommit = "unknown" }
$ManifestFiles = Get-ChildItem $PayloadDir -Recurse -File | ForEach-Object {
    [ordered]@{
        path   = $_.FullName.Substring($PayloadDir.Length + 1).Replace("\", "/")
        bytes  = $_.Length
        sha256 = (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    }
}
[ordered]@{
    created_utc = [DateTime]::UtcNow.ToString("o")
    profile     = $CargoProfile
    git_commit  = $GitCommit
    files       = @($ManifestFiles)
} | ConvertTo-Json -Depth 4 | Set-Content "$PayloadDir\release-manifest.json" -Encoding utf8
Write-Ok "release-manifest.json  ($($ManifestFiles.Count) hashed files)"

# 3.9 Single-file transport artifact
Compress-Archive -Path "$PayloadDir\*" -DestinationPath $ArchivePath -CompressionLevel Optimal
Remove-Item -Recurse -Force -LiteralPath $PayloadDir
Copy-Item "$PSScriptRoot\update-release.ps1" "$DeployDir\update-release.ps1"
Write-Ok "$([IO.Path]::GetFileName($ArchivePath))"
Write-Ok "update-release.ps1"

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
Write-Host "  Copy these two files to the server:" -ForegroundColor White
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
Write-Host "  Next: copy both files to the server and run update-release.ps1 as Administrator." -ForegroundColor Yellow
Write-Host ""
