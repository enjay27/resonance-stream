# src-python/setup_env.ps1

Write-Host "--- Setting up Python Sidecar Environment ---" -ForegroundColor Cyan

# 1. Check if Python is installed
if (-not (Get-Command "python" -ErrorAction SilentlyContinue)) {
    Write-Error "Python is not installed! Please install Python 3.10+ and add it to PATH."
    exit 1
}

# 2. Create Virtual Environment (.venv) if missing
if (-not (Test-Path ".venv")) {
    Write-Host "Creating .venv..."
    python -m venv .venv
} else {
    Write-Host ".venv already exists."
}

# 3. Activate .venv
Write-Host "Activating .venv..."
& ".\.venv\Scripts\Activate.ps1"

# 4. Upgrade pip
Write-Host "Upgrading pip..."
python -m pip install --upgrade pip

# 5. Install Requirements
# Note: For NVIDIA GPU support, you might need:
# $env:CMAKE_ARGS="-DGGML_CUDA=on"; pip install llama-cpp-python --force-reinstall --no-cache-dir
Write-Host "Installing dependencies from requirements.txt..."
pip install -r requirements.txt

Write-Host "--- Setup Complete! ---" -ForegroundColor Green
Write-Host "To build the binary, run:"
Write-Host "  .venv\Scripts\activate"
Write-Host "  pyinstaller --clean --onefile --distpath src-tauri/bin --name translator-x86_64-pc-windows-msvc src-python/main.py"