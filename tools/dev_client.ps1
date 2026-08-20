<#
.SYNOPSIS
    Режим 2: собрать фронт и довести его до живого окна десктоп-клиента.

.DESCRIPTION
    Клиент — F:\dev\sdd_desktop (Tauri v2). Он не бандлит фронт: главное окно грузит
    выбранный бэкенд, а бэкенд раздаёт `dist/`. Значит после `trunk build` клиенту
    нужна ровно одна вещь — перезагрузка страницы.

    Управление идёт по Chrome DevTools Protocol: WebView2 — это Chromium, и он
    открывает порт отладки по переменной WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS.
    Правок в самом sdd_desktop это не требует, и граница доверия оболочки не
    затрагивается: команды идут снаружи внутрь страницы, IPC главному окну не выдаётся.

    Окно ПЕРЕЗАГРУЖАЕТСЯ, а не перезапускается: Page.reload сохраняет маршрут SPA и
    геометрию окна. Перезапуск процесса (-Restart) нужен, только когда менялась сама
    оболочка или порт отладки закрыт.

    Заодно снимается то, ради чего этот канал и нужен: ошибки консоли (включая паники
    WASM), размер heap, число узлов DOM и время загрузки. Полный отчёт —
    target/build_log/client_last.json.

.PARAMETER NoBuild
    Не собирать фронт, только обновить клиента.

.PARAMETER Restart
    Перезапустить процесс клиента, а не перезагружать окно.

.PARAMETER Wait
    Сколько секунд собирать события консоли после перезагрузки (по умолчанию 8).

.NOTES
    Отладочный порт слушает только 127.0.0.1 и включается только этим скриптом:
    он даёт полный контроль над сессией с живым JWT, в релизе его быть не должно.

    Из корня репозитория:
        powershell -File tools/dev_client.ps1
        powershell -File tools/dev_client.ps1 -NoBuild
#>

param(
    [switch]$NoBuild,
    [switch]$Restart,
    [int]$Wait = 8,
    [int]$Port = 9222
)

$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent

$ClientExe   = 'F:\dev\sdd_desktop\src-tauri\target\debug\sdd_desktop.exe'
$ProfilePath = Join-Path $env:APPDATA 'com.sdd.desktop\profiles.json'

# --- CDP поверх WebSocket ----------------------------------------------------
# ClientWebSocket есть в .NET 4.5, то есть в PS 5.1 без единой зависимости.
# HTTP-эндпоинты /json/* умеют только перечислять цели; navigate/reload/evaluate
# живут исключительно в WebSocket-протоколе.

function Connect-Cdp([string]$Url) {
    $sock = New-Object System.Net.WebSockets.ClientWebSocket
    $cts  = New-Object System.Threading.CancellationTokenSource 10000
    $sock.ConnectAsync([Uri]$Url, $cts.Token).Wait()
    return $sock
}

function Send-Cdp($Socket, [int]$Id, [string]$Method, $Params) {
    $msg = @{ id = $Id; method = $Method }
    if ($Params) { $msg.params = $Params }
    $bytes = [Text.Encoding]::UTF8.GetBytes(($msg | ConvertTo-Json -Depth 8 -Compress))
    $seg = New-Object System.ArraySegment[byte] (, $bytes)
    $Socket.SendAsync($seg, [System.Net.WebSockets.WebSocketMessageType]::Text, $true,
                      [Threading.CancellationToken]::None).Wait()
}

# Один приём в полёте за раз, буфер общий. Начатую ReceiveAsync НЕЛЬЗЯ отменять по
# таймауту: отмена в WebSocket не мягкая, она обрывает сокет, и следующий Send падает
# с AggregateException. Поэтому недочитанная задача просто переживает таймаут и
# дочитывается на следующем вызове.
$script:CdpPending = $null
$script:CdpBuf     = New-Object byte[] 65536
$script:CdpSeg     = New-Object System.ArraySegment[byte] (, $script:CdpBuf)

