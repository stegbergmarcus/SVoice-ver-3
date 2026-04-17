# bundle-python.ps1 — Bundlar Python-runtime + faster-whisper + CUDA 12 DLLs
# för release-build av SVoice 3.
#
# Resultat: src-tauri/resources/python-runtime/python/ med allt som behövs.
#
# Körs manuellt innan `cargo tauri build` om python-runtime saknas eller
# behöver uppdateras. Idempotent: hoppar steg om redan gjorda.

$ErrorActionPreference = "Stop"
$root = Resolve-Path (Join-Path $PSScriptRoot "..")
$runtime = Join-Path $root "src-tauri\resources\python-runtime"
$pythonDir = Join-Path $runtime "python"

$PythonVersion = "3.11.9"
$embedZipName = "python-$PythonVersion-embed-amd64.zip"
$embedUrl = "https://www.python.org/ftp/python/$PythonVersion/$embedZipName"

# Pip-paket som ska installeras i runtime.
# Versioner låsta för reproducerbar build.
$packages = @(
    "faster-whisper",
    "numpy",
    "nvidia-cublas-cu12",
    "nvidia-cudnn-cu12",
    "nvidia-cuda-runtime-cu12",
    "nvidia-cuda-nvrtc-cu12",
    "hf_xet"
)

function Log($msg) {
    Write-Host "[bundle-python] $msg" -ForegroundColor Cyan
}

function Log-Warn($msg) {
    Write-Host "[bundle-python] $msg" -ForegroundColor Yellow
}

# 1. Hämta embeddable Python om den inte redan finns
if (Test-Path (Join-Path $pythonDir "python.exe")) {
    Log "Python-embeddable redan extraherat till $pythonDir — skippar nedladdning."
} else {
    Log "Laddar ner Python $PythonVersion embeddable..."
    New-Item -ItemType Directory -Force -Path $runtime | Out-Null
    $embedZip = Join-Path $runtime $embedZipName
    Invoke-WebRequest $embedUrl -OutFile $embedZip
    Log "Extraherar till $pythonDir..."
    Expand-Archive $embedZip -DestinationPath $pythonDir -Force
    Remove-Item $embedZip
}

# 2. Uncomment `import site` i python311._pth så pip + site-packages fungerar.
#    Embeddable-defaulten är att kommentera ut det, vilket bryter pip.
$pthFile = Join-Path $pythonDir "python311._pth"
if (Test-Path $pthFile) {
    $pthContent = Get-Content $pthFile -Raw
    if ($pthContent -match "(?m)^#\s*import\s+site\s*$") {
        Log "Aktiverar 'import site' i python311._pth..."
        $pthContent = $pthContent -replace "(?m)^#\s*import\s+site\s*$", "import site"
        Set-Content $pthFile $pthContent -NoNewline
    } else {
        Log "'import site' redan aktivt i python311._pth."
    }
} else {
    Log-Warn "python311._pth saknas — embeddable-distributionen har ändrats?"
}

# 3. Bootstrap pip (saknas i embeddable)
$pipCheck = & (Join-Path $pythonDir "python.exe") -m pip --version 2>&1
if ($LASTEXITCODE -eq 0) {
    Log "pip redan installerat: $pipCheck"
} else {
    Log "Bootstrapar pip via get-pip.py..."
    $getPip = Join-Path $runtime "get-pip.py"
    Invoke-WebRequest "https://bootstrap.pypa.io/get-pip.py" -OutFile $getPip
    & (Join-Path $pythonDir "python.exe") $getPip
    Remove-Item $getPip
}

# 4. Installera pip-paketen
Log "Installerar Python-paket i bundlad runtime..."
$pkgArgs = @("-m", "pip", "install", "--upgrade") + $packages
& (Join-Path $pythonDir "python.exe") @pkgArgs
if ($LASTEXITCODE -ne 0) {
    Write-Error "pip install failade (exit $LASTEXITCODE)."
}

# 5. Storleksrapport
$sizeBytes = (Get-ChildItem $runtime -Recurse -ErrorAction SilentlyContinue |
    Measure-Object -Property Length -Sum).Sum
$sizeMb = [math]::Round($sizeBytes / 1MB, 1)
Log "Python-runtime bundlad till $runtime"
Log "Total storlek: $sizeMb MB"

if ($sizeMb -gt 800) {
    Log-Warn "Runtime är $sizeMb MB — kontrollera om något onödigt drogs in."
}
