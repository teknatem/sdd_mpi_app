---
date: 2026-01-19
type: runbook
tags: [disk-space, cargo, cleanup, maintenance, windows]
version: 1.0
platforms: [windows, linux, macos]
---

# Runbook: Recover from Disk Space Issues During Rust Builds

## Purpose
Step-by-step procedure to recover from disk space exhaustion during Rust/Cargo builds, particularly for Leptos WASM projects on Windows.

## Prerequisites
- Command-line access (PowerShell on Windows, bash on Unix)
- Ability to stop running development processes
- Understanding of your project's workspace structure

## When to Use This Runbook

### Triggers
- Build fails with "There is not enough space on the disk (os error 112)"
- `cargo build` hangs or is extremely slow
- Windows disk space warning appears during compilation
- Build succeeds for dependencies but fails for workspace crates

### Indicators You Should NOT Use This
- Source code changes aren't compiling (syntax errors)
- Permission errors unrelated to disk space
- Network issues during dependency download

## Procedure

### Step 1: Identify the Problem

**Check Current Disk Space:**

```powershell
# Windows PowerShell
Get-PSDrive E | Select-Object Used,Free,@{Name="PercentFree";Expression={($_.Free/$_.Used)*100}}

# Linux/Mac
df -h
```

**Expected Output:**
- ✅ **Healthy**: >10GB free
- ⚠️ **Warning**: 2-10GB free
- 🔴 **Critical**: <2GB free

**Check Target Directory Size:**

```powershell
# Windows PowerShell
Get-ChildItem -Path "target" -Recurse -ErrorAction SilentlyContinue | 
  Measure-Object -Property Length -Sum | 
  Select-Object @{Name="SizeGB";Expression={[math]::Round($_.Sum / 1GB, 2)}}

# Linux/Mac
du -sh target/
```

**Typical Sizes:**
- Small project: 1-5GB
- Medium project (Leptos): 5-15GB
- Large workspace: 15-50GB

---

### Step 2: Assess Running Processes

**Before cleaning, identify what's running:**

```powershell
# Windows PowerShell
Get-Process | Where-Object {$_.ProcessName -match "backend|trunk|cargo"}

# Linux/Mac
ps aux | grep -E "backend|trunk|cargo"
```

**Decision Tree:**
- **Backend running**: Use Option A (Selective WASM cleanup)
- **Nothing running**: Use Option B (Full cleanup)
- **Disk critically full**: Use Option C (Emergency cleanup)

---

### Step 3: Choose Cleanup Strategy

#### **Option A: Selective WASM Cleanup** (Recommended - Fastest)

**When to use:** Backend is running, only frontend needs rebuilding

```powershell
# 1. Navigate to project root
cd f:\dev\sdd_mpi_app

# 2. Stop trunk serve (Ctrl+C in its terminal)

# 3. Remove WASM artifacts
Remove-Item -Path "target\wasm32-unknown-unknown" -Recurse -Force -ErrorAction SilentlyContinue

# 4. Verify cleanup
Get-ChildItem target\ | Select-Object Name, @{Name="SizeMB";Expression={(Get-ChildItem $_.FullName -Recurse | Measure-Object -Property Length -Sum).Sum / 1MB}}

# 5. Rebuild frontend only
cargo build --target=wasm32-unknown-unknown --manifest-path crates/frontend/Cargo.toml

# 6. Restart trunk serve in separate terminal
trunk serve
```

**Expected Results:**
- Space freed: 3-10GB
- Rebuild time: 1-3 minutes
- Backend: Remains running (no downtime)

---

#### **Option B: Full Cleanup** (Most Thorough)

**When to use:** Nothing is running, or both frontend and backend need rebuilding

```powershell
# 1. Stop ALL processes
# - Ctrl+C in trunk serve terminal
# - Ctrl+C in backend terminal (or stop debugging)

# 2. Navigate to project root
cd f:\dev\sdd_mpi_app

# 3. Full cargo clean
cargo clean

# 4. Verify target directory is empty/small
Get-ChildItem target\ -ErrorAction SilentlyContinue

# 5. Rebuild entire project
cargo build

# 6. Rebuild frontend for WASM
cargo build --target=wasm32-unknown-unknown --manifest-path crates/frontend/Cargo.toml

# 7. Restart both processes
# Terminal 1: cargo run -p backend
# Terminal 2: trunk serve
```

**Expected Results:**
- Space freed: 10-50GB (all build artifacts removed)
- Rebuild time: 5-10 minutes (all dependencies recompiled)
- Downtime: ~5-10 minutes (both services restart)

---

#### **Option C: Emergency Cleanup** (When Disk is Completely Full)

**When to use:** Disk is so full that even `cargo clean` fails

