# Quick installation helper for Anvil.
# Anvil targets statically linked Linux binaries; Windows binaries are unavailable.

$ErrorActionPreference = "Stop"

$installDir = if ($env:ANVIL_INSTALL_DIR) {
    $env:ANVIL_INSTALL_DIR
} else {
    Join-Path $HOME ".local\bin"
}

if ($IsWindows -ne $true) {
    Write-Host "This PowerShell installer is intended for Windows."
    exit 1
}

Write-Host "Anvil does not currently provide Windows release binaries."
Write-Host "Supported targets are Linux x86_64 and aarch64 only."
Write-Host "Install it on Linux with scripts/install.sh, or inspect docs/quickstart.md."

exit 1
