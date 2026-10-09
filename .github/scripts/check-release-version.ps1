param(
    [Parameter(Mandatory = $true)]
    [string]$Tag
)

$ErrorActionPreference = 'Stop'
$projectPath = Join-Path $PSScriptRoot '../../outputs/windows-game-launcher'
$package = Get-Content -LiteralPath (Join-Path $projectPath 'package.json') -Raw | ConvertFrom-Json
$packageLock = Get-Content -LiteralPath (Join-Path $projectPath 'package-lock.json') -Raw | ConvertFrom-Json -AsHashtable
$tauri = Get-Content -LiteralPath (Join-Path $projectPath 'src-tauri/tauri.conf.json') -Raw | ConvertFrom-Json
$cargo = Get-Content -LiteralPath (Join-Path $projectPath 'src-tauri/Cargo.toml') -Raw
$cargoPackage = [regex]::Match($cargo, '(?ms)^\[package\]\s*(.*?)(?=^\[|\z)').Groups[1].Value
$cargoVersion = [regex]::Match($cargoPackage, '(?m)^version\s*=\s*"([^"]+)"').Groups[1].Value
$version = $tauri.version

if (-not $version -or $package.version -ne $version -or $cargoVersion -ne $version -or
    $packageLock.version -ne $version -or $packageLock.packages[''].version -ne $version) {
    throw 'package.json、package-lock.json、Cargo.toml 与 tauri.conf.json 的版本必须一致。'
}
if ($Tag -cne "v$version") {
    throw "发布标签 $Tag 与应用版本不一致，预期标签为 v$version。"
}

Write-Output "发布版本校验通过：$Tag"
