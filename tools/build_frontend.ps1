<#
.SYNOPSIS
    Сборка фронта одной командой. Основной режим разработки фронта.

.DESCRIPTION
    Вотчер (`trunk serve`) выведен из цикла: правки, приходящие во время сборки,
    он не отбрасывал, а ставил следующую сборку в очередь — пачка из пяти файлов
    превращалась в пять полных прогонов. Теперь сборка запускается явно, один раз
    на пачку правок.

    Результат кладётся в `dist/`, который раздаёт бэкенд на :3000 (ServeDir::new("dist")),
    поэтому отдельный dev-сервер на :8080 не нужен: в dev и в проде фронт живёт на
    одном origin, как и в Tauri-клиенте.

    Профиль сборки НЕ передаётся флагом — он задан в Trunk.toml (`cargo_profile =
    "wasm-dev"`). Замерено: профиль `dev` давал wasm на 197 МБ и 11,7 с постобработки
    на пустой пересборке, `wasm-dev` — 66 МБ и 4,7 с.

.PARAMETER Client
    После сборки поднять/обновить десктоп-клиент (tools/dev_client.ps1).

.PARAMETER CssOnly
    Скопировать только `static/` в `dist/` и выйти, не трогая cargo. Правки
    per-page CSS не меняют wasm, а полная сборка стоит секунды впустую.

.PARAMETER Release
    Релизная сборка (`trunk build --cargo-profile release`). Флаг `--release` здесь
    НЕ подходит: `cargo_profile` из Trunk.toml сильнее него, и `trunk build --release`
    молча собрал бы wasm-dev.

.NOTES
    Из корня репозитория:
        powershell -File tools/build_frontend.ps1
        powershell -File tools/build_frontend.ps1 -CssOnly
        powershell -File tools/build_frontend.ps1 -Client
#>

param(
    [switch]$Client,
    [switch]$CssOnly,
    [switch]$Release
)

$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
Push-Location $root

try {
    # trunk 0.21 читает NO_COLOR как булево и падает на распространённом NO_COLOR=1
    # ("error: invalid value '1' for '--no-color'"). Приводим к тому, что он понимает.
    if ($env:NO_COLOR -and $env:NO_COLOR -notin @('true', 'false')) { $env:NO_COLOR = 'true' }
    $env:TRUNK_SKIP_VERSION_CHECK = 'true'

    # Вотчер и явная сборка спорят за один каталог сборки — предупреждаем, но не убиваем.
    $serve = Get-CimInstance Win32_Process -Filter "name='trunk.exe'" -ErrorAction SilentlyContinue |
             Where-Object { $_.CommandLine -match '\bserve\b' }
    if ($serve) {
        Write-Host "Внимание: запущен trunk serve (PID $($serve.ProcessId)) — он будет собирать параллельно." -ForegroundColor Yellow
    }

    $sw = [Diagnostics.Stopwatch]::StartNew()
    $mode = 'build'

    if ($CssOnly) {
        $mode = 'css'
        # /MIR: dist/static — производная копия crates/frontend/static, лишние файлы в ней
        # означают переименование, а не ценные данные. Коды возврата 0..7 у robocopy — успех.
        $null = robocopy 'crates\frontend\static' 'dist\static' /MIR /NJH /NJS /NP /NDL /NFL
        if ($LASTEXITCODE -ge 8) { throw "robocopy завершился с кодом $LASTEXITCODE" }
        $global:LASTEXITCODE = 0
    }
    else {
        # --cargo-profile, а не --release: ключ cargo_profile в Trunk.toml перебивает
        # флаг --release, и релиз собрался бы профилем wasm-dev.
        if ($Release) { $mode = 'release'; & trunk build --cargo-profile release } else { & trunk build }
        if ($LASTEXITCODE -ne 0) { throw "trunk build завершился с кодом $LASTEXITCODE" }
    }

    $sw.Stop()
    $sec = [math]::Round($sw.Elapsed.TotalSeconds, 1)

    $wasm = Get-ChildItem (Join-Path $root 'dist') -Filter *.wasm -ErrorAction SilentlyContinue |
            Select-Object -First 1
    $mb = if ($wasm) { [math]::Round($wasm.Length / 1MB, 1) } else { 0 }

    Write-Host ("Готово за {0} с   ({1}, wasm {2} МБ)" -f $sec, $mode, $mb) -ForegroundColor Green

    # Каждая сборка — замер. Лог лежит в target/ (не в репозитории): он нужен, чтобы
    # видеть распределение реальных сборок, а не один синтетический прогон.
    $logDir = Join-Path $root 'target\build_log'
    if (-not (Test-Path $logDir)) { $null = New-Item -ItemType Directory -Path $logDir -Force }
    $line = "{0}`t{1}`t{2}`t{3}" -f (Get-Date -Format 'o'), $mode, $sec, $mb
    Add-Content -Path (Join-Path $logDir 'frontend.tsv') -Value $line -Encoding utf8

    if ($Client) {
        & (Join-Path $PSScriptRoot 'dev_client.ps1') -NoBuild
    }
}
finally {
    Pop-Location
}
