from __future__ import annotations

import os
from pathlib import Path

import soundfile as sf
import torch
from qwen_tts import Qwen3TTSModel

MODEL_ID = "Qwen/Qwen3-TTS-12Hz-1.7B-VoiceDesign"
OUTPUT = Path(os.getenv("CENAI_VOICE_REFERENCE_OUTPUT", "/secure/cenai-voice-reference.wav"))
REFERENCE_TEXT = (
    "Welcome to CenAI. I am here to help you think clearly, explore ideas, and get things done. "
    "We can work through a difficult question one step at a time, compare different possibilities, "
    "or turn a plan into something practical. I will keep the explanation clear and natural, "
    "with careful pronunciation, comfortable pacing, and a little warmth. When a detail matters, "
    "I will slow down and make it easier to follow. When the answer is simple, I will keep it direct. "
    "This is your space to create, learn, and make progress. Let's begin."
)
VOICE_INSTRUCTION = (
    "Create an original adult English voice for a premium, friendly AI assistant. "
    "Use a warm, clear, confident medium-range speaking voice with a natural contemporary "
    "American English accent, precise consonants, smooth connected speech, balanced vowels, "
    "subtle expressive intonation, and calm conversational pacing. Sound thoughtful and "
    "approachable, not like a radio announcer. Do not imitate any real person, celebrity, "
    "public figure, or recognizable fictional character."
)


def main() -> None:
    if not torch.cuda.is_available():
        raise SystemExit("A CUDA GPU is required to prepare the CenAI voice reference.")
    torch.manual_seed(4212026)
    dtype = torch.bfloat16 if torch.cuda.is_bf16_supported() else torch.float16
    model = Qwen3TTSModel.from_pretrained(
        MODEL_ID,
        device_map="cuda:0",
        dtype=dtype,
        attn_implementation=os.getenv("CENAI_TTS_ATTENTION", "sdpa"),
    )
    wavs, sample_rate = model.generate_voice_design(
        text=REFERENCE_TEXT,
        language="English",
        instruct=VOICE_INSTRUCTION,
        do_sample=True,
        top_k=50,
        top_p=0.9,
        temperature=0.65,
        repetition_penalty=1.05,
        max_new_tokens=4096,
    )
    if not wavs:
        raise SystemExit("Qwen VoiceDesign did not return a reference recording.")
    audio = wavs[0]
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    sf.write(OUTPUT, audio, int(sample_rate), format="WAV", subtype="PCM_16")
    transcript_path = OUTPUT.with_suffix(".txt")
    transcript_path.write_text(REFERENCE_TEXT + "\n", encoding="utf-8")
    print(f"Created AI-designed CenAI Original voice reference: {OUTPUT}")
    print(f"Exact reference transcript: {transcript_path}")
    print("Listen to and approve the generated voice before using it in a public release.")


if __name__ == "__main__":
    main()
