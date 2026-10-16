# ACE-Step 1.5 spike (Apple M3 Max, 64 GB)

Run on 2026-10-15 against ACE-Step-1.5 commit `ca1e85f`, Python 3.12, uv 0.13.1.

- Package name in pyproject: `ace-step` (import name `acestep`), version 1.5.0, requires Python `>=3.11,<3.13`.
- GenerationParams fields include: `caption`, `lyrics`, `instrumental`, `bpm`, `duration`, `seed`, `inference_steps`, `thinking`, `use_cot_metas`, `use_cot_caption`, `use_cot_language` (78 fields in all).
- GenerationConfig fields: `batch_size`, `allow_lm_batch`, `use_random_seed`, `seeds`, `lm_batch_chunk_size`, `constrained_decoding_debug`, `audio_format`, `mp3_bitrate`, `mp3_sample_rate`.
- `generate_music(dit_handler, llm_handler, params, config, save_dir=None, progress=None) -> GenerationResult`.
- `AceStepHandler.initialize_service(project_root, config_path, device="auto", ..., use_mlx_dit=True)` returns `(message, ok)`; the caller must check `ok`.
- `thinking` field present: yes.
- Uninitialised LLMHandler works with thinking off: yes. The log shows `llm_initialized=False, use_lm=False`; the COT options stay on but are skipped.
- Checkpoints downloaded to: `<ACE-Step root>/checkpoints` on first `initialize_service` (9.4 GB: `acestep-v15-turbo`, `vae`, `Qwen3-Embedding-0.6B`, `acestep-5Hz-lm-1.7B`).
- Model load time: 92 s on first run, including the download.
- 60 s generation time: 30.4 s first (warm-up), 11.8 s second; realtime factor 0.51 cold, about 0.20 warm.
- Output: WAV, 48 kHz stereo, 60.0 s, mean volume -17.7 dB, peak -1.0 dB.
- Install note: ACE-Step's pyproject points `nano-vllm` at a local path through `[tool.uv.sources]` (Linux only), so installing it as a git dependency may fail; the fallback is an editable install from the local checkout.
