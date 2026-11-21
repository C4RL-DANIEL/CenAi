# CenAI Voice

CenAI Voice is CenAI's branded speech product built on the open-source Qwen3-TTS model family.

## Product identity and attribution

- Product name: **CenAI Voice**
- Upstream model family: Qwen3-TTS
- Upstream source: https://github.com/QwenLM/Qwen3-TTS
- Upstream project licence: Apache-2.0. Retain required copyright, licence and NOTICE material when redistributing upstream code or model artifacts.
- CenAI owns its app experience, voice profile names, pronunciation policy, local model management and integration code. CenAI does not claim authorship of Qwen's model architecture or pretrained weights.
- Verify the licence and terms of each exact checkpoint and tokenizer artifact before bundling or distributing it. Do not remove upstream attribution.

## Model choice

Primary fine-tuning source:

`Qwen/Qwen3-TTS-12Hz-0.6B-Base`

Tokenizer:

`Qwen/Qwen3-TTS-Tokenizer-12Hz`

The 0.6B Base checkpoint is the selected starting point because Qwen documents Base models as supporting voice cloning and single-speaker fine-tuning. The initial CenAI target profile is `cenai_voice_en_v1` (English, `en-US`). The 0.6B CustomVoice checkpoint remains a baseline for quality comparisons; it is not the final CenAI voice identity. The 1.7B VoiceDesign checkpoint is an optional research/reference model and is not the Android default.

Qwen documents support for ten languages: Chinese, English, Japanese, Korean, German, French, Russian, Portuguese, Spanish and Italian. English is the first quality gate. Language availability is not a guarantee of equal pronunciation quality; each language must pass its own test set before being labelled production-ready. Filipino/Tagalog is not in the documented ten-language list and must not be advertised as natively supported by this model.

## Offline and Android requirements

The product requirement is offline-only synthesis after model installation:

- no Google TTS, Android system TTS, Gemini TTS, DashScope API, or cloud fallback;
- no audio or text leaves the device during local synthesis;
- model packs are downloaded explicitly, verified, stored in app-private storage, and usable after disconnecting the network;
- users can remove a model pack and see its disk usage;
- voice playback is cancelled on user request, app lifecycle shutdown, or account/session invalidation.

**Runtime status:** the Rust contract and product selection are implemented, but a Qwen-compatible Android ARM64 inference backend is not yet integrated. Qwen's upstream quickstart uses a Python/PyTorch runtime and its documented local deployment paths are not an Android NativeActivity runtime. Do not mark offline playback as working until a supported export/backend has been proven on Android. The implementation must benchmark cold start, peak RAM, real-time factor, audio quality, thermal load and sustained playback on the REDMI Pad 2 Pro class of hardware. No cloud service may be used as a substitute.

## Voice identity and fine-tuning

The intended CenAI voice is an original, consistent English speaker profile, not a copy of a celebrity or another identifiable person. Use recordings from a voice actor who has explicitly consented to model training and commercial distribution. Keep dataset provenance, consent, transcript and licence records.

Qwen's current fine-tuning guide documents single-speaker fine-tuning for the 12Hz Base family. Its JSONL training records contain `audio`, `text`, and `ref_audio`; the preparation step extracts tokenizer audio codes, then `sft_12hz.py` fine-tunes the selected Base checkpoint.

Example starting workflow (run on a compatible cloud GPU; standard GitHub-hosted runners are not assumed to have GPUs):

```bash
git clone https://github.com/QwenLM/Qwen3-TTS.git
cd Qwen3-TTS
pip install -U qwen-tts
cd finetuning

python prepare_data.py \
  --device cuda:0 \
  --tokenizer_model_path Qwen/Qwen3-TTS-Tokenizer-12Hz \
  --input_jsonl train_raw.jsonl \
  --output_jsonl train_with_codes.jsonl

python sft_12hz.py \
  --init_model_path Qwen/Qwen3-TTS-12Hz-0.6B-Base \
  --output_model_path output/cenai-voice-en-v1 \
  --train_jsonl train_with_codes.jsonl \
  --batch_size 2 \
  --lr 2e-5 \
  --num_epochs 3 \
  --speaker_name cenai_voice_en_v1
```

This is a reproducible starting recipe, not a guaranteed optimal training configuration. Tune batch size and learning rate for the actual GPU and dataset. Keep validation/test utterances separate, inspect checkpoints, and never automatically promote a checkpoint just because training completed.

## English-first quality gate

Before release, evaluate:

1. Word error rate using independent ASR only as a measurement tool (not as a synthesis dependency).
2. Human intelligibility and naturalness ratings from native English listeners.
3. Pronunciation of names, abbreviations, numbers, dates, currencies, units, URLs and technical terms.
4. Sentence stress, punctuation pauses, long-form consistency and expressive control.
5. Voice identity consistency across short and long passages.
6. Cold-start latency, real-time factor, peak memory, thermal behaviour and interrupted playback on supported Android devices.

Keep the reference text set and scoring scripts versioned. Publish per-language quality status rather than one blanket "multilingual accurate" claim.

## Rust integration contract

`native/src/core/qwen_voice.rs` owns stable model metadata, the supported-language catalogue, the voice profile and fail-closed runtime status. The eventual inference backend must implement this contract without changing chat/provider authorization. The model pack must be independently versioned from the APK.

## Current implementation status

- [x] CenAI Voice product identity and Qwen upstream attribution policy.
- [x] Select Qwen3-TTS 12Hz 0.6B Base as the fine-tuning source.
- [x] English-first profile and ten-language metadata.
- [x] Offline-only/no-fallback contract and validation tests.
- [x] Fine-tuning workflow documentation.
- [ ] Acquire/record licensed English voice dataset and signed consent.
- [ ] Train and evaluate the CenAI English voice checkpoint.
- [ ] Select and validate a compatible ARM64 inference backend/export.
- [ ] Implement verified model-pack download, integrity checking, storage and deletion.
- [ ] Integrate native inference and PCM playback.
- [ ] Pass offline and performance tests on physical Android hardware.
