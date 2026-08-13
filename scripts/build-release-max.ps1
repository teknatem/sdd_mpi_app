# Maximum, deployment-safe optimization profile.
# Fat LTO and a single codegen unit make this considerably slower than a normal release build.

param(
    [switch]$NativeCpu
)

$BuildScript = Join-Path $PSScriptRoot "build-release.ps1"
$PreviousRustFlags = $env:RUSTFLAGS

try {
    if ($NativeCpu) {
        Write-Warning "NativeCpu may produce a binary that will not run on an older/different server CPU."
        $env:RUSTFLAGS = ((@($PreviousRustFlags, "-C target-cpu=native") | Where-Object { $_ }) -join " ").Trim()
    }

    & $BuildScript -CargoProfile release-max
    exit $LASTEXITCODE
}
finally {
    $env:RUSTFLAGS = $PreviousRustFlags
}
