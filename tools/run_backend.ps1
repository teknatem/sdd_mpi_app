# Перезапуск dev-бэкенда.
#
# Запущенный backend.exe держит target\debug\backend.exe, из-за чего повторный
# `cargo run -p backend` падает с "Access is denied (os error 5)". Скрипт
# останавливает все запущенные экземпляры и запускает свежую сборку.
#
# Использование (из корня репозитория):
#   powershell -File tools/restart_backend.ps1

$ErrorActionPreference = "Stop"

$procs = @(Get-Process backend -ErrorAction SilentlyContinue)
foreach ($p in $procs) {
    Write-Host "Останавливаю backend.exe (PID $($p.Id), запущен $($p.StartTime))..."
    Stop-Process -Id $p.Id -Force
}
if ($procs.Count -gt 0) {
    Start-Sleep -Milliseconds 500
}

cargo run -p backend
