# Copies the Tuatara bundled libraries into deps\, which git ignores, so build.tua
# names no path outside this repository. The whole libs tree is copied because the libraries
# depend on each other by relative path (vulkan on vk, spirv and win32; vk on xml and cdecl).
#
# Usage: powershell -File tools\vendor-libs.ps1 <path to a Tuatara checkout>
param([Parameter(Mandatory = $true)][string]$TuataraRepo)
$ErrorActionPreference = 'Stop'

$src = Join-Path $TuataraRepo 'libs'
if (-not (Test-Path (Join-Path $src 'vulkan\build.tua'))) {
    throw "no libs\vulkan\build.tua under '$TuataraRepo'"
}
$dst = Join-Path $PSScriptRoot '..\deps'
if (Test-Path $dst) { Remove-Item -Recurse -Force $dst }
New-Item -ItemType Directory -Force $dst | Out-Null
Copy-Item -Recurse (Join-Path $src '*') $dst

# Each library's build cache stays behind.
$caches = @(Get-ChildItem $dst -Directory -Recurse -Force -Filter '.tua')
foreach ($c in $caches) {
    if (Test-Path $c.FullName) { Remove-Item -Recurse -Force $c.FullName }
}
Write-Output "copied $src to $((Resolve-Path $dst).Path)"
