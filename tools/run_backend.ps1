# Собрать и запустить dev-бэкенд.
#
# `cargo run -p backend` уже включает сборку, поэтому отдельной команды сборки
# бэкенда нет. Скрипт нужен из-за одного: запущенный backend.exe держит
# target\debug\backend.exe, и повторный `cargo run` падает с
# "Access is denied (os error 5)". Здесь старый процесс снимается, дальше —
# обычный `cargo run`.
#
# Использование (из корня репозитория):
#   powershell -File tools/run_backend.ps1

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
