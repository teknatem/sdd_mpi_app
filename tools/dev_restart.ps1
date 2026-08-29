<#
.SYNOPSIS
    Полный цикл dev-перезапуска: пересобрать фронт, собрать бэкенд, поднять его заново.

.DESCRIPTION
    Один вызов вместо трёх ручных шагов (`build_frontend.ps1` → снять backend.exe →
    `cargo run -p backend`). Порядок шагов не произвольный:

      1. Фронт собирается ПЕРВЫМ, пока старый бэкенд ещё жив и раздаёт прежний `dist/`.
      2. Бэкенд снимается ДО `cargo build`: запущенный процесс держит
         `target\debug\backend.exe`, и линковка падает с "Access is denied (os error 5)".
      3. Сборка идёт через `cargo build --bin backend`, а не `cargo run`: артефакт тот
         же самый (`target\debug\backend.exe`), но запуск отделён от сборки —
         бэкенд поднимается в СВОЁМ окне консоли с живым логом запросов, а эта
         консоль освобождается сразу. Другие режимы — -Foreground и -Detached.

    Сборка фронта не дублируется, а делегируется `build_frontend.ps1` (там живут
    NO_COLOR-обход, профиль из Trunk.toml и лог замеров).

    Готовность проверяется TCP-коннектом на 127.0.0.1:3000: HTTP-эндпоинта здоровья
    у бэкенда нет, а «порт принял соединение» и означает, что axum уже слушает.

.PARAMETER CssOnly
    Фронт — только копированием `static/` в `dist/` (0 с, без cargo). Бэкенд
    пересобирается и перезапускается как обычно.

.PARAMETER SkipFrontend
    Не трогать фронт вообще: только пересобрать и перезапустить бэкенд.

.PARAMETER Foreground
    Запустить `cargo run -p backend` прямо в этой консоли (Ctrl+C останавливает,
    скрипт не возвращает управление, пока бэкенд жив).

.PARAMETER Detached
    Служебный режим: без окна, вывод в `target\build_log\backend_out.log`.
    Такой процесс переживает закрытие этой консоли — его некому послать
    CTRL_CLOSE. Остановить: `Stop-Process -Name backend` или прогон с -NoStart.

.PARAMETER NoStart
    Собрать всё и остановить старый процесс, но новый не поднимать.

.PARAMETER Client
    После перезапуска обновить окно десктоп-клиента (`dev_client.ps1 -NoBuild`).

.PARAMETER TimeoutSec
    Сколько ждать, пока порт 3000 начнёт принимать соединения (по умолчанию 90).
    Первый старт после миграций бывает долгим.

.NOTES
    Из корня репозитория:
        powershell -File tools/dev_restart.ps1
        powershell -File tools/dev_restart.ps1 -CssOnly
        powershell -File tools/dev_restart.ps1 -SkipFrontend -Detached
#>

param(
    [switch]$CssOnly,
    [switch]$SkipFrontend,
    [switch]$Foreground,
    [switch]$Detached,
    [switch]$NoStart,
    [switch]$Client,
    [int]$Port = 3000,
    [int]$TimeoutSec = 90
)

$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
Push-Location $root

function Write-Step([string]$Text) {
    Write-Host ""
    Write-Host "==> $Text" -ForegroundColor Cyan
}

function Test-Port([int]$P) {
    $client = New-Object Net.Sockets.TcpClient
    try {
        $async = $client.BeginConnect('127.0.0.1', $P, $null, $null)
        if (-not $async.AsyncWaitHandle.WaitOne(300)) { return $false }
        $client.EndConnect($async)
        return $true
    } catch { return $false } finally { $client.Close() }
}

$total = [Diagnostics.Stopwatch]::StartNew()

