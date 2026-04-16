# SVoice 3 — Setup-script för utvecklings/användning på Windows 11
#
# Användning: högerklicka → "Kör med PowerShell" (eller från PS: .\scripts\setup-dev.ps1)
# Skriptet installerar nödvändiga verktyg via winget och kör ett första dev-bygge.

$ErrorActionPreference = "Stop"

function Write-Step($msg) {
    Write-Host "`n>>> $msg" -ForegroundColor Cyan
}

function Have-Command($name) {
    $null -ne (Get-Command $name -ErrorAction SilentlyContinue)
}

Write-Host "SVoice 3 dev-setup" -ForegroundColor Green
Write-Host "Denna installation installerar Rust, Node.js, pnpm och Tauri CLI."
Write-Host "Total tid: ~15 minuter (nätverksberoende)."
Write-Host ""
Read-Host "Tryck Enter för att fortsätta (eller Ctrl+C för att avbryta)"

# 1. Rust
Write-Step "Kontrollerar Rust..."
if (-not (Have-Command "cargo")) {
    Write-Host "Rust saknas. Installerar via winget..."
    winget install --id Rustlang.Rustup -e --accept-source-agreements --accept-package-agreements
    $env:PATH = "$env:USERPROFILE\.cargo\bin;" + $env:PATH
    if (-not (Have-Command "cargo")) {
        throw "Rust-installation misslyckades. Starta om terminalen och prova igen."
    }
}
Write-Host "Rust: $(rustc --version)"

# 2. Node.js
Write-Step "Kontrollerar Node.js..."
if (-not (Have-Command "node")) {
    Write-Host "Node.js saknas. Installerar via winget..."
    winget install --id OpenJS.NodeJS.LTS -e --accept-source-agreements --accept-package-agreements
}
Write-Host "Node: $(node --version)"

# 3. pnpm
Write-Step "Kontrollerar pnpm..."
if (-not (Have-Command "pnpm")) {
    Write-Host "pnpm saknas. Installerar globalt via npm..."
    npm install -g pnpm
}
Write-Host "pnpm: $(pnpm --version)"

# 4. CMake (för stt-spike-deps om de behövs)
Write-Step "Kontrollerar CMake..."
if (-not (Have-Command "cmake")) {
    Write-Host "CMake saknas. Installerar via winget..."
    winget install --id Kitware.CMake -e --accept-source-agreements --accept-package-agreements
}

# 5. Tauri CLI
Write-Step "Kontrollerar Tauri CLI..."
if (-not (Have-Command "cargo-tauri")) {
    Write-Host "Tauri CLI saknas. Installerar via cargo (~2-4 min)..."
    cargo install tauri-cli --version "^2.0" --locked
}

# 6. Node-deps
Write-Step "Installerar Node-paket..."
pnpm install

# 7. Första bygget
Write-Step "Kör första cargo build (kan ta 5-10 min första gången)..."
Push-Location src-tauri
cargo build -p svoice-v3
Pop-Location

Write-Host "`n✔ Setup klar!" -ForegroundColor Green
Write-Host "Starta appen med:"
Write-Host "    cargo tauri dev" -ForegroundColor Yellow
Write-Host ""
Write-Host "Appen öppnar ett huvudfönster + en liten overlay-pill i hörnet."
Write-Host "Håll höger Ctrl i valfri Windows-app och släpp → testtexten injiceras."
