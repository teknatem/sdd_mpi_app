$ErrorActionPreference = "Stop"

$workspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$pluginRoot = (Resolve-Path (Join-Path $workspaceRoot "plugins\ym-cash-payments")).Path
$distRoot = Join-Path $workspaceRoot "plugins\dist"
$archivePath = Join-Path $distRoot "PLG-YM-CASH-PAYMENTS.plugin.zip"

if (-not $pluginRoot.StartsWith($workspaceRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Plugin source resolved outside the workspace"
}

$required = @(
    "plugin.json",
    "client.js",
    "server.js",
    "styles.css",
    "sql\cabinets.sql",
    "sql\dailyFlow.sql",
    "sql\freshness.sql",
    "sql\monthlyCosts.sql",
    "sql\monthlyCostSummary.sql",
    "sql\orderStateSummary.sql",
    "sql\orders.sql",
    "sql\pendingSummary.sql",
    "sql\settlementDue.sql",
    "sql\summary.sql"
)

foreach ($relativePath in $required) {
    $path = Join-Path $pluginRoot $relativePath
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Missing plugin file: $relativePath"
    }
}

New-Item -ItemType Directory -Path $distRoot -Force | Out-Null
if (Test-Path -LiteralPath $archivePath) {
    Remove-Item -LiteralPath $archivePath -Force
}

Add-Type -AssemblyName System.IO.Compression
$stream = [System.IO.File]::Open(
    $archivePath,
    [System.IO.FileMode]::CreateNew,
    [System.IO.FileAccess]::ReadWrite,
    [System.IO.FileShare]::None
)
try {
    $archive = [System.IO.Compression.ZipArchive]::new(
        $stream,
        [System.IO.Compression.ZipArchiveMode]::Create,
        $false
    )
    try {
        foreach ($relativePath in $required) {
            $entryName = $relativePath.Replace("\", "/")
            $entry = $archive.CreateEntry(
                $entryName,
                [System.IO.Compression.CompressionLevel]::Optimal
            )
            $entryStream = $entry.Open()
            try {
                $bytes = [System.IO.File]::ReadAllBytes((Join-Path $pluginRoot $relativePath))
                $entryStream.Write($bytes, 0, $bytes.Length)
            }
            finally {
                $entryStream.Dispose()
            }
        }
    }
    finally {
        $archive.Dispose()
    }
}
finally {
    $stream.Dispose()
}

Write-Output $archivePath
