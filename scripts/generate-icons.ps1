# generate-icons.ps1 — rendera Echo-logotyp till PNG-serien + ICO.
#
# Kör: .\scripts\generate-icons.ps1
#
# Kräver: ImageMagick ("magick" på PATH). Om inte installerad:
#   winget install ImageMagick.ImageMagick
#
# Producerar:
#   src-tauri/icons/icon.png           (512×512 master)
#   src-tauri/icons/32x32.png
#   src-tauri/icons/64x64.png
#   src-tauri/icons/128x128.png
#   src-tauri/icons/128x128@2x.png     (256×256)
#   src-tauri/icons/icon.ico           (multi-size 16/32/48/64/128/256)
#   src-tauri/icons/Square30x30Logo.png
#   src-tauri/icons/Square44x44Logo.png
#   src-tauri/icons/Square71x71Logo.png
#   src-tauri/icons/Square89x89Logo.png
#   src-tauri/icons/Square107x107Logo.png
#   src-tauri/icons/Square142x142Logo.png
#   src-tauri/icons/Square150x150Logo.png
#   src-tauri/icons/Square284x284Logo.png
#   src-tauri/icons/Square310x310Logo.png
#   src-tauri/icons/StoreLogo.png     (50×50)
#   src-tauri/icons/tray-idle.png     (32×32, samma design)
#   src-tauri/icons/tray-recording.png (32×32, amber hot)

$ErrorActionPreference = "Stop"
$root = Resolve-Path (Join-Path $PSScriptRoot "..")
$iconsDir = Join-Path $root "src-tauri\icons"
$svgMaster = Join-Path $root "scripts\icon-master.svg"
$svgTray = Join-Path $root "scripts\icon-tray-recording.svg"

function Has-Magick {
    try {
        $null = Get-Command magick -ErrorAction Stop
        return $true
    } catch {
        return $false
    }
}

if (-not (Has-Magick)) {
    Write-Error @"
ImageMagick (`magick`) saknas på PATH. Installera via:
  winget install ImageMagick.ImageMagick
Starta sedan om terminalen och kör om detta script.
"@
}

if (-not (Test-Path $svgMaster)) {
    Write-Error "SVG-mastern saknas: $svgMaster. Skapa den först."
}

New-Item -ItemType Directory -Force -Path $iconsDir | Out-Null

function Render-Png($sizePx, $outName, $srcSvg = $svgMaster) {
    $out = Join-Path $iconsDir $outName
    Write-Host "[icons] $outName ($sizePx px)"
    & magick -background none -density 600 "$srcSvg" -resize "${sizePx}x${sizePx}" -strip "$out"
}

# Master + basic sizes
Render-Png 512 "icon.png"
Render-Png 32  "32x32.png"
Render-Png 64  "64x64.png"
Render-Png 128 "128x128.png"
Render-Png 256 "128x128@2x.png"

# Windows Store / Tauri bundle sizes
$storeSizes = @{
    "Square30x30Logo.png"   = 30
    "Square44x44Logo.png"   = 44
    "Square71x71Logo.png"   = 71
    "Square89x89Logo.png"   = 89
    "Square107x107Logo.png" = 107
    "Square142x142Logo.png" = 142
    "Square150x150Logo.png" = 150
    "Square284x284Logo.png" = 284
    "Square310x310Logo.png" = 310
    "StoreLogo.png"         = 50
}
foreach ($name in $storeSizes.Keys) {
    Render-Png $storeSizes[$name] $name
}

# Multi-size .ico — Tauri vill ha 256-max enligt Windows-spec.
Write-Host "[icons] icon.ico (multi-size)"
$icoSizes = "16,32,48,64,128,256"
& magick -background none -density 600 "$svgMaster" `
    -define icon:auto-resize=$icoSizes `
    -strip (Join-Path $iconsDir "icon.ico")

# Tray: idle = same design, recording = amber-hot variant.
Render-Png 32 "tray-idle.png"
if (Test-Path $svgTray) {
    Render-Png 32 "tray-recording.png" $svgTray
} else {
    Write-Host "[icons] tray-recording.svg saknas — kopierar tray-idle som fallback"
    Copy-Item (Join-Path $iconsDir "tray-idle.png") (Join-Path $iconsDir "tray-recording.png") -Force
}

Write-Host "[icons] Klart. Ikoner under: $iconsDir"
