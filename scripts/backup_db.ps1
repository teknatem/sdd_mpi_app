# Database Backup Script for Marketplace Integrator
# Creates timestamped backups of the database

# Рабочие данные лежат вне репозитория (см. [database].path в config.toml)
$DataRoot = "F:\data\sdd_mpi_app"
$DbPath = "$DataRoot\db\app.db"
$BackupDir = "$DataRoot\backups"
$Timestamp = Get-Date -Format "yyyy-MM-dd_HH-mm-ss"
$BackupPath = "$BackupDir\app_backup_$Timestamp.db"

# Create backup directory if it doesn't exist
if (-not (Test-Path $BackupDir)) {
    New-Item -ItemType Directory -Path $BackupDir -Force | Out-Null
    Write-Host 'Created backup directory:' $BackupDir
}

# Check if database exists
if (Test-Path $DbPath) {
    # Copy database to backup
    Copy-Item -Path $DbPath -Destination $BackupPath -Force
    
    $DbSize = (Get-Item $DbPath).Length
    $DbSizeKB = [math]::Round($DbSize / 1KB, 2)
    
    Write-Host 'Database backed up successfully!' -ForegroundColor Green
    Write-Host '  Source:' $DbPath '(' $DbSizeKB 'KB )'
    Write-Host '  Backup:' $BackupPath
    
    # Keep only last 30 backups (delete older ones)
    Get-ChildItem $BackupDir -Filter "app_backup_*.db" | 
        Sort-Object LastWriteTime -Descending | 
        Select-Object -Skip 30 | 
        Remove-Item -Force
    
    $BackupCount = (Get-ChildItem $BackupDir -Filter "app_backup_*.db").Count
    Write-Host '  Total backups:' $BackupCount '(keeping last 30)'
} else {
    Write-Host 'Database not found at:' $DbPath -ForegroundColor Red
    Write-Host '  Make sure the application has been run at least once.'
    exit 1
}

# Optional: Create a "latest" symlink/copy for easy access
$LatestPath = "$BackupDir\app_latest.db"
Copy-Item -Path $DbPath -Destination $LatestPath -Force
Write-Host '  Latest backup also saved to:' $LatestPath
