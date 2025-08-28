@echo off
setlocal
set PHI3_MODEL_DIR=%~dp0..\models\phi3-mini-4k-onnx
set PHI3_PORT=8009
python -m venv "%~dp0..\venv"
call "%~dp0..\venv\Scripts\activate"
pip install fastapi uvicorn onnxruntime-genai
python "%~dp0..\tools\phi3_server.py"
