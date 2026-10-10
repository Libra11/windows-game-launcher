param(
  [string]$ProjectDirectory = 'outputs/windows-game-launcher',
  [string]$BundleDirectory,
  [string]$OutputDirectory,
  [string]$Repository = $env:GITHUB_REPOSITORY
)
$ErrorActionPreference = 'Stop'

function Read-ReleaseNotes([string]$Project, [string]$Version) {
  $path = Join-Path $Project "release-notes/$Version.md"
  if (!(Test-Path -LiteralPath $path -PathType Leaf)) { throw "缺少版本 $Version 的中文发布说明。" }
  $notes = [IO.File]::ReadAllText((Resolve-Path -LiteralPath $path)).Trim()
  if ([string]::IsNullOrWhiteSpace($notes) -or [Text.Encoding]::UTF8.GetByteCount($notes) -gt 262144) { throw '更新说明为空或超过 256 KiB。' }
  return $notes
}

function New-UpdateManifest([string]$Version, [string]$Notes, [string]$Signature, [string]$Repo, [string]$Installer) {
  if ($Version -notmatch '^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$') { throw '更新版本号无效。' }
  if ($Repo -notmatch '^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$') { throw '发布仓库标识无效。' }
  if ($Installer -ne "youji_${Version}_x64-setup.exe") { throw '更新安装包文件名与版本不一致。' }
  if ([string]::IsNullOrWhiteSpace($Notes)) { throw '更新说明为空。' }
  try { $decoded = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($Signature)) } catch { throw '更新签名编码无效。' }
  $lines = $decoded -split '\r?\n'
  if ($lines.Count -lt 4 -or $lines[0] -notmatch '^untrusted comment:' -or $lines[2] -notmatch '^trusted comment:') { throw '更新签名格式无效。' }
  try {
    if ([Convert]::FromBase64String($lines[1]).Length -ne 74 -or [Convert]::FromBase64String($lines[3]).Length -ne 64) { throw '长度无效' }
  } catch { throw '更新签名格式无效。' }
  $signedVersion = ($lines[2] -split "`t" | Where-Object { $_.StartsWith('version:') } | Select-Object -First 1)
  if (!$signedVersion -or $signedVersion.Substring(8) -ne $Version) { throw '签名版本与发布版本不一致，请使用支持版本签名的 Tauri CLI。' }
  return [ordered]@{
    version = $Version
    notes = $Notes
    pub_date = [DateTime]::UtcNow.ToString('yyyy-MM-ddTHH:mm:ssZ')
    platforms = [ordered]@{
      'windows-x86_64' = [ordered]@{
        url = "https://github.com/$Repo/releases/download/v$Version/$Installer"
        signature = $Signature
      }
    }
  }
}

function Prepare-UpdateRelease([string]$Project, [string]$Bundle, [string]$Output, [string]$Repo) {
  $config = Get-Content -LiteralPath (Join-Path $Project 'src-tauri/tauri.conf.json') -Raw | ConvertFrom-Json
  $version = $config.version
  $notes = Read-ReleaseNotes $Project $version
  $installers = @(Get-ChildItem -LiteralPath $Bundle -Filter '*.exe' -File)
  if ($installers.Count -ne 1) { throw '预期生成一个 NSIS 更新安装包。' }
  $source = $installers[0].FullName
  $signaturePath = "$source.sig"
  if (!(Test-Path -LiteralPath $signaturePath -PathType Leaf)) { throw '缺少安装包对应的 .sig 文件。' }
  $signature = [IO.File]::ReadAllText($signaturePath).Trim()
  $name = "youji_${version}_x64-setup.exe"
  $manifest = New-UpdateManifest $version $notes $signature $Repo $name
  & node (Join-Path $PSScriptRoot 'verify-update-signature.mjs') (Join-Path $Project 'src-tauri/tauri.conf.json') $source $signaturePath
  if ($LASTEXITCODE -ne 0) { throw '安装包与签名的密码学验证失败，停止发布。' }
  [IO.Directory]::CreateDirectory($Output) | Out-Null
  Copy-Item -LiteralPath $source -Destination (Join-Path $Output $name)
  Copy-Item -LiteralPath $signaturePath -Destination (Join-Path $Output "$name.sig")
  [IO.File]::WriteAllText((Join-Path $Output 'latest.json'), ($manifest | ConvertTo-Json -Depth 8), [Text.UTF8Encoding]::new($false))
  [IO.File]::WriteAllText((Join-Path $Output 'release-notes.md'), $notes, [Text.UTF8Encoding]::new($false))
}

if ($MyInvocation.InvocationName -ne '.') {
  if (!$BundleDirectory -or !$OutputDirectory) { throw '需要指定安装包目录和发布输出目录。' }
  Prepare-UpdateRelease $ProjectDirectory $BundleDirectory $OutputDirectory $Repository
}
