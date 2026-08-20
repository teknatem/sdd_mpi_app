---
date: 2026-01-19
type: known-issue
tags: [disk-space, wasm, windows, compilation, cargo]
severity: medium
frequency: occasional
platforms: [windows]
status: documented
---

# Known Issue: Disk Space Exhaustion During WASM Compilation on Windows

## Issue ID
KI_disk-space-wasm-windows_2026-01-19

## Summary
Cargo WASM builds fail with "There is not enough space on the disk (os error 112)" when compiling large Rust projects like Leptos applications on Windows systems with limited disk space.

## Symptoms

### Primary Error
```
error: failed to build archive at `E:\...\target\wasm32-unknown-unknown\debug\deps\libfrontend.rlib`: 
There is not enough space on the disk. (os error 112)
```

### Secondary Indicators
- Build succeeds initially but fails during final archive creation
- Multiple dependency compilations complete successfully
- Error occurs at `Compiling <project>` stage, not during dependency compilation
- May be preceded by slower-than-usual build times

### Related Errors When Attempting `cargo clean`
```
error: failed to remove file `...\backend.exe`
Caused by: Access is denied. (os error 5)
```

```
error: failed to write `...\invoked.timestamp`
Caused by: The system cannot find the path specified. (os error 3)
```

## Root Cause

1. **WASM Artifact Accumulation**: The `target/wasm32-unknown-unknown/` directory accumulates large build artifacts over time
2. **Debug Builds**: Unoptimized debug builds produce larger artifacts than release builds
3. **Incremental Compilation**: Cargo's incremental compilation saves intermediate artifacts
4. **Windows File System**: NTFS can fragment `target/` directory with thousands of small files

## Detection

### Manual Check (PowerShell)
```powershell
# Check available disk space
Get-PSDrive E | Select-Object Used,Free

# Check target directory size
Get-ChildItem -Path "target" -Recurse | 
  Measure-Object -Property Length -Sum | 
  Select-Object @{Name="Size(GB)";Expression={$_.Sum / 1GB}}
```

### During Build
- Watch for os error 112 in cargo output
- Monitor disk space warnings in Windows
- Check if other background processes are also reporting disk errors

## Fix / Workaround

### Immediate Fix (When Build Fails)

**Option 1: Clean WASM Target Only** (Recommended when backend is running)
```powershell
# PowerShell
Remove-Item -Path "target\wasm32-unknown-unknown" -Recurse -Force -ErrorAction SilentlyContinue

# Then rebuild (профиль обязателен — иначе cargo заведёт второй набор wasm-артефактов
# в target/wasm32-unknown-unknown/debug, ровно то, что мы только что удалили)
powershell -File tools/build_frontend.ps1
```

**Option 2: Full Clean** (When no processes are running)
```powershell
# Stop the backend first (tools/run_backend.ps1 снимает занятый backend.exe)
cargo clean

# Then rebuild both backend and frontend
cargo build
```

**Option 3: Clean Specific Package**
```bash
cargo clean -p frontend
```

### Preventive Measures

1. **Periodic Cleanup**: Schedule regular `cargo clean` or selective target cleanup
2. **Release Builds**: Use `--release` flag when possible (smaller artifacts)
3. **Separate Drive**: Configure `CARGO_TARGET_DIR` to point to a larger drive
   ```powershell
   $env:CARGO_TARGET_DIR="D:\rust-target"
   ```
4. **Disk Space Monitoring**: Set up Windows disk space alerts

### Emergency Recovery
If disk is completely full:
```powershell
# 1. Stop all development processes
# 2. Delete only WASM debug artifacts
Remove-Item -Path "target\wasm32-unknown-unknown\debug" -Recurse -Force

# 3. If still insufficient, remove all debug artifacts
Remove-Item -Path "target\debug" -Recurse -Force

# 4. Keep release builds if possible
```

## Impact

- **Build Failure**: Frontend compilation fails completely
- **Development Blocked**: Cannot test changes until space is freed
- **Time Loss**: ~2-5 minutes to clean and rebuild
- **Data Safety**: No risk to source code or database

## Affected Components

- ✅ Frontend (WASM target) - Primary victim
- ⚠️ Backend (native target) - Can also be affected if disk is critically low
- ❌ Source code - Not affected
- ❌ Database - Not affected

## Workaround Limitations

- **Selective cleanup** требует, чтобы сборка фронта не шла в этот момент (вотчера в цикле нет — сборка разовая, `tools/build_frontend.ps1`)
- **Full cleanup** требует остановленного бэкенда: он держит `target\debug\backend.exe` и раздаёт `dist/` на :3000
- **Large projects** may take 1-3 minutes to rebuild after cleanup

## Environment Specifics

### Windows 11
- More likely on system drive (C:) with Windows, apps, and dev tools
- NTFS file system can slow down with fragmented `target/` directories

### Rust/Cargo Version
- Affects all Cargo versions
- WASM builds particularly space-intensive due to `wasm-bindgen` artifacts

### Project Characteristics
- **Leptos apps**: Large dependency trees increase artifact size
- **Workspaces**: Multiple crates multiply artifact count
- **Debug builds**: 2-5x larger than release builds

## Prevention Strategies

### For Individual Developers

1. **Monitor Disk Space**: Keep at least 10-20GB free on development drive
2. **Regular Cleanup**: Run `cargo clean` weekly or when switching major features
3. **Use Release Profile**: Test with `--release` when debugging isn't needed
4. **External Target Dir**: Move `target/` to secondary drive with more space

### For Teams

1. **Documentation**: Document minimum disk space requirements (e.g., 50GB free)
2. **CI/CD**: Ensure build servers have adequate space and cleanup between builds
3. **Onboarding**: Include disk space setup in developer onboarding
4. **Gitignore**: Ensure `target/` is properly ignored (prevents accidental commits)

## Related Issues

- Windows file locking when backend is running prevents full `cargo clean`
- PowerShell path handling for cleanup commands
- Пересборка фронта после очистки — разовая команда `tools/build_frontend.ps1`, вотчера в цикле нет

## References

- Rust Book: [Build Artifacts](https://doc.rust-lang.org/cargo/guide/build-cache.html)
- Cargo Docs: [Target Directory](https://doc.rust-lang.org/cargo/guide/build-cache.html#build-cache)
- Windows Error Codes: [os error 112 = ERROR_DISK_FULL](https://learn.microsoft.com/en-us/windows/win32/debug/system-error-codes--0-499-)

## Version History

- **2026-01-19**: Initial documentation
  - Discovered during frontend WASM compilation
  - Documented selective cleanup approach
  - Added Windows-specific details
