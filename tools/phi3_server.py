import os
from fastapi import FastAPI
from pydantic import BaseModel
from fastapi.middleware.cors import CORSMiddleware
import uvicorn

try:
    from onnxruntime_genai import Model, Tokenizer, Generator
    MODEL_OK = True
except Exception as e:
    Model = Tokenizer = Generator = None
    MODEL_OK = False
    print("onnxruntime-genai not available:", e)

MODEL_DIR = os.getenv("PHI3_MODEL_DIR", "./models/phi3-mini-4k-onnx")
model = None
tokenizer = None
if MODEL_OK:
    model = Model(MODEL_DIR)
    tokenizer = Tokenizer(MODEL_DIR)

def generate_text(system: str, prompt: str, temperature: float, top_p: float, max_tokens: int) -> str:
    if not MODEL_OK:
        return '{"action":"Hold","confidence":0.33,"reason":"sidecar_mock"}'
    full = f"<|system|>{system}<|user|>{prompt}<|assistant|>"
    generator = Generator(model, tokenizer, **{"temperature": temperature, "top_p": top_p, "max_length": max_tokens})
    return generator(full)

class GenReq(BaseModel):
    system: str
    prompt: str
    temperature: float
    top_p: float
    max_tokens: int

class GenResp(BaseModel):
    text: str

app = FastAPI()
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_methods=["*"], allow_headers=["*"])

@app.post("/generate", response_model=GenResp)
def generate(req: GenReq):
    text = generate_text(req.system, req.prompt, req.temperature, req.top_p, req.max_tokens)
    return GenResp(text=text)

if __name__ == "__main__":
    port = int(os.getenv("PHI3_PORT", "8009"))
    uvicorn.run(app, host="127.0.0.1", port=port)
