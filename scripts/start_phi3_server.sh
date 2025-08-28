#!/usr/bin/env bash
set -euo pipefail
export PHI3_MODEL_DIR="$(dirname "$0")/../models/phi3-mini-4k-onnx"
export PHI3_PORT=8009
python3 -m venv "$(dirname "$0")/../venv"
source "$(dirname "$0")/../venv/bin/activate"
pip install fastapi uvicorn onnxruntime-genai
python "$(dirname "$0")/../tools/phi3_server.py"
