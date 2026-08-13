# Stable server-side updater. Keep this file next to marketplace-deploy.zip.
[CmdletBinding()]
param(
    [string]$ArchivePath,
    [string]$InstallDir = "C:\Users\udv\Desktop\MPI",
    [string]$ServiceName = "backend",
    [string]$ServyCli = "servy-cli.exe",
    [string]$HealthUrl = "http://127.0.0.1:3000/health",
    [int]$HealthTimeoutSeconds = 60,
    [switch]$SkipServiceControl
)

$ErrorActionPreference = "Stop"
$InstallDir = [IO.Path]::GetFullPath($InstallDir)
$TrimChars = [char[]]@('\', '/')
$InstallRoot = [IO.Path]::GetPathRoot($InstallDir).TrimEnd($TrimChars)
if ($InstallDir.TrimEnd($TrimChars) -eq $InstallRoot) {
    throw "InstallDir must not be a drive root: $InstallDir"
}

if ([string]::IsNullOrWhiteSpace($ArchivePath)) {
    $ArchivePath = Join-Path $PSScriptRoot "marketplace-deploy.zip"
} else {
    $ArchivePath = [IO.Path]::GetFullPath($ArchivePath)
}
if (-not (Test-Path -LiteralPath $ArchivePath -PathType Leaf)) {
    throw "Release archive not found: $ArchivePath"
}

$ManagedItems = @("backend.exe", "dist", "migrations")
$Timestamp = "$(Get-Date -Format 'yyyyMMdd-HHmmss')-$([guid]::NewGuid().ToString('N').Substring(0, 8))"
$ExtractDir = Join-Path ([IO.Path]::GetTempPath()) "marketplace-release-$Timestamp"
$PackageDir = $ExtractDir
$ManifestPath = Join-Path $PackageDir "release-manifest.json"
$BackupRoot = Join-Path $InstallDir ".deploy-backups"
$BackupDir = Join-Path $BackupRoot $Timestamp
$StageDir = Join-Path $InstallDir ".deploy-stage-$Timestamp"
$ServiceWasRunning = $false
$BackedUpItems = @()
$InstalledItems = @()

function Invoke-Servy {
    param([ValidateSet("start", "stop")][string]$Action)
    & $ServyCli $Action "--name=$ServiceName" --quiet
    if ($LASTEXITCODE -ne 0) {
        throw "Servy failed to $Action service '$ServiceName' (exit code $LASTEXITCODE)."
    }
}

function Wait-ForHealth {
    $deadline = (Get-Date).AddSeconds($HealthTimeoutSeconds)
    do {
        try {
            $response = Invoke-WebRequest -Uri $HealthUrl -UseBasicParsing -TimeoutSec 5
            if ($response.StatusCode -ge 200 -and $response.StatusCode -lt 300) { return $true }
        } catch {
            Start-Sleep -Seconds 2
        }
    } while ((Get-Date) -lt $deadline)
    return $false
}

function Restore-Backup {
    Write-Warning "Deployment failed; restoring $BackupDir"
    foreach ($item in $InstalledItems) {
        $current = Join-Path $InstallDir $item
        if (Test-Path -LiteralPath $current) {
            Remove-Item -Recurse -Force -LiteralPath $current
        }
    }
    foreach ($item in $BackedUpItems) {
        $saved = Join-Path $BackupDir $item
        if (Test-Path -LiteralPath $saved) {
            Move-Item -LiteralPath $saved -Destination (Join-Path $InstallDir $item)
        }
    }
}

try {
    Write-Host "Extracting $ArchivePath" -ForegroundColor Cyan
    New-Item -ItemType Directory -Path $ExtractDir | Out-Null
    Expand-Archive -LiteralPath $ArchivePath -DestinationPath $ExtractDir

    if (-not (Test-Path -LiteralPath $ManifestPath -PathType Leaf)) {
        throw "release-manifest.json is missing from the archive."
    }
    foreach ($item in $ManagedItems) {
        if (-not (Test-Path -LiteralPath (Join-Path $PackageDir $item))) {
            throw "Release payload is incomplete: $item is missing."
        }
    }

    $manifest = Get-Content -Raw -LiteralPath $ManifestPath | ConvertFrom-Json
    $PackageRoot = [IO.Path]::GetFullPath($PackageDir).TrimEnd($TrimChars) + [IO.Path]::DirectorySeparatorChar
    foreach ($file in $manifest.files) {
        $source = [IO.Path]::GetFullPath((Join-Path $PackageDir ($file.path -replace '/', [IO.Path]::DirectorySeparatorChar)))
        if (-not $source.StartsWith($PackageRoot, [StringComparison]::OrdinalIgnoreCase)) {
            throw "Unsafe path in release manifest: $($file.path)"
        }
        if (-not (Test-Path -LiteralPath $source -PathType Leaf)) {
            throw "Manifest file missing: $($file.path)"
        }
        $actual = (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($actual -ne $file.sha256) { throw "Checksum mismatch: $($file.path)" }
    }
    Write-Host "Release verified: profile=$($manifest.profile), commit=$($manifest.git_commit)" -ForegroundColor Green

    New-Item -ItemType Directory -Force -Path $InstallDir, $BackupRoot, $BackupDir, $StageDir | Out-Null
    foreach ($item in $ManagedItems) {
        Copy-Item -Recurse -LiteralPath (Join-Path $PackageDir $item) -Destination (Join-Path $StageDir $item)
    }

    if (-not $SkipServiceControl) {
        if (-not (Get-Command $ServyCli -ErrorAction SilentlyContinue)) {
            throw "Servy CLI not found: $ServyCli"
        }
        $service = Get-Service -Name $ServiceName -ErrorAction Stop
        if ($service.Status -ne "Stopped") {
            Invoke-Servy stop
            $ServiceWasRunning = $true
        }
    }

    foreach ($item in $ManagedItems) {
        $current = Join-Path $InstallDir $item
        if (Test-Path -LiteralPath $current) {
            Move-Item -LiteralPath $current -Destination (Join-Path $BackupDir $item)
            $BackedUpItems += $item
        }
        Move-Item -LiteralPath (Join-Path $StageDir $item) -Destination $current
        $InstalledItems += $item
    }

    if (-not $SkipServiceControl -and $ServiceWasRunning) {
        Invoke-Servy start
        if (-not (Wait-ForHealth)) {
            throw "Service did not become healthy at $HealthUrl within $HealthTimeoutSeconds seconds."
        }
    }

    Write-Host "Deployment completed. Backup: $BackupDir" -ForegroundColor Green
}
catch {
    $failure = $_
    if ($InstalledItems.Count -gt 0 -or $BackedUpItems.Count -gt 0) {
        if (-not $SkipServiceControl) {
            & $ServyCli stop "--name=$ServiceName" --quiet 2>$null
        }
        Restore-Backup
    }
    if (-not $SkipServiceControl -and $ServiceWasRunning) {
        & $ServyCli start "--name=$ServiceName" --quiet 2>$null
    }
    throw $failure
}
finally {
    if (Test-Path -LiteralPath $StageDir) {
        Remove-Item -Recurse -Force -LiteralPath $StageDir
    }
    if (Test-Path -LiteralPath $ExtractDir) {
        Remove-Item -Recurse -Force -LiteralPath $ExtractDir
    }
}
