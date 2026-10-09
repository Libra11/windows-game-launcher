param(
    [Parameter(Mandatory = $true)]
    [ValidatePattern('^[0-9a-f]{40}$')]
    [string]$Before,
    [string]$ProjectPath = (Join-Path $PSScriptRoot '../../outputs/windows-game-launcher'),
    [string]$OutputPath = $env:GITHUB_OUTPUT
)

$ErrorActionPreference = 'Stop'
$version = & (Join-Path $PSScriptRoot 'check-release-version.ps1') -ProjectPath $ProjectPath
$previousVersion = $null
if ($Before -ne ('0' * 40)) {
    $previous = git -C $ProjectPath show "${Before}:outputs/windows-game-launcher/src-tauri/tauri.conf.json"
    if ($LASTEXITCODE -ne 0) { throw '无法读取 main 推送前的应用版本，停止自动发布。' }
    $previousVersion = ($previous -join "`n" | ConvertFrom-Json).version
    if (-not $previousVersion) { throw 'main 推送前的应用版本不存在，停止自动发布。' }
}
$changed = $version -cne $previousVersion
$tag = "v$version"
if ($changed) {
    $existingTag = git -C $ProjectPath tag --list $tag
    if ($LASTEXITCODE -ne 0) { throw '无法查询版本标签，停止自动发布。' }
    if ($existingTag) {
        $tagCommit = git -C $ProjectPath rev-parse --verify "refs/tags/$tag^{commit}"
        if ($LASTEXITCODE -ne 0) { throw '无法读取版本标签指向的提交，停止自动发布。' }
        $headCommit = git -C $ProjectPath rev-parse HEAD
        if ($LASTEXITCODE -ne 0 -or $tagCommit -cne $headCommit) {
            throw '版本标签已指向其他提交，请升级版本号后发布。'
        }
    }
}
if ($OutputPath) {
    @("changed=$($changed.ToString().ToLowerInvariant())", "tag=$tag") |
        Add-Content -LiteralPath $OutputPath -Encoding utf8
}
Write-Host "应用版本：$previousVersion -> $version；需要发布：$changed"
return [pscustomobject]@{ Changed = $changed; Tag = $tag }
