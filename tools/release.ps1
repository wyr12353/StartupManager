$ErrorActionPreference = "Stop"
Set-Location (Split-Path -Parent $PSScriptRoot)

npm run version:check
if ($LASTEXITCODE -ne 0) { throw "version check failed" }

npm run tauri build
if ($LASTEXITCODE -ne 0) { throw "tauri build failed" }

$version = (Get-Content "package.json" -Raw -Encoding UTF8 | ConvertFrom-Json).version
$targetExe = "src-tauri/target/release/StartupManager.exe"
$targetInstaller = Get-ChildItem "src-tauri/target/release/bundle/nsis" -Filter "*_${version}_x64-setup.exe" |
    Select-Object -First 1
$targetMsi = Get-ChildItem "src-tauri/target/release/bundle/msi" -Filter "*_${version}_x64_*.msi" |
    Select-Object -First 1
if (-not (Test-Path $targetExe)) { throw "missing $targetExe" }
if (-not $targetInstaller) { throw "missing NSIS installer" }
if (-not $targetMsi) { throw "missing MSI installer" }

New-Item -ItemType Directory -Force "release" | Out-Null
Get-ChildItem "release" -File -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -like 'StartupManager*' } |
    Remove-Item -Force
Copy-Item $targetExe "release/StartupManager.exe" -Force
Copy-Item $targetInstaller.FullName "release/StartupManager_${version}_x64-setup.exe" -Force
Copy-Item $targetMsi.FullName "release/StartupManager_${version}_x64.msi" -Force

$installerName = "StartupManager_${version}_x64-setup.exe"
$msiName = "StartupManager_${version}_x64.msi"
$exeName = "StartupManager.exe"
$lines = foreach ($name in @($installerName, $msiName, $exeName)) {
    $hash = (Get-FileHash (Join-Path "release" $name) -Algorithm SHA256).Hash.ToLower()
    "$hash *$name"
}

$checksumPath = Join-Path (Get-Location) "release/SHA256SUMS.txt"
[System.IO.File]::WriteAllText($checksumPath, (($lines -join "`n") + "`n"), [System.Text.UTF8Encoding]::new($false))

foreach ($name in @($installerName, $msiName, $exeName)) {
    $expected = ($lines | Where-Object { $_ -like "*$name" }).Split(" ")[0]
    $actual = (Get-FileHash (Join-Path "release" $name) -Algorithm SHA256).Hash.ToLower()
    if ($expected -ne $actual) { throw "SHA256 mismatch for $name" }
}

Write-Host "Release artifacts updated:"
Get-Content $checksumPath
