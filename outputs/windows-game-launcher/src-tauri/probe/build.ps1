param([string]$Destination)
$ErrorActionPreference='Stop'
if (!$Destination) { $Destination=Join-Path $PSScriptRoot '../resources/XboxLocalProbe.exe' }
Add-Type -Path @((Join-Path $PSScriptRoot 'WellInterfaceProbe.cs'),(Join-Path $PSScriptRoot 'ProbeHost.cs')) -OutputAssembly $Destination -OutputType WindowsApplication
