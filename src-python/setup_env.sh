#!/bin/bash

echo "--- Setting up Python Sidecar Environment ---"

# 1. Create Virtual Environment
if [ ! -d ".venv" ]; then
    echo "Creating .venv..."
    python3 -m venv .venv
else
    echo ".venv already exists."
fi

# 2. Activate and Install
source .venv/bin/activate

echo "Upgrading pip..."
pip install --upgrade pip

echo "Installing dependencies..."
# For Mac Metal (M1/M2/M3) support:
# CMAKE_ARGS="-DGGML_METAL=on" pip install llama-cpp-python --force-reinstall --no-cache-dir
pip install -r requirements.txt

echo "--- Setup Complete! ---"
echo "To build: source .venv/bin/activate && pyinstaller --noconfirm --clean --onefile --distpath src-tauri/bin --name translator-x86_64-pc-windows-msvc --add-data ".venv/Lib/site-packages/llama_cpp;llama_cpp" main.py"