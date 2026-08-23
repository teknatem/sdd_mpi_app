<#
.SYNOPSIS
    Regenerates ARCHITECTURE.md from code (source of truth).

.DESCRIPTION
    Builds the project map straight from sources, without compiling and without a DB:
      - Aggregates a0XX    -> crates/contracts/src/domain/<a0XX>/metadata.json (+ dir names)
      - Projections p9XX   -> dir names crates/backend/src/projections/
      - Use-cases u5XX     -> dir names crates/backend/src/usecases/
      - Data schemes dsXX  -> dir names crates/backend/src/data_schemes/
      - DataView dvXXX     -> dir names + //! header crates/backend/src/data_view/
      - Dashboards d4XX    -> dir names, union of backend/ and frontend/ src/dashboards/
                              (most dashboards are frontend-only)
      - Quality checks     -> file names + //! header crates/backend/src/quality/checks/
      - Tasks task0XX      -> file names crates/backend/src/system/tasks/managers/
      - Mechanisms         -> static definitions (Processes, Stages, Actions, Plugins);
                              their INSTANCES live in the DB and are generated into the
                              instance knowledge base, not here
      - Actions            -> ActionInfo in backend/src/processes/actions/*.rs
      - Chart of accounts  -> ACCOUNT_REGISTRY (shared/analytics/account_registry.rs)
      - Turnover classes   -> TURNOVER_CLASSES (shared/analytics/turnover_registry.rs)
      - API routes         -> .route(...) in api/routes.rs
      - UI scopes          -> SCOPE_CATALOG (system/access/scope_catalog.rs)
      - UI sidebar/tabs    -> frontend layout/left/sidebar.rs, layout/tabs/*.rs

    The "Docs" column marks objects that carry a hand-written llm.md next to the
    code: the map says what exists, llm.md says why it behaves the way it does.

.NOTES
    Run from the repo root:
        powershell -File tools/gen_architecture.ps1
    ARCHITECTURE.md is GENERATED. Edit this script, not the output.
#>

$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$out  = New-Object System.Text.StringBuilder
$BT   = [char]96   # backtick, to wrap code spans without in-string escaping

function W([string]$s = '') { [void]$out.AppendLine($s) }
function Q([string]$s)      { return "$BT$s$BT" }   # `value`

# code = "p904", rest = "sales_data" -> "sales data"
function Split-Code([string]$name) {
    if ($name -match '^([a-z]+\d+)[_-](.+)$') {
        [pscustomobject]@{ Code = $Matches[1]; Label = ($Matches[2] -replace '_', ' ') }
    } else {
        [pscustomobject]@{ Code = $name; Label = '' }
    }
}

function Truncate([string]$s, [int]$n) {
    if ([string]::IsNullOrWhiteSpace($s)) { return '' }
    $s = ($s -replace '\s+', ' ').Trim()
    if ($s.Length -le $n) { return $s }
    return $s.Substring(0, $n).TrimEnd() + [char]0x2026
}

# Cell text must not break the markdown table it lives in.
function Cell([string]$s) {
    if ([string]::IsNullOrWhiteSpace($s)) { return '' }
    return ($s -replace '\|', '\|')
}

$CHECK = [char]0x2713

# Module headline: first '//!' line of the file, stripped of markdown heading marks.
function Get-DocLine([string]$file) {
    if (-not (Test-Path $file)) { return '' }
    foreach ($line in (Get-Content $file -Encoding UTF8)) {
        $t = $line.Trim()
        if ($t -notmatch '^//!') { if ($t -eq '') { continue } else { break } }
        $t = ($t -replace '^//!\s*', '') -replace '^#+\s*', ''
        if ($t -ne '') { return $t }
    }
    return ''
}

# 'Docs' marker: a hand-written llm.md living next to the code.
function Has-LlmDoc([string]$dir) {
    if (Test-Path (Join-Path $dir 'llm.md')) { return $CHECK }
    return ''
}

# Enumerate a layer's items by prefix and emit a Code | Name table
function Add-Catalog([string]$relDir, [string]$prefix, [string]$title, [bool]$filesNotDirs = $false, [bool]$withDocs = $false) {
    $path = Join-Path $root $relDir
    if (-not (Test-Path $path)) { return }
    $items = if ($filesNotDirs) {
        Get-ChildItem $path -File -Filter "$prefix*.rs" | Where-Object { $_.BaseName -ne 'mod' }
    } else {
        Get-ChildItem $path -Directory | Where-Object { $_.Name -match "^$prefix\d" }
    }
    if (-not $items) { return }
    W "## $title"
    W ''
    if ($withDocs) { W '| Code | Name | Docs |'; W '|------|------|------|' }
    else           { W '| Code | Name |';        W '|------|------|' }
    foreach ($it in ($items | Sort-Object Name)) {
        $n = if ($filesNotDirs) { $it.BaseName } else { $it.Name }
        $sc = Split-Code $n
        if ($withDocs) {
            W "| $(Q $sc.Code) | $($sc.Label) | $(Has-LlmDoc $it.FullName) |"
        } else {
            W "| $(Q $sc.Code) | $($sc.Label) |"
        }
    }
    W ''
}

# Dashboards live mostly in the FRONTEND (only d400 has a backend half), so a single
# directory scan would miss most of them. Union both sides and show where each half is.
function Add-DashboardCatalog([string]$title) {
    $sides = @{
        Backend  = Join-Path $root 'crates/backend/src/dashboards'
        Frontend = Join-Path $root 'crates/frontend/src/dashboards'
    }
    $found = @{}   # code -> @{ Label; Backend; Frontend }
    foreach ($side in $sides.Keys) {
        if (-not (Test-Path $sides[$side])) { continue }
        Get-ChildItem $sides[$side] -Directory |
            Where-Object { $_.Name -match '^d\d' } |
            ForEach-Object {
                $sc = Split-Code $_.Name
                if (-not $found.ContainsKey($sc.Code)) {
                    $found[$sc.Code] = @{ Label = $sc.Label; Backend = ''; Frontend = '' }
                }
                # Frontend label wins: the UI half carries the name users actually see.
                if ($side -eq 'Frontend') { $found[$sc.Code].Label = $sc.Label }
                $found[$sc.Code][$side] = '+'
            }
    }
    if ($found.Count -eq 0) { return }
    W "## $title"
    W ''
    W '| Code | Name | Backend | Frontend |'
    W '|------|------|---------|----------|'
    foreach ($code in ($found.Keys | Sort-Object)) {
        $e = $found[$code]
        W "| $(Q $code) | $($e.Label) | $($e.Backend) | $($e.Frontend) |"
    }
    W ''
}

# Like Add-Catalog, but the name comes from the module's own '//!' headline.
# Used for layers whose directory/file names carry no readable label.
function Add-DocCatalog([string]$relDir, [string]$title, [bool]$filesNotDirs = $false, [string[]]$skip = @()) {
    $path = Join-Path $root $relDir
    if (-not (Test-Path $path)) { return }
    $items = if ($filesNotDirs) {
        Get-ChildItem $path -File -Filter '*.rs' | Where-Object { $skip -notcontains $_.BaseName }
    } else {
        Get-ChildItem $path -Directory | Where-Object { $skip -notcontains $_.Name }
    }
    if (-not $items) { return }
    W "## $title"
    W ''
    # Files carry no llm.md next to them, so the Docs column only fits directories.
    if ($filesNotDirs) { W '| Code | Description |';        W '|------|-------------|' }
    else               { W '| Code | Description | Docs |'; W '|------|-------------|------|' }
    foreach ($it in ($items | Sort-Object Name)) {
        if ($filesNotDirs) {
            $name = $it.BaseName
            $desc = Get-DocLine $it.FullName
        } else {
            $name = $it.Name
            $desc = Get-DocLine (Join-Path $it.FullName 'mod.rs')
        }
        # The headline usually repeats the code ("dv001 - DataView: ..."): drop it.
        $desc = $desc -replace "^$name\s*[-$([char]0x2014):]\s*", ''
        if ($filesNotDirs) {
            W "| $(Q $name) | $(Cell (Truncate $desc 140)) |"
        } else {
            W "| $(Q $name) | $(Cell (Truncate $desc 140)) | $(Has-LlmDoc $it.FullName) |"
        }
    }
    W ''
}

W '# ARCHITECTURE'
W ''
W "> **GENERATED file - do not edit by hand.** Source of truth is the code."
W "> Regenerate: $(Q 'powershell -File tools/gen_architecture.ps1')"
W '> Project object map (aggregates, projections, use-cases, chart of accounts, turnovers, API).'
W ''

# ----- Mechanisms -----
# Проза о механизмах живёт ОДНИМ файлом — статьёй базы знаний
# `src/shared/llm/docs/mechanisms.md`. Здесь она только инлайнится: раньше текст
# был захардкожен в этом скрипте, то есть правился в PowerShell, хотя выходной
# файл помечен «GENERATED — do not edit by hand». Берём тело статьи до маркера
# `<!-- architecture:end -->`: хвост за маркером адресован LLM-чату и карте не нужен.
$mechFile = Join-Path $root 'crates/backend/src/shared/llm/docs/mechanisms.md'
W '## Mechanisms'
W ''
if (Test-Path $mechFile) {
    $mechLines = Get-Content -Encoding UTF8 $mechFile
    # Отрезаем frontmatter: он ограничен строками '---' в начале файла.
    $from = 0
    if ($mechLines.Count -gt 0 -and $mechLines[0].Trim() -eq '---') {
        for ($i = 1; $i -lt $mechLines.Count; $i++) {
            if ($mechLines[$i].Trim() -eq '---') { $from = $i + 1; break }
        }
    }
    $body = @()
    for ($i = $from; $i -lt $mechLines.Count; $i++) {
        $line = $mechLines[$i]
        # Хвост за маркером адресован чату (id карт для get_knowledge) — в карту не идёт.
        if ($line -match 'architecture:end') { break }
        # H1 статьи заменён заголовком раздела; вложенные опускаем на уровень.
        if ($line -match '^#\s')  { continue }
        if ($line -match '^##\s') { $line = '#' + $line }
        $body += $line
    }
    W (($body -join [Environment]::NewLine).Trim())
    W ''
} else {
    W "_Источник ``crates/backend/src/shared/llm/docs/mechanisms.md`` не найден._"
    W ''
}

# ----- Actions -----
# Паспорта Действий заданы в Rust (ActionInfo), поэтому каталог выводится из кода.
$actionsDir = Join-Path $root 'crates/backend/src/processes/actions'
if (Test-Path $actionsDir) {
    $rows = @()
    foreach ($file in Get-ChildItem -Path $actionsDir -Filter '*.rs' | Where-Object { $_.BaseName -ne 'mod' } | Sort-Object Name) {
        $text = Get-Content -Raw -Encoding UTF8 $file.FullName
        $name = [regex]::Match($text, 'name:\s*"([^"]+)"').Groups[1].Value
        if (-not $name) { continue }
        $rows += [pscustomobject]@{
            Name       = $name
            Method     = [regex]::Match($text, 'method:\s*"([^"]+)"').Groups[1].Value
            Title      = [regex]::Match($text, 'title:\s*"([^"]+)"').Groups[1].Value
            Reversible = [regex]::Match($text, 'reversible:\s*(true|false)').Groups[1].Value
            Writes     = ([regex]::Matches(
                            [regex]::Match($text, 'write_tables:\s*&\[([^\]]*)\]').Groups[1].Value,
                            '"([^"]+)"') | ForEach-Object { $_.Groups[1].Value }) -join ', '
        }
    }
    W "## Actions ($($rows.Count))"
    W ''
    W ("Операции ядра с побочным эффектом. В mjs Этапа — $(Q 'host.actions.<method>'), право — " +
       "$(Q 'action:<name>') в манифесте Этапа. В LLM-чате те же записи подаются как инструменты.")
    W ''
    W '| Name | host.actions | Title | Reversible | Writes |'
    W '|------|--------------|-------|------------|--------|'
    foreach ($r in $rows) {
        $writes = if ($r.Writes) { (($r.Writes -split ', ') | ForEach-Object { Q $_ }) -join ', ' } else { '' }
        W "| $(Q $r.Name) | $(Q $r.Method) | $(Cell $r.Title) | $($r.Reversible) | $writes |"
    }
    W ''
}

# ----- Aggregates a0XX -----
$domainDir = Join-Path $root 'crates/contracts/src/domain'
$aggDirs = Get-ChildItem (Join-Path $root 'crates/backend/src/domain') -Directory |
    Where-Object { $_.Name -match '^a\d' } | Sort-Object Name
# entity_index -> list_name, reused by the UI section to resolve tab labels that
# are written as `A012.ui.list_name` instead of a string literal.
$listNames = @{}
$elementNames = @{}
W '## Aggregates (a0XX)'
W ''
W '| Index | Entity | Table | Description | Related | Docs |'
W '|-------|--------|-------|-------------|---------|------|'
foreach ($d in $aggDirs) {
    $meta = Join-Path $domainDir (Join-Path $d.Name 'metadata.json')
    $sc = Split-Code $d.Name
    $docs = Has-LlmDoc $d.FullName
    if (Test-Path $meta) {
        $j = Get-Content $meta -Raw -Encoding UTF8 | ConvertFrom-Json
        $entity = if ($j.ui.element_name) { $j.ui.element_name } elseif ($j.entity_name) { $j.entity_name } else { $sc.Label }
        $table  = if ($j.table_name) { $j.table_name } else { '' }
        $desc   = Truncate $j.ai.description 140
        $rel    = if ($j.ai.related) { ($j.ai.related -join ', ') } else { '' }
        if ($j.entity_index -and $j.ui.list_name)    { $listNames[$j.entity_index]    = $j.ui.list_name }
        if ($j.entity_index -and $j.ui.element_name) { $elementNames[$j.entity_index] = $j.ui.element_name }
        W "| $(Q $j.entity_index) | $entity | $(Q $table) | $(Cell $desc) | $rel | $docs |"
    } else {
        W "| $(Q $sc.Code) | $($sc.Label) | | _(no metadata.json)_ | | $docs |"
    }
}
W ''

# ----- Name-based catalogs -----
Add-Catalog 'crates/backend/src/projections'           'p'    'Projections (p9XX)' $false $true
Add-Catalog 'crates/backend/src/usecases'              'u'    'Use-cases (u5XX)'   $false $true
Add-Catalog 'crates/backend/src/data_schemes'          'ds'   'Data schemes (dsXX)'
Add-Catalog 'crates/backend/src/system/tasks/managers' 'task' 'Scheduled tasks (task0XX)' $true
Add-DashboardCatalog 'Dashboards (d4XX)'

# ----- Headline-based catalogs -----
Add-DocCatalog 'crates/backend/src/data_view'     'DataView (dvXXX)'  $false @('filters', 'mod')
Add-DocCatalog 'crates/backend/src/quality/checks' 'Quality checks'   $true  @('mod', 'registrator_registry')

# ----- Chart of accounts -----
$accFile = Join-Path $root 'crates/backend/src/shared/analytics/account_registry.rs'
if (Test-Path $accFile) {
    $txt = Get-Content $accFile -Raw -Encoding UTF8
    $blocks = [regex]::Matches($txt, 'AccountDef\s*\{(.+?)\}', 'Singleline')
    if ($blocks.Count -gt 0) {
        W '## Chart of accounts (account_registry)'
        W ''
        W '| Account | Name | Parent | Section |'
        W '|---------|------|--------|---------|'
        foreach ($b in $blocks) {
            $body = $b.Groups[1].Value
            $code   = if ($body -match 'code:\s*"([^"]*)"')   { $Matches[1] } else { '' }
            $name   = if ($body -match 'name:\s*"([^"]*)"')   { $Matches[1] } else { '' }
            $parent = if ($body -match 'parent_code:\s*Some\("([^"]+)"\)') { $Matches[1] } else { '' }
            $sect   = if ($body -match 'section:\s*StatementSection::(\w+)') { $Matches[1] } else { '' }
            W "| $(Q $code) | $name | $parent | $sect |"
        }
        W ''
    }
}

# ----- Turnover classes -----
$turnFile = Join-Path $root 'crates/backend/src/shared/analytics/turnover_registry.rs'
if (Test-Path $turnFile) {
    $txt = Get-Content $turnFile -Raw -Encoding UTF8
    $blocks = [regex]::Matches($txt, 'TurnoverClassDef\s*\{(.+?)\}', 'Singleline')
    if ($blocks.Count -gt 0) {
        W '## Turnover classes (turnover_registry)'
        W ''
        W '| Code | Name | Debit | Credit | Entry |'
        W '|------|------|-------|--------|-------|'
        foreach ($b in $blocks) {
            $body = $b.Groups[1].Value
            $code = if ($body -match 'code:\s*"([^"]*)"') { $Matches[1] } else { '' }
            $name = if ($body -match 'name:\s*"([^"]*)"') { $Matches[1] } else { '' }
            $deb  = if ($body -match 'debit_account:\s*"([^"]*)"')  { $Matches[1] } else { '' }
            $cred = if ($body -match 'credit_account:\s*"([^"]*)"') { $Matches[1] } else { '' }
            $je   = if ($body -match 'generates_journal_entry:\s*true') { [char]0x2713 } else { '' }
            W "| $(Q $code) | $name | $deb | $cred | $je |"
        }
        W ''
    }
}

# ----- UI: access scopes -----
# SCOPE_CATALOG is the single source of truth for what a UI section is called and
# what it is for, so it doubles as the catalog of user-facing subsystems.
$scopeLabels = @{}
$scopeFile = Join-Path $root 'crates/backend/src/system/access/scope_catalog.rs'
if (Test-Path $scopeFile) {
    $txt = Get-Content $scopeFile -Raw -Encoding UTF8
    $blocks = [regex]::Matches($txt, 'ScopeDescriptor\s*\{(.+?)\}', 'Singleline')
    if ($blocks.Count -gt 0) {
        W "## UI scopes ($($blocks.Count))"
        W ''
        W '| Scope | Type | Category | Label | Description |'
        W '|-------|------|----------|-------|-------------|'
        foreach ($b in $blocks) {
            $body = $b.Groups[1].Value
            $id    = if ($body -match 'scope_id:\s*"([^"]*)"')            { $Matches[1] } else { '' }
            $type  = if ($body -match 'scope_type:\s*ScopeType::(\w+)')   { $Matches[1] } else { '' }
            $cat   = if ($body -match 'category:\s*"([^"]*)"')            { $Matches[1] } else { '' }
            $label = if ($body -match 'label:\s*"([^"]*)"')               { $Matches[1] } else { '' }
            $desc  = if ($body -match 'description:\s*"([^"]*)"')         { $Matches[1] } else { '' }
            if ($id -and $label) { $scopeLabels[$id] = $label }
            W "| $(Q $id) | $type | $cat | $(Cell $label) | $(Cell (Truncate $desc 120)) |"
        }
        W ''
    }
}

# ----- UI: tab labels and components -----
# The frontend is tab-based, not URL-routed: a page is identified by its tab key.
$tabLabels = @{}
$labelsFile = Join-Path $root 'crates/frontend/src/layout/tabs/tab_labels.rs'
if (Test-Path $labelsFile) {
    $txt = Get-Content $labelsFile -Raw -Encoding UTF8
    foreach ($m in [regex]::Matches($txt, '"(?<k>[^"]+)"\s*=>\s*"(?<v>[^"]*)"')) {
        $tabLabels[$m.Groups['k'].Value] = $m.Groups['v'].Value
    }
    # Aggregate tabs take their label from contracts metadata: `A012.ui.list_name`.
    foreach ($m in [regex]::Matches($txt, '"(?<k>[^"]+)"\s*=>\s*A(?<n>\d+)\.ui\.(?<f>list_name|element_name)')) {
        $idx = 'a' + $m.Groups['n'].Value
        $src = if ($m.Groups['f'].Value -eq 'list_name') { $listNames } else { $elementNames }
        if ($src.ContainsKey($idx)) { $tabLabels[$m.Groups['k'].Value] = $src[$idx] }
    }
}

$tabComponents = @{}
$tabsFile = Join-Path $root 'crates/frontend/src/layout/tabs/registry.rs'
if (Test-Path $tabsFile) {
    $txt = Get-Content $tabsFile -Raw -Encoding UTF8
    # An arm may wrap the view in a block and log first, and the component may be
    # written as a fully qualified path - take the last segment either way.
    # The body must not run past the start of the next arm, or a key whose arm ends
    # differently would steal the following key's `.into_any()`.
    foreach ($m in [regex]::Matches($txt, '"(?<k>[^"]+)"\s*=>\s*(?<body>(?:(?!"\s*=>)[\s\S]){0,600}?)\.into_any\(\)')) {
        if ($m.Groups['body'].Value -match '<(?<c>[A-Za-z_][\w:]*)') {
            $tabComponents[$m.Groups['k'].Value] = ($Matches['c'] -split '::')[-1]
        }
    }
}

# ----- UI: sidebar tree -----
$sidebarFile = Join-Path $root 'crates/frontend/src/layout/left/sidebar.rs'
if (Test-Path $sidebarFile) {
    $txt = Get-Content $sidebarFile -Raw -Encoding UTF8
    $chunks = [regex]::Split($txt, 'MenuGroup\s*\{')
    $groups = @()
    for ($i = 1; $i -lt $chunks.Count; $i++) {
        $c = $chunks[$i]
        $gid    = if ($c -match 'id:\s*"([^"]+)"')    { $Matches[1] } else { '' }
        $glabel = if ($c -match 'label:\s*"([^"]+)"') { $Matches[1] } else { '' }
        if (-not $gid) { continue }
        # The group's own admin_only flag is written after its items.
        $adminMatches = [regex]::Matches($c, 'admin_only:\s*(true|false)')
        $gadmin = if ($adminMatches.Count -gt 0) { $adminMatches[$adminMatches.Count - 1].Groups[1].Value -eq 'true' } else { $false }

        $items = @()
        foreach ($m in [regex]::Matches($c, 'SidebarItem::with_scope\(\s*"([^"]+)"')) {
            $items += [pscustomobject]@{ Pos = $m.Index; Id = $m.Groups[1].Value; Scope = $m.Groups[1].Value }
        }
        foreach ($m in [regex]::Matches($c, 'SidebarItem::new\(\s*"([^"]+)"')) {
            $items += [pscustomobject]@{ Pos = $m.Index; Id = $m.Groups[1].Value; Scope = '' }
        }
        foreach ($m in [regex]::Matches($c, 'SidebarItem\s*\{\s*id:\s*"(?<id>[^"]+)"(?<rest>[\s\S]{0,300}?)\n\s*\}')) {
            $scope = if ($m.Groups['rest'].Value -match 'scope_id:\s*Some\("([^"]+)"\)') { $Matches[1] } else { '' }
            $items += [pscustomobject]@{ Pos = $m.Index; Id = $m.Groups['id'].Value; Scope = $scope }
        }
        $groups += [pscustomobject]@{ Id = $gid; Label = $glabel; Admin = $gadmin; Items = ($items | Sort-Object Pos) }
    }

    if ($groups.Count -gt 0) {
        W "## UI sidebar ($($groups.Count) groups)"
        W ''
        W '> Tab key = page identity: it is also the scope id and the key in `layout/tabs/registry.rs`.'
        W '> Plugin pages are added at runtime from the `plugin` table and are not listed here.'
        W ''
        foreach ($g in $groups) {
            $suffix = if ($g.Admin) { ' (admin only)' } else { '' }
            W "### $(Q $g.Id) $($g.Label)$suffix"
            W ''
            W '| Tab key | Label | Scope | Component |'
            W '|---------|-------|-------|-----------|'
            foreach ($it in $g.Items) {
                $label = if ($tabLabels.ContainsKey($it.Id)) { $tabLabels[$it.Id] }
                         elseif ($scopeLabels.ContainsKey($it.Id)) { $scopeLabels[$it.Id] }
                         else { '' }
                $comp = if ($tabComponents.ContainsKey($it.Id)) { $tabComponents[$it.Id] } else { '' }
                $scope = if ($it.Scope) { Q $it.Scope } else { '' }
                W "| $(Q $it.Id) | $(Cell $label) | $scope | $comp |"
            }
            W ''
        }
    }
}

# ----- API routes -----
$routesFile = Join-Path $root 'crates/backend/src/api/routes.rs'
if (Test-Path $routesFile) {
    $txt = Get-Content $routesFile -Raw -Encoding UTF8
    # Split on the call itself instead of matching a closing paren: routes are
    # written both inline (`.route("/x", get(h))`) and spread over several lines,
    # and a single regex for both either misses the inline form or swallows the
    # next route together with its verbs.
    $rows = @()
    $parts = $txt -split '\.route\('
    for ($i = 1; $i -lt $parts.Count; $i++) {
        $part = $parts[$i]
        if ($part -notmatch '^\s*"(?<path>[^"]+)"\s*,') { continue }
        $p = $Matches['path']
        # Verbs live right after the path; the cap keeps unrelated code that
        # follows the route out of the match.
        $head = if ($part.Length -gt 400) { $part.Substring(0, 400) } else { $part }
        $verbs = [regex]::Matches($head, '\b(get|post|put|delete|patch)\(') |
            ForEach-Object { $_.Groups[1].Value.ToUpper() } | Select-Object -Unique
        $seg = if ($p -match '^/api/([^/]+)') { $Matches[1] } else { $p }
        $rows += [pscustomobject]@{ Group = $seg; Path = $p; Verbs = ($verbs -join ' ') }
    }
    if ($rows.Count -gt 0) {
        W "## API routes ($($rows.Count))"
        W ''
        foreach ($g in ($rows | Group-Object Group | Sort-Object Name)) {
            W "### $(Q ('/' + $g.Name))"
            foreach ($r in ($g.Group | Sort-Object Path)) {
                W "- $(Q $r.Verbs) $($r.Path)"
            }
            W ''
        }
    }
}

$dest = Join-Path $root 'ARCHITECTURE.md'
$enc  = New-Object System.Text.UTF8Encoding($true)   # UTF-8 with BOM
[System.IO.File]::WriteAllText($dest, $out.ToString(), $enc)
Write-Host "ARCHITECTURE.md regenerated: $dest"
