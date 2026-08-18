# Project Utility Scripts

## Overview

This directory contains utility scripts for the marketplace project:

- **Database Management**: Backup and restore scripts
- **Code Analysis**: Scripts for collecting and analyzing codebase

---

## Code Collection Script

### Purpose

The `collect_crates.py` script collects all code files from the `crates/` directory into a single text file, optimized for analysis by LLM (Language Learning Models).

### Usage

```powershell
# Run from project root
python scripts\collect_crates.py
```

### Output

- File: `scripts/crates_dump.txt`
- Format: Each file is preceded by a header with full path, size, and encoding
- Statistics: Summary at the beginning with file counts by extension

### Included File Types

- `.rs` - Rust source code
- `.toml` - Configuration files
- `.md` - Documentation
- `.css` - Stylesheets
- `.js` - JavaScript
- `.html` - HTML templates

### Excluded Directories

- `target/` - Build artifacts
- `node_modules/` - Node.js dependencies
- `.git/` - Git metadata

### Use Cases

- Analyzing codebase structure
- Preparing code for LLM review
- Documentation generation
- Code auditing
- Quick codebase overview

---

## Database Backup Scripts

The default database location (`target/db/app.db`) can be **deleted** when running:

- `cargo clean`
- Clearing build artifacts
- Switching git branches with different build states

### Solution

1. **config.toml** moves database to `data/app.db` (outside target/)
2. **Automatic backups** before each run
3. **Manual backup/restore scripts**

### Quick Start

#### 1. Initial Setup (First Time Only)

```powershell
# Copy your current database to the new location
Copy-Item "target\db\app.db" "data\app.db"

# Or if you have old data in crates\backend\target\db\:
Copy-Item "crates\backend\target\db\app.db" "data\app.db"
```

#### 2. Daily Usage

**Before starting work:**

```powershell
.\scripts\auto_backup.ps1
```

**Manual backup anytime:**

```powershell
.\scripts\backup_db.ps1
```

**Restore from backup:**

```powershell
.\scripts\restore_db.ps1
# Then select from list of available backups
```

## All Scripts

| File                  | Purpose                                                    |
| --------------------- | ---------------------------------------------------------- |
| `build-release.ps1`   | Build backend + frontend, pack into deploy/ for deployment |
| `build-release-max.ps1` | Build with fat LTO and maximum safe optimizations         |
| `update-release.ps1`  | Verify, deploy and roll back through Servy                  |
| `install-servy.ps1`   | Register the application in Servy on first installation     |
| `collect_crates.py`   | Collects all code files from crates/ into single text file |
| `backup_db.ps1`       | Creates timestamped database backup                        |
| `restore_db.ps1`      | Restores database from backup                              |
| `auto_backup.ps1`     | Auto-backup if database changed                            |

---

## Release Build Script

### Purpose

`build-release.ps1` собирает релиз. В `deploy/` остаются ровно два файла для переноса на сервер: ZIP и стабильный `update-release.ps1`.

### Usage

```powershell
# Запуск из корня проекта
.\scripts\build-release.ps1

# Максимальная оптимизация (fat LTO, один codegen unit, strip, panic=abort)
.\scripts\build-release-max.ps1

# Только если сборочная и целевая машины имеют совместимый CPU
.\scripts\build-release-max.ps1 -NativeCpu
```

### Output

```
deploy/
├── marketplace-deploy.zip # приложение, frontend и миграции
└── update-release.ps1              # постоянный серверный скрипт обновления
```

### Workflow

1. Запустить `.\scripts\build-release.ps1` или `.\scripts\build-release-max.ps1`.
2. Скопировать оба файла из `deploy\` на сервер в один каталог.
3. Запустить PowerShell от администратора и выполнить `.\update-release.ps1`.

По умолчанию обновляется `C:\Users\udv\Desktop\MPI`, имя службы Servy — `backend`.

### Backup Location

```
sdd_mpi_app/
├── data/
│   ├── app.db                          # Main database
│   └── backups/
│       ├── app_backup_2025-12-10_14-30-00.db
│       ├── app_backup_2025-12-10_15-45-00.db
│       └── app_latest.db               # Most recent backup
```

### Backup Retention

- Keeps **last 30 backups** automatically
- Older backups are automatically deleted
- `app_latest.db` is always the most recent

### Integration with Development

Add to your workflow:

```powershell
# Start backend (with auto-backup)
.\scripts\auto_backup.ps1; cargo run --bin backend

# Or create a convenience script
.\run_backend.ps1
```

### Scheduled Backups (Optional)

Create a Windows Task Scheduler task to run `backup_db.ps1` daily:

```powershell
# Run as Administrator
$Action = New-ScheduledTaskAction -Execute "powershell.exe" -Argument "-File F:\dev\sdd_mpi_app\scripts\backup_db.ps1"
$Trigger = New-ScheduledTaskTrigger -Daily -At 9am
Register-ScheduledTask -TaskName "Marketplace DB Backup" -Action $Action -Trigger $Trigger
```

### Emergency Recovery

If you lost data:

1. Check `data\backups\` for recent backups
2. Run `.\scripts\restore_db.ps1`
3. Select the backup to restore
4. Your current DB is automatically backed up before restore

### .gitignore

Database files are excluded from git:

```
/data/app.db
/data/backups/*.db
/target/db/*.db
```

But scripts and config are tracked for team sharing.