function Receive-Cdp($Socket, [int]$TimeoutMs) {
    # $null = таймаут. Сообщение может приехать несколькими фреймами (дампы большие),
    # поэтому собираем до EndOfMessage.
    if (-not $script:CdpPending) {
        $script:CdpPending = $Socket.ReceiveAsync($script:CdpSeg, [Threading.CancellationToken]::None)
    }
    if (-not $script:CdpPending.Wait($TimeoutMs)) { return $null }
    $res = $script:CdpPending.Result
    $script:CdpPending = $null

    $sb = New-Object Text.StringBuilder
    $null = $sb.Append([Text.Encoding]::UTF8.GetString($script:CdpBuf, 0, $res.Count))
    while (-not $res.EndOfMessage) {
        $task = $Socket.ReceiveAsync($script:CdpSeg, [Threading.CancellationToken]::None)
        if (-not $task.Wait(15000)) { break }
        $res = $task.Result
        $null = $sb.Append([Text.Encoding]::UTF8.GetString($script:CdpBuf, 0, $res.Count))
    }
    return ($sb.ToString() | ConvertFrom-Json)
}

function Invoke-Cdp($Socket, [int]$Id, [string]$Method, $Params, [ref]$Events, [int]$TimeoutMs = 15000) {
    # Ответ на команду приходит вперемешку с событиями — события не выбрасываем,
    # они и есть полезная нагрузка (ошибки консоли).
    Send-Cdp $Socket $Id $Method $Params
    $deadline = (Get-Date).AddMilliseconds($TimeoutMs)
    while ((Get-Date) -lt $deadline) {
        $m = Receive-Cdp $Socket 3000
        if ($null -eq $m) { continue }
        if ($m.id -eq $Id) { return $m }
        if ($m.method -and $Events) { $null = $Events.Value.Add($m) }
    }
    return $null
}

# --- 1. Сборка ---------------------------------------------------------------
if (-not $NoBuild) {
    & (Join-Path $PSScriptRoot 'build_frontend.ps1')
}

# --- 2. Куда смотрит клиент --------------------------------------------------
if (-not (Test-Path $ProfilePath)) { throw "Нет профилей клиента: $ProfilePath" }
$profiles = Get-Content $ProfilePath -Raw -Encoding UTF8 | ConvertFrom-Json
$active   = $profiles.profiles | Where-Object { $_.id -eq $profiles.active_profile_id } | Select-Object -First 1
if (-not $active) { throw "В $ProfilePath не выбран активный профиль" }
$baseUrl = $active.base_url.TrimEnd('/')
Write-Host "Профиль клиента: $($active.name) -> $baseUrl"

try {
    $null = Invoke-WebRequest -Uri "$baseUrl/" -TimeoutSec 5 -UseBasicParsing
} catch {
    Write-Host "Бэкенд на $baseUrl не отвечает — запусти tools/run_backend.ps1" -ForegroundColor Yellow
}

# --- 3. Клиент с открытым портом отладки -------------------------------------
function Test-CdpPort([int]$P) {
    try { $null = Invoke-RestMethod "http://127.0.0.1:$P/json/version" -TimeoutSec 2; return $true }
    catch { return $false }
}

$running = @(Get-Process sdd_desktop -ErrorAction SilentlyContinue)
$hasCdp  = Test-CdpPort $Port

if ($Restart -or ($running.Count -gt 0 -and -not $hasCdp)) {
    # Порт отладки задаётся только при старте процесса: клиент, поднятый вручную,
    # управляться не может — его надо перезапустить.
    foreach ($p in $running) { Stop-Process -Id $p.Id -Force }
    Start-Sleep -Milliseconds 700
    $running = @()
}

if ($running.Count -eq 0) {
    if (-not (Test-Path $ClientExe)) { throw "Клиент не собран: $ClientExe" }
    $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=$Port"
    Start-Process -FilePath $ClientExe
    for ($i = 0; $i -lt 20 -and -not (Test-CdpPort $Port); $i++) { Start-Sleep -Milliseconds 500 }
    if (-not (Test-CdpPort $Port)) { throw "Клиент запущен, но порт отладки $Port не открылся" }
    Write-Host "Клиент запущен, порт отладки $Port"
} else {
    Write-Host "Клиент уже работает (PID $($running[0].Id)), порт отладки $Port"
}