```powershell
# 1. Stop ALL processes immediately

# 2. Navigate to project root
cd f:\dev\sdd_mpi_app

# 3. Remove debug artifacts manually (largest first)
Remove-Item -Path "target\wasm32-unknown-unknown\debug" -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item -Path "target\debug" -Recurse -Force -ErrorAction SilentlyContinue

# 4. Check if space is freed
Get-PSDrive E | Select-Object Free

# 5. If still full, remove incremental artifacts
Remove-Item -Path "target\debug\incremental" -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item -Path "target\wasm32-unknown-unknown\debug\incremental" -Recurse -Force -ErrorAction SilentlyContinue

# 6. Try cargo clean now
cargo clean

# 7. Full rebuild
cargo build
cargo build --target=wasm32-unknown-unknown --manifest-path crates/frontend/Cargo.toml
```

**Expected Results:**
- Space freed: 15-30GB (aggressive cleanup)
- Rebuild time: 5-10 minutes
- Risk: Release artifacts may also be removed if needed

---

### Step 4: Verify Recovery

**Check compilation succeeds:**

```powershell
# Test frontend build
cargo check -p frontend

# Test backend build
cargo check -p backend

# Test full workspace
cargo check
```

**Expected output:**
```
    Finished `dev` profile [unoptimized + debuginfo] target(s) in X.XXs
```

**Check disk space recovered:**

```powershell
Get-PSDrive E | Select-Object Free
```

---

### Step 5: Post-Recovery Actions

#### **Immediate Actions**

1. **Restart development servers:**
   ```powershell
   # Terminal 1
   cargo run -p backend
   
   # Terminal 2
   trunk serve
   ```

2. **Test application:**
   - Open browser to `http://localhost:8080`
   - Verify frontend loads
   - Test API endpoints

#### **Follow-up Tasks**

1. **Monitor disk space** for next few builds
2. **Document** how much space was freed (for future reference)
3. **Consider** moving `target/` to larger drive if this recurs

---

## Troubleshooting

### Problem: `cargo clean` fails with "Access denied"

**Cause:** Background process (backend.exe) is locking files

**Solution:**
```powershell
# 1. Find and stop the process
Get-Process | Where-Object {$_.ProcessName -match "backend"} | Stop-Process -Force

# 2. Try cargo clean again
cargo clean
```

---

### Problem: Cleanup doesn't free enough space

**Cause:** Other Rust projects or cached artifacts

**Solution:**
```powershell
# Check cargo global cache
Get-ChildItem $env:USERPROFILE\.cargo\registry | 
  Measure-Object -Property Length -Sum | 
  Select-Object @{Name="SizeGB";Expression={$_.Sum / 1GB}}

# Clean cargo cache (CAREFUL - affects all Rust projects)
cargo cache --autoclean

# Or manually remove old artifacts
Remove-Item $env:USERPROFILE\.cargo\registry\cache -Recurse -Force
```

---

### Problem: Build still fails after cleanup

**Cause:** Disk is critically low even after cleanup

**Solutions:**

1. **Use external target directory:**
   ```powershell
   # Set environment variable
   $env:CARGO_TARGET_DIR="D:\rust-target"
   
   # Add to PowerShell profile for persistence
   Add-Content $PROFILE "`n`$env:CARGO_TARGET_DIR='D:\rust-target'"
   ```

2. **Free up system space:**
   - Run Windows Disk Cleanup
   - Remove temporary files
   - Move large files to external storage

3. **Use release builds** (smaller artifacts):
   ```powershell
   cargo build --release
   ```

---

## Prevention

### Regular Maintenance (Weekly)

```powershell
# Add to weekly routine
cargo clean
# or
Remove-Item -Path "target\wasm32-unknown-unknown" -Recurse -Force
```

### Disk Space Monitoring

**Set up a PowerShell script:**

```powershell
# save as check-disk-space.ps1
$drive = Get-PSDrive E
$percentFree = ($drive.Free / ($drive.Used + $drive.Free)) * 100

if ($percentFree -lt 10) {
    Write-Host "⚠️ WARNING: Disk space below 10%!" -ForegroundColor Yellow
    Write-Host "Consider running: cargo clean" -ForegroundColor Yellow
} else {
    Write-Host "✅ Disk space OK: $([math]::Round($drive.Free / 1GB, 2))GB free"
}
```

### Project-Specific Configuration

**Add to `.cargo/config.toml`:**

```toml
[build]
# Move target to larger drive
target-dir = "D:/rust-target/sdd_mpi_app"
```

---

## Rollback

If cleanup causes issues (rare):

1. **Restore from git** (source code only):
   ```bash
   git status
   # target/ should not be tracked
   ```

2. **Rebuild** from clean state:
   ```bash
   cargo build
   ```

3. **No data loss** - cleanup only removes build artifacts, not source code or databases

---

## Related Documents

- [[KI_disk-space-wasm-windows_2026-01-19]] - Known issue details
- [[2026-01-19-session-debrief-disk-space-wasm-build]] - Original incident
- Project README - Development setup requirements

---

## Metrics

Track these for continuous improvement:

- **Frequency**: How often does this occur?
- **Space freed**: Average amount freed by cleanup
- **Rebuild time**: Time to recover after cleanup
- **Disk utilization trend**: Is usage growing over time?

---

## Change Log

- **2026-01-19 v1.0**: Initial runbook
  - Documented three cleanup strategies
  - Added Windows-specific PowerShell commands
  - Included troubleshooting for common issues
