$ErrorActionPreference = 'Stop'
$testRoot = Join-Path ([IO.Path]::GetTempPath()) "youji-release-tests-$([guid]::NewGuid())"
$project = Join-Path $testRoot 'outputs/windows-game-launcher'
New-Item -ItemType Directory -Path (Join-Path $project 'src-tauri') -Force | Out-Null
$check = Join-Path $PSScriptRoot 'check-release-version.ps1'
$detect = Join-Path $PSScriptRoot 'detect-release-version.ps1'

function Write-Version([string]$version) {
    "{`"version`":`"$version`"}" | Set-Content (Join-Path $project 'package.json')
    "{`"version`":`"$version`",`"packages`":{`"`":{`"version`":`"$version`"}}}" |
        Set-Content (Join-Path $project 'package-lock.json')
    "{`"version`":`"$version`"}" | Set-Content (Join-Path $project 'src-tauri/tauri.conf.json')
    "[package]`nname = `"local_achievement_launcher`"`nversion = `"$version`"" |
        Set-Content (Join-Path $project 'src-tauri/Cargo.toml')
    "[[package]]`nname = `"local_achievement_launcher`"`nversion = `"$version`"" |
        Set-Content (Join-Path $project 'src-tauri/Cargo.lock')
}
function Git-Test {
    $arguments = @($args)
    $result = git -C $testRoot @arguments
    if ($LASTEXITCODE -ne 0) { throw "测试仓库 Git 命令失败：$arguments" }
    return $result
}
function Assert-Throws([scriptblock]$action, [string]$message) {
    try { & $action | Out-Null } catch { return }
    throw "预期失败：$message"
}

try {
    Git-Test @('init', '--quiet') | Out-Null
    Write-Version '0.2.1'
    Git-Test @('add', '.') | Out-Null
    Git-Test @('-c', 'user.name=Release Test', '-c', 'user.email=release-test@example.invalid', 'commit', '--quiet', '-m', 'before') | Out-Null
    $before = Git-Test @('rev-parse', 'HEAD')
    Write-Version '0.2.2'
    Git-Test @('add', '.') | Out-Null
    Git-Test @('-c', 'user.name=Release Test', '-c', 'user.email=release-test@example.invalid', 'commit', '--quiet', '-m', 'version bump') | Out-Null
    $head = Git-Test @('rev-parse', 'HEAD')

    $output = Join-Path $testRoot 'outputs.txt'
    $bump = & $detect -Before $before -ProjectPath $project -OutputPath $output
    if (-not $bump.Changed -or $bump.Tag -ne 'v0.2.2' -or
        (Get-Content $output) -notcontains 'changed=true') { throw '版本变更或 GitHub 输出检测失败' }
    $unchanged = & $detect -Before $head -ProjectPath $project -OutputPath ''
    if ($unchanged.Changed) { throw '版本未变时不应发布' }
    $created = & $detect -Before ('0' * 40) -ProjectPath $project -OutputPath ''
    if (-not $created.Changed) { throw '首次创建 main 时未检测到版本' }
    Git-Test @('tag', 'v0.2.2') | Out-Null
    $retry = & $detect -Before $before -ProjectPath $project -OutputPath ''
    if (-not $retry.Changed) { throw '原事件重试应保留发布判定' }

    foreach ($relative in @('package.json', 'package-lock.json', 'src-tauri/Cargo.toml', 'src-tauri/Cargo.lock', 'src-tauri/tauri.conf.json')) {
        $path = Join-Path $project $relative
        $original = Get-Content -LiteralPath $path -Raw
        $original.Replace('0.2.2', '0.2.3') | Set-Content -LiteralPath $path
        Assert-Throws { & $check -ProjectPath $project } "版本不一致：$relative"
        $original | Set-Content -LiteralPath $path
    }
    Assert-Throws { & $check -ProjectPath $project -Tag 'v0.2.3' } '标签不一致'
    Assert-Throws { & $detect -Before ('f' * 40) -ProjectPath $project -OutputPath '' } '无法读取推送前版本'
    Write-Version 'bad-version'
    Assert-Throws { & $check -ProjectPath $project } '无效版本'
    Write-Version '0.2.3-beta.1'
    if ((& $check -ProjectPath $project) -ne '0.2.3-beta.1') { throw '预发布版本校验失败' }
    Write-Version '0.2.3'
    Git-Test @('tag', 'v0.2.3', $before) | Out-Null
    Assert-Throws { & $detect -Before $before -ProjectPath $project -OutputPath '' } '已有标签指向其他提交'
    Write-Host '版本检测、重试、五处一致性、标签冲突与预发布校验通过。'
} finally {
    $resolvedRoot = [IO.Path]::GetFullPath($testRoot)
    $tempBase = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd([IO.Path]::DirectorySeparatorChar)
    if ([IO.Path]::GetDirectoryName($resolvedRoot) -ne $tempBase -or
        [IO.Path]::GetFileName($resolvedRoot) -notlike 'youji-release-tests-*') { throw '测试清理路径不符合预期' }
    Remove-Item -LiteralPath $resolvedRoot -Recurse -Force
}
