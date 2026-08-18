# Database Restore Script for Marketplace Integrator
# Restores database from a backup file

param(
    [Parameter(Mandatory=$false)]
    [string]$BackupFile
)

# Рабочие данные лежат вне репозитория (см. [database].path в config.toml)
$DataRoot = "F:\data\sdd_mpi_app"
$DbPath = "$DataRoot\db\app.db"
$BackupDir = "$DataRoot\backups"

# If no backup file specified, show available backups
if (-not $BackupFile) {
    Write-Host "Available backups:" -ForegroundColor Cyan
    Write-Host ""
    
    $Backups = Get-ChildItem $BackupDir -Filter "app_backup_*.db" | 
        Sort-Object LastWriteTime -Descending
    
    if ($Backups.Count -eq 0) {
        Write-Host "No backups found in: $BackupDir" -ForegroundColor Red
        exit 1
    }
    
    for ($i = 0; $i -lt $Backups.Count; $i++) {
        $Backup = $Backups[$i]
        $Size = [math]::Round($Backup.Length / 1KB, 2)
        Write-Host "[$i] $($Backup.Name) - $Size KB - $($Backup.LastWriteTime)"
    }
    
    Write-Host ""
    Write-Host "Usage: .\restore_db.ps1 -BackupFile <path_to_backup>" -ForegroundColor Yellow
    Write-Host "   or: .\restore_db.ps1 (then select number)" -ForegroundColor Yellow
    Write-Host ""
    
    $Selection = Read-Host "Select backup number to restore (or press Enter to cancel)"
    
    if ($Selection -eq "") {
        Write-Host "Cancelled." -ForegroundColor Yellow
        exit 0
    }
    
    $SelectedIndex = [int]$Selection
    if ($SelectedIndex -lt 0 -or $SelectedIndex -ge $Backups.Count) {
        Write-Host "Invalid selection." -ForegroundColor Red
        exit 1
    }
    
    $BackupFile = $Backups[$SelectedIndex].FullName
}

# Check if backup file exists
if (-not (Test-Path $BackupFile)) {
    Write-Host "✗ Backup file not found: $BackupFile" -ForegroundColor Red
    exit 1
}

# Confirm restore
Write-Host ""
Write-Host "WARNING: This will replace the current database!" -ForegroundColor Yellow
Write-Host "Current DB: $DbPath"
Write-Host "Restore from: $BackupFile"
Write-Host ""
$Confirm = Read-Host "Are you sure? (yes/no)"

if ($Confirm -ne "yes") {
    Write-Host "Cancelled." -ForegroundColor Yellow
    exit 0
}

# Create backup of current database before restoring
if (Test-Path $DbPath) {
    $PreRestoreBackup = "$BackupDir\app_before_restore_$(Get-Date -Format 'yyyy-MM-dd_HH-mm-ss').db"
    Copy-Item -Path $DbPath -Destination $PreRestoreBackup -Force
    Write-Host "✓ Current database backed up to: $PreRestoreBackup" -ForegroundColor Green
}

# Restore from backup
Copy-Item -Path $BackupFile -Destination $DbPath -Force

Write-Host "✓ Database restored successfully!" -ForegroundColor Green
Write-Host "  From: $BackupFile"
Write-Host "  To: $DbPath"

