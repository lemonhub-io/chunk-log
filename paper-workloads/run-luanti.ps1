param(
    [Parameter(Mandatory = $true)]
    [string]$LuantiRoot,

    [Parameter(Mandatory = $true)]
    [string]$SqliteExe,

    [Parameter(Mandatory = $true)]
    [string]$WorldPath
)

$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$luantiExe = Join-Path $LuantiRoot 'bin\luanti.exe'
$gameDestination = Join-Path $LuantiRoot 'games\chunklog_artifact'
$gameSource = Join-Path $PSScriptRoot 'luanti-game'
$worldSource = Join-Path $PSScriptRoot 'luanti-world'

if (-not (Test-Path -LiteralPath $luantiExe -PathType Leaf)) {
    throw "Luanti executable not found: $luantiExe"
}
if (-not (Test-Path -LiteralPath $SqliteExe -PathType Leaf)) {
    throw "sqlite3 executable not found: $SqliteExe"
}
if (Test-Path -LiteralPath $WorldPath) {
    throw "WorldPath already exists; choose a new empty target: $WorldPath"
}

New-Item -ItemType Directory -Path $gameDestination -Force | Out-Null
New-Item -ItemType Directory -Path (Join-Path $gameDestination 'mods') -Force | Out-Null
Copy-Item -LiteralPath (Join-Path $gameSource 'game.conf') -Destination $gameDestination
Copy-Item -LiteralPath (Join-Path $gameSource 'mods\generator') -Destination (Join-Path $gameDestination 'mods') -Recurse

New-Item -ItemType Directory -Path $WorldPath | Out-Null
Copy-Item -Path (Join-Path $worldSource '*') -Destination $WorldPath

$arguments = @(
    '--server',
    '--world', $WorldPath,
    '--gameid', 'chunklog_artifact',
    '--config', (Join-Path $WorldPath 'minetest.conf')
)
$process = Start-Process -FilePath $luantiExe -ArgumentList $arguments -WindowStyle Hidden -Wait -PassThru
if ($process.ExitCode -ne 0) {
    throw "Luanti exited with code $($process.ExitCode)"
}

$database = Join-Path $WorldPath 'map.sqlite'
if (-not (Test-Path -LiteralPath $database -PathType Leaf)) {
    throw "Luanti did not create $database"
}

Push-Location $repoRoot
try {
    cargo run --release --offline --example luanti_workload -- $SqliteExe $database
    if ($LASTEXITCODE -ne 0) {
        throw "luanti_workload failed with code $LASTEXITCODE"
    }
}
finally {
    Pop-Location
}
