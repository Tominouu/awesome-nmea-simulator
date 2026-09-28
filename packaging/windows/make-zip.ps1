# Construit une archive portable nmeasim-rs-<version>-windows-x64.zip (à lancer sur Windows).
$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..\..")
cargo build --release -p nmeasim
$meta = cargo metadata --no-deps --format-version 1 | ConvertFrom-Json
$version = ($meta.packages | Where-Object { $_.name -eq "nmeasim" }).version
$out = "target\windows\nmeasim-rs-$version"
Remove-Item -Recurse -Force $out -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $out | Out-Null
Copy-Item target\release\nmeasim.exe $out
Copy-Item README.md, docs\user-guide.md, docs\gamepad.md, docs\network.md $out
Compress-Archive -Path "$out\*" -DestinationPath "target\windows\nmeasim-rs-$version-windows-x64.zip" -Force
Write-Output "target\windows\nmeasim-rs-$version-windows-x64.zip"
