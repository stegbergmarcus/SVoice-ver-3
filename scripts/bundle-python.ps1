# bundle-python.ps1 -- Bundle Python runtime + faster-whisper + CUDA 12 DLLs
# for SVoice 3 release build.
#
# Result: src-tauri/resources/python-runtime/python/ with everything needed.
#
# Run manually before `cargo tauri build` if python-runtime is missing or
# needs updating. Idempotent: skips steps already done.
#
# Note: plain ASCII only. Windows PowerShell 5.1 reads script files with
# the console's OEM codepage (not UTF-8), so non-ASCII characters break
# parsing when the file has no BOM.

$ErrorActionPreference = "Stop"
$root = Resolve-Path (Join-Path $PSScriptRoot "..")
$runtime = Join-Path $root "src-tauri\resources\python-runtime"
$pythonDir = Join-Path $runtime "python"

$PythonVersion = "3.11.9"
$embedZipName = "python-$PythonVersion-embed-amd64.zip"
$embedUrl = "https://www.python.org/ftp/python/$PythonVersion/$embedZipName"

# Pip packages installed into the runtime.
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

# 1. Fetch embeddable Python if not already present
if (Test-Path (Join-Path $pythonDir "python.exe")) {
    Log "Python embeddable already extracted to $pythonDir -- skipping download."
} else {
    Log "Downloading Python $PythonVersion embeddable..."
    New-Item -ItemType Directory -Force -Path $runtime | Out-Null
    $embedZip = Join-Path $runtime $embedZipName
    Invoke-WebRequest $embedUrl -OutFile $embedZip
    Log "Extracting to $pythonDir..."
    Expand-Archive $embedZip -DestinationPath $pythonDir -Force
    Remove-Item $embedZip
}

# 2. Uncomment `import site` in python311._pth so pip + site-packages work.
#    Embeddable default is to comment it out, which breaks pip.
$pthFile = Join-Path $pythonDir "python311._pth"
if (Test-Path $pthFile) {
    $pthContent = Get-Content $pthFile -Raw
    if ($pthContent -match "(?m)^#\s*import\s+site\s*$") {
        Log "Enabling 'import site' in python311._pth..."
        $pthContent = $pthContent -replace "(?m)^#\s*import\s+site\s*$", "import site"
        Set-Content $pthFile $pthContent -NoNewline
    } else {
        Log "'import site' already active in python311._pth."
    }
} else {
    Log-Warn "python311._pth missing -- embeddable distribution layout changed?"
}

# 3. Bootstrap pip (not included in embeddable)
$pipModule = Join-Path $pythonDir "Lib\site-packages\pip"
if (Test-Path $pipModule) {
    Log "pip already installed."
} else {
    Log "Bootstrapping pip via get-pip.py..."
    $getPip = Join-Path $runtime "get-pip.py"
    Invoke-WebRequest "https://bootstrap.pypa.io/get-pip.py" -OutFile $getPip
    & (Join-Path $pythonDir "python.exe") $getPip
    if ($LASTEXITCODE -ne 0) {
        Write-Error "get-pip.py failed (exit $LASTEXITCODE)."
    }
    Remove-Item $getPip
}

# 4. Install pip packages
Log "Installing Python packages into bundled runtime..."
$pkgArgs = @("-m", "pip", "install", "--upgrade") + $packages
& (Join-Path $pythonDir "python.exe") @pkgArgs
if ($LASTEXITCODE -ne 0) {
    Write-Error "pip install failed (exit $LASTEXITCODE)."
}

# 5. Size report
$sizeBytes = (Get-ChildItem $runtime -Recurse -ErrorAction SilentlyContinue |
    Measure-Object -Property Length -Sum).Sum
$sizeMb = [math]::Round($sizeBytes / 1MB, 1)
Log "Python runtime bundled to $runtime"
Log "Total size: $sizeMb MB"

if ($sizeMb -gt 800) {
    Log-Warn "Runtime is $sizeMb MB -- check whether something unnecessary was pulled in."
}
