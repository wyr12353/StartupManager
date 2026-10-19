$ErrorActionPreference = "Stop"
Set-Location (Split-Path -Parent $PSScriptRoot)

# Windows PowerShell 5.1 decodes native stdout with the OEM code page by
# default; the JSON produced by the CLI is UTF-8.
chcp 65001 | Out-Null
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$OutputEncoding = [System.Text.Encoding]::UTF8

Write-Host "== Toolchain =="
cargo --version
rustc --version
node --version
npm --version

Write-Host "`n== StartupApproved selftest =="
cargo run --manifest-path src-tauri/Cargo.toml -- --selftest
if ($LASTEXITCODE -ne 0) { throw "selftest failed" }

Write-Host "`n== Read-only scan =="
$json = cargo run --manifest-path src-tauri/Cargo.toml -- --scan-json | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) { throw "scan failed" }
foreach ($source in $json) {
    $suffix = if ($source.error) { " (error: $($source.error))" } else { "" }
    Write-Host ("{0}: {1} items{2}" -f $source.source, $source.items.Count, $suffix)
}

Write-Host "`nRead-only verification complete. See docs/ACCEPTANCE.md for write tests."