try {
    # --- 1. Фронт ------------------------------------------------------------
    if ($SkipFrontend) {
        Write-Step "Фронт пропущен (-SkipFrontend)"
    }
    else {
        Write-Step ("Фронт: " + $(if ($CssOnly) { "только static/ -> dist/" } else { "trunk build" }))
        if ($CssOnly) { & (Join-Path $PSScriptRoot 'build_frontend.ps1') -CssOnly }
        else          { & (Join-Path $PSScriptRoot 'build_frontend.ps1') }
        if ($LASTEXITCODE -ne 0) { throw "сборка фронта завершилась с кодом $LASTEXITCODE" }
    }

    # --- 2. Снять старый бэкенд ---------------------------------------------
    # До сборки, а не после: живой процесс держит exe, и линковка упадёт.
    Write-Step "Останавливаю backend.exe"
    $procs = @(Get-Process backend -ErrorAction SilentlyContinue)
    if ($procs.Count -eq 0) {
        Write-Host "    не запущен"
    }
    else {
        foreach ($p in $procs) {
            Write-Host ("    PID {0}, работал с {1}" -f $p.Id, $p.StartTime)
            Stop-Process -Id $p.Id -Force
        }
        # Windows освобождает файловый лок не в момент Stop-Process, а когда
        # ядро закроет хендлы процесса. Ждём этого, иначе линкер поймает гонку.
        foreach ($p in $procs) { $null = $p.WaitForExit(10000) }
        Start-Sleep -Milliseconds 300
    }

    # --- 3. Сборка бэкенда ---------------------------------------------------
    Write-Step "Сборка бэкенда (cargo build -p backend --bin backend)"
    $sw = [Diagnostics.Stopwatch]::StartNew()
    & cargo build -p backend --bin backend
    if ($LASTEXITCODE -ne 0) { throw "cargo build завершился с кодом $LASTEXITCODE" }
    $sw.Stop()
    Write-Host ("    собрано за {0} с" -f [math]::Round($sw.Elapsed.TotalSeconds, 1)) -ForegroundColor Green

    if ($NoStart) {
        Write-Step "Запуск пропущен (-NoStart)"
        return
    }

    # --- 4. Запуск -----------------------------------------------------------
    if ($Foreground) {
        Write-Step "Запуск в этой консоли (Ctrl+C — остановить)"
        & cargo run -p backend
        return
    }

    $exe = Join-Path $root 'target\debug\backend.exe'
    if (-not (Test-Path $exe)) { throw "не найден $exe" }

    # Свой лог бэкенда (tracing) пишется рядом с бинарником в любом режиме —
    # именно по нему разбирают падение, когда консоль закрылась вместе с ним.
    $traceLog = Join-Path $root 'target\debug\logs\backend.log'
    $tailLogs = @($traceLog)

    if ($Detached) {
        # Служебный режим. WindowStyle Hidden — не косметика: backend.exe
        # консольный, и без него Windows открывает ему окно, пустое (весь вывод
        # уехал в файлы) и смертельное — его закрытие шлёт CTRL_CLOSE и убивает
        # бэкенд. Скрытый процесс живёт сам и переживает закрытие этой консоли.
        # Потоки разводятся по разным файлам: один и тот же Start-Process не примет.
        $logDir = Join-Path $root 'target\build_log'
        if (-not (Test-Path $logDir)) { $null = New-Item -ItemType Directory -Path $logDir -Force }
        $outLog = Join-Path $logDir 'backend_out.log'
        $errLog = Join-Path $logDir 'backend_err.log'

        Write-Step "Запуск backend.exe (фоном, без окна)"
        $proc = Start-Process -FilePath $exe -WorkingDirectory $root -PassThru -WindowStyle Hidden `
                              -RedirectStandardOutput $outLog -RedirectStandardError $errLog
        $tailLogs = @($errLog, $outLog, $traceLog)
        $where = "вывод: $outLog"
    }
    else {
        # По умолчанию — своё консольное окно с живым логом запросов: ради него
        # dev-бэкенд обычно и держат на виду. Перенаправлять потоки здесь нельзя,
        # иначе окно окажется пустым. Плата за живой лог: окно владеет процессом —
        # закроешь окно, остановишь бэкенд (нужно обратное — флаг -Detached).
        Write-Step "Запуск backend.exe (в отдельном окне консоли)"
        $proc = Start-Process -FilePath $exe -WorkingDirectory $root -PassThru
        $where = "лог — в открывшейся консоли, копия: $traceLog"
    }

    # WorkingDirectory = корень: бэкенд читает config.toml относительным путём.
    $wait = [Diagnostics.Stopwatch]::StartNew()
    $ready = $false
    while ($wait.Elapsed.TotalSeconds -lt $TimeoutSec) {
        if ($proc.HasExited) { break }
        if (Test-Port $Port) { $ready = $true; break }
        Start-Sleep -Milliseconds 400
    }
    $wait.Stop()

    if (-not $ready) {
        Write-Host ""
        if ($proc.HasExited) { Write-Host ("backend.exe упал (код {0})" -f $proc.ExitCode) -ForegroundColor Red }
        else                 { Write-Host ("порт {0} не ответил за {1} с" -f $Port, $TimeoutSec) -ForegroundColor Red }
        foreach ($log in $tailLogs) {
            if ((Test-Path $log) -and (Get-Item $log).Length -gt 0) {
                Write-Host "--- $log (хвост) ---" -ForegroundColor Yellow
                Get-Content $log -Tail 25
            }
        }
        throw "бэкенд не поднялся"
    }

    $sec = [math]::Round($wait.Elapsed.TotalSeconds, 1)
    Write-Host ("    PID {0}, порт {1} отвечает через {2} с" -f $proc.Id, $Port, $sec) -ForegroundColor Green
    Write-Host "    $where"
    Write-Host "    http://localhost:$Port"

    if ($Client) { & (Join-Path $PSScriptRoot 'dev_client.ps1') -NoBuild }
}
finally {
    $total.Stop()
    Write-Host ""
    Write-Host ("Всего {0} с" -f [math]::Round($total.Elapsed.TotalSeconds, 1)) -ForegroundColor Green
    Pop-Location
}
