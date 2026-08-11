param(
    [string]$OutputPath = "paper-results/opfs-benchmark-raw.json"
)

$ErrorActionPreference = "Stop"
$projectDir = Split-Path -Parent $PSScriptRoot
$benchmarkDir = Join-Path $PSScriptRoot "opfs-benchmark"
$manifest = Join-Path $benchmarkDir "Cargo.toml"
$targetDir = Join-Path $env:TEMP "chunklog-opfs-benchmark-target"
$wasm = Join-Path $targetDir "wasm32-unknown-unknown/release/chunklog_opfs_benchmark.wasm"
$pkg = Join-Path $benchmarkDir "pkg"

$env:CARGO_TARGET_DIR = $targetDir
cargo build --manifest-path $manifest --target wasm32-unknown-unknown --release --locked
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

wasm-bindgen $wasm --target web --out-dir $pkg --no-typescript
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

if (-not $env:PLAYWRIGHT_CORE_PATH) {
    $link = Get-ChildItem (Join-Path $env:LOCALAPPDATA "ms-playwright/.links") -File -ErrorAction SilentlyContinue |
        ForEach-Object { Get-Content $_.FullName } |
        Where-Object { Test-Path $_ } |
        Select-Object -First 1
    if ($link) { $env:PLAYWRIGHT_CORE_PATH = $link }
}
if (-not $env:OPFS_BROWSER_EXE) {
    $browser = Get-ChildItem (Join-Path $env:LOCALAPPDATA "ms-playwright") -Recurse -Filter chrome.exe -ErrorAction SilentlyContinue |
        Select-Object -First 1 -ExpandProperty FullName
    if ($browser) { $env:OPFS_BROWSER_EXE = $browser }
}
if (-not $env:PLAYWRIGHT_CORE_PATH) {
    throw "Set PLAYWRIGHT_CORE_PATH or run npm install in paper-workloads/opfs-benchmark"
}
if (-not $env:OPFS_BROWSER_EXE) {
    throw "Set OPFS_BROWSER_EXE to a Chromium or Edge executable"
}

$resolvedOutput = Join-Path $projectDir $OutputPath
node (Join-Path $benchmarkDir "runner.cjs") $resolvedOutput
exit $LASTEXITCODE
