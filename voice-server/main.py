from __future__ import annotations

import asyncio
import io
import logging
import os
import re
import secrets
import threading
import uuid
from contextlib import asynccontextmanager
from pathlib import Path
from typing import Literal

import numpy as np
import soundfile as sf
import torch
from fastapi import FastAPI, Header, HTTPException
from fastapi.responses import Response
from pydantic import BaseModel, Field
from qwen_tts import Qwen3TTSModel

logging.basicConfig(level=os.getenv("LOG_LEVEL", "INFO"))
logger = logging.getLogger("cenai-voice")

MODEL_ID = os.getenv("CENAI_VOICE_MODEL_ID", "Qwen/Qwen3-TTS-12Hz-1.7B-Base")
VOICE_ID = "cenai-en-v1"
MAX_TEXT_CHARS = 8000
MAX_AUDIO_BYTES = 32 * 1024 * 1024
LANGUAGES = {
    "English": "English",
    "Chinese": "Chinese",
    "Japanese": "Japanese",
    "Korean": "Korean",
    "German": "German",
    "French": "French",
    "Russian": "Russian",
    "Portuguese": "Portuguese",
    "Spanish": "Spanish",
    "Italian": "Italian",
}


class SpeechRequest(BaseModel):
    input: str = Field(min_length=1, max_length=MAX_TEXT_CHARS)
    language: Literal[
        "English", "Chinese", "Japanese", "Korean", "German",
        "French", "Russian", "Portuguese", "Spanish", "Italian"
    ] = "English"
    voice: str = VOICE_ID
    response_format: Literal["wav"] = "wav"


@asynccontextmanager
async def lifespan(app: FastAPI):
    api_key = os.getenv("CENAI_TTS_API_KEY", "")
    reference_audio = Path(os.getenv("CENAI_VOICE_REFERENCE_AUDIO", "/run/secrets/cenai-voice-reference.wav"))
    reference_text = os.getenv("CENAI_VOICE_REFERENCE_TEXT", "").strip()
    if len(api_key) < 32:
        raise RuntimeError("CENAI_TTS_API_KEY must be a random secret of at least 32 characters.")
    if not reference_audio.is_file() or not reference_text:
        raise RuntimeError(
            "CenAI Original English voice is not configured. Mount a licensed voice-actor "
            "reference WAV and set CENAI_VOICE_REFERENCE_TEXT."
        )
    if not torch.cuda.is_available():
        raise RuntimeError("A CUDA GPU is required for the configured cloud inference service.")

    dtype = torch.bfloat16 if torch.cuda.is_bf16_supported() else torch.float16
    logger.info("Loading %s on the cloud GPU", MODEL_ID)
    model = await asyncio.to_thread(
        Qwen3TTSModel.from_pretrained,
        MODEL_ID,
        device_map="cuda:0",
        dtype=dtype,
        attn_implementation=os.getenv("CENAI_TTS_ATTENTION", "sdpa"),
    )
    logger.info("Preparing the fixed CenAI Original voice profile")
    prompt = await asyncio.to_thread(
        model.create_voice_clone_prompt,
        ref_audio=str(reference_audio),
        ref_text=reference_text,
        x_vector_only_mode=False,
    )
    app.state.model = model
    app.state.voice_prompt = prompt
    app.state.api_key = api_key
    app.state.inference_lock = threading.Lock()
    app.state.semaphore = asyncio.Semaphore(max(1, int(os.getenv("CENAI_TTS_MAX_CONCURRENCY", "1"))))
    app.state.ready = True
    logger.info("CenAI Voice cloud model is ready")
    yield
    app.state.ready = False
    app.state.model = None
    app.state.voice_prompt = None


app = FastAPI(title="CenAI Voice Inference", version="1.0.0", lifespan=lifespan)


@app.get("/healthz")
async def healthz():
    return {"service": "cenai-voice", "status": "alive"}


@app.get("/readyz")
async def readyz():
    if not getattr(app.state, "ready", False):
        raise HTTPException(status_code=503, detail="Voice model is not ready")
    return {"service": "cenai-voice", "status": "ready", "profile": VOICE_ID, "model": MODEL_ID}


def _synthesize(text: str, language: str) -> bytes:
    with app.state.inference_lock, torch.inference_mode():
        wavs, sample_rate = app.state.model.generate_voice_clone(
            text=text,
            language=LANGUAGES[language],
            voice_clone_prompt=app.state.voice_prompt,
            do_sample=True,
            top_k=50,
            top_p=0.9,
            temperature=0.75,
            repetition_penalty=1.05,
            max_new_tokens=min(4096, max(512, len(text) * 8)),
        )
    if not wavs:
        raise RuntimeError("Speech model returned no audio.")
    audio = np.asarray(wavs[0], dtype=np.float32).reshape(-1)
    if audio.size == 0 or not np.isfinite(audio).all():
        raise RuntimeError("Speech model returned invalid audio samples.")
    buffer = io.BytesIO()
    sf.write(buffer, audio, int(sample_rate), format="WAV", subtype="PCM_16")
    result = buffer.getvalue()
    if len(result) < 44 or len(result) > MAX_AUDIO_BYTES:
        raise RuntimeError("Generated audio exceeded the response size limit.")
    return result


@app.post("/v1/audio/speech")
async def speech(
    payload: SpeechRequest,
    authorization: str = Header(default=""),
    x_cenai_user_id: str = Header(default=""),
):
    expected = "Bearer " + getattr(app.state, "api_key", "")
    if not secrets.compare_digest(authorization, expected):
        raise HTTPException(status_code=401, detail="Invalid service credentials.")
    try:
        uuid.UUID(x_cenai_user_id)
    except (ValueError, AttributeError):
        raise HTTPException(status_code=401, detail="A verified CenAI user identity is required.")
    text = payload.input.strip()
    if not text or len(text) > MAX_TEXT_CHARS or any(ord(ch) < 32 and ch not in "\n\r\t" for ch in text):
        raise HTTPException(status_code=400, detail="Speech text is invalid or too long.")
    if payload.voice != VOICE_ID:
        raise HTTPException(status_code=404, detail="CenAI Voice profile not found.")
    if not getattr(app.state, "ready", False):
        raise HTTPException(status_code=503, detail="CenAI Voice is still starting.")
    try:
        async with app.state.semaphore:
            audio = await asyncio.to_thread(_synthesize, text, payload.language)
    except Exception:
        logger.exception("Speech inference failed for an authenticated user; input text is intentionally not logged.")
        raise HTTPException(status_code=503, detail="CenAI Voice could not generate speech right now.")
    return Response(
        content=audio,
        media_type="audio/wav",
        headers={
            "Cache-Control": "no-store",
            "X-Content-Type-Options": "nosniff",
            "X-CenAI-Voice-Profile": VOICE_ID,
        },
    )