# --- 4. Главное окно ---------------------------------------------------------
# Локальное окно «Подключения» живёт на tauri.localhost и нам не интересно.
$targets = Invoke-RestMethod "http://127.0.0.1:$Port/json/list" -TimeoutSec 5
$main = $targets | Where-Object { $_.type -eq 'page' -and $_.url -notlike 'http://tauri.localhost*' } | Select-Object -First 1
if (-not $main) { throw "Главное окно клиента не найдено среди целей CDP" }

$sock   = Connect-Cdp $main.webSocketDebuggerUrl
$events = New-Object System.Collections.ArrayList
$id     = 0
$metrics = $null

try {
    $null = Invoke-Cdp $sock (++$id) 'Runtime.enable' $null ([ref]$events)
    $null = Invoke-Cdp $sock (++$id) 'Log.enable'     $null ([ref]$events)
    $null = Invoke-Cdp $sock (++$id) 'Page.enable'    $null ([ref]$events)

    if ($main.url -like "$baseUrl*") {
        $null = Invoke-Cdp $sock (++$id) 'Page.reload' @{ ignoreCache = $true } ([ref]$events)
        Write-Host "Окно перезагружено ($($main.url))"
    } else {
        $null = Invoke-Cdp $sock (++$id) 'Page.navigate' @{ url = "$baseUrl/" } ([ref]$events)
        Write-Host "Окно переведено на $baseUrl/ (было: $($main.url))"
    }

    # Собираем события, пока страница поднимается.
    $deadline = (Get-Date).AddSeconds($Wait)
    while ((Get-Date) -lt $deadline) {
        $m = Receive-Cdp $sock 1000
        if ($m -and $m.method) { $null = $events.Add($m) }
    }

    $js = @'
JSON.stringify({
  url: location.href,
  nodes: document.getElementsByTagName('*').length,
  heap_mb: performance.memory ? +(performance.memory.usedJSHeapSize/1048576).toFixed(1) : null,
  load_ms: (function(){ var n = performance.getEntriesByType('navigation')[0]; return n ? Math.round(n.duration) : null; })(),
  title: document.title
})
'@
    $r = Invoke-Cdp $sock (++$id) 'Runtime.evaluate' @{ expression = $js; returnByValue = $true } ([ref]$events)
    if ($r -and $r.result.result.value) { $metrics = $r.result.result.value | ConvertFrom-Json }
}
finally {
    try { $sock.Abort() } catch { }
    $sock.Dispose()
}

# --- 5. Отчёт ----------------------------------------------------------------
$errors = @()
foreach ($e in $events) {
    switch ($e.method) {
        'Runtime.exceptionThrown' {
            $d = $e.params.exceptionDetails
            $errors += [pscustomobject]@{ kind = 'exception'; text = "$($d.text) $($d.exception.description)" }
        }
        'Runtime.consoleAPICalled' {
            if ($e.params.type -in @('error', 'warning')) {
                $errors += [pscustomobject]@{ kind = $e.params.type; text = (($e.params.args | ForEach-Object { $_.value }) -join ' ') }
            }
        }
        'Log.entryAdded' {
            if ($e.params.entry.level -in @('error', 'warning')) {
                $errors += [pscustomobject]@{ kind = $e.params.entry.level; text = $e.params.entry.text }
            }
        }
    }
}

if ($metrics) {
    Write-Host ("Страница: {0}   DOM {1} узлов   heap {2} МБ   загрузка {3} мс" -f $metrics.title, $metrics.nodes, $metrics.heap_mb, $metrics.load_ms) -ForegroundColor Green
}
if ($errors.Count -eq 0) {
    Write-Host "Консоль чистая" -ForegroundColor Green
} else {
    Write-Host "Ошибок и предупреждений в консоли: $($errors.Count)" -ForegroundColor Yellow
    $errors | Select-Object -First 10 | ForEach-Object { Write-Host ("  [{0}] {1}" -f $_.kind, $_.text) }
}

$logDir = Join-Path $root 'target\build_log'
if (-not (Test-Path $logDir)) { $null = New-Item -ItemType Directory -Path $logDir -Force }
[pscustomobject]@{
    at      = (Get-Date -Format 'o')
    base    = $baseUrl
    metrics = $metrics
    errors  = $errors
} | ConvertTo-Json -Depth 6 | Set-Content (Join-Path $logDir 'client_last.json') -Encoding utf8
