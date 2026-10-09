param(
    [string]$Tag,
    [string]$ProjectPath = (Join-Path $PSScriptRoot '../../outputs/windows-game-launcher')
)

$ErrorActionPreference = 'Stop'
$package = Get-Content -LiteralPath (Join-Path $projectPath 'package.json') -Raw | ConvertFrom-Json
$packageLock = Get-Content -LiteralPath (Join-Path $projectPath 'package-lock.json') -Raw | ConvertFrom-Json -AsHashtable
$tauri = Get-Content -LiteralPath (Join-Path $projectPath 'src-tauri/tauri.conf.json') -Raw | ConvertFrom-Json
$cargo = Get-Content -LiteralPath (Join-Path $projectPath 'src-tauri/Cargo.toml') -Raw
$cargoPackage = [regex]::Match($cargo, '(?ms)^\[package\]\s*(.*?)(?=^\[|\z)').Groups[1].Value
$cargoVersion = [regex]::Match($cargoPackage, '(?m)^version\s*=\s*"([^"]+)"').Groups[1].Value
$cargoLock = Get-Content -LiteralPath (Join-Path $projectPath 'src-tauri/Cargo.lock') -Raw
$cargoLockVersion = [regex]::Match($cargoLock, '(?ms)^\[\[package\]\]\s*name = "local_achievement_launcher"\s*version = "([^"]+)"').Groups[1].Value
$version = $tauri.version

if ($version -notmatch '^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$') {
    throw '应用版本必须是有效的 major.minor.patch 版本号，可包含预发布标识。'
}
if ($package.version -cne $version -or $cargoVersion -cne $version -or $cargoLockVersion -cne $version -or
    $packageLock.version -ne $version -or $packageLock.packages[''].version -ne $version) {
    throw 'package.json、package-lock.json、Cargo.toml、Cargo.lock 与 tauri.conf.json 的版本必须一致。'
}
if (-not $Tag) { $Tag = "v$version" }
if ($Tag -cne "v$version") {
    throw "发布标签 $Tag 与应用版本不一致，预期标签为 v$version。"
}

Write-Host "发布版本校验通过：$Tag"
return $version
