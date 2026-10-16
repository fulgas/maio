"""ACE-Step 1.5 backend. API facts verified in docs/ace-step-spike.md."""

import shutil
import tempfile
import wave
from pathlib import Path

from .server import GenRequest, JobError


class AceStepBackend:
    def __init__(self, root: Path, model: str, device: str, steps: int):
        self.root, self.model, self.device, self.steps = root, model, device, steps
        self.info = {
            "id": "ace-step",
            "name": "ACE-Step",
            "version": "1.5",
            "licence": {"name": "MIT", "commercial": "yes",
                        "url": "https://github.com/ACE-Step/ACE-Step-1.5/blob/main/LICENSE", "note": ""},
            "capabilities": {"min_seconds": 10, "max_seconds": 600, "vocals": True, "tempo": True, "concurrency": 1},
            "realtime_factor": 0.5,  # first guess; the server measures the real value per job
        }

    def load(self) -> None:
        from acestep.handler import AceStepHandler
        from acestep.llm_inference import LLMHandler

        self.dit = AceStepHandler()
        message, ok = self.dit.initialize_service(project_root=str(self.root), config_path=self.model, device=self.device)
        if not ok:
            raise RuntimeError(message)
        self.llm = LLMHandler()  # initialised on first need (_planner); thinking stays off

    def _planner(self):
        """ACE-Step's planning model, loaded the first time a job needs it (SPEC 10, Lyrics)."""
        if not getattr(self.llm, "llm_initialized", False):
            message, ok = self.llm.initialize(checkpoint_dir=str(self.root / "checkpoints"),
                                              lm_model_path="acestep-5Hz-lm-1.7B", backend="pt", device=self.device)
            if not ok:
                raise JobError("planner_unavailable", message)
        return self.llm

    def generate(self, req: GenRequest, out: Path) -> tuple[float, dict]:
        from acestep.inference import GenerationConfig, GenerationParams, create_sample, generate_music

        lyrics, metas = req.lyrics or "", {}
        if req.vocals and req.write_lyrics:
            plan = create_sample(self._planner(), req.prompt, instrumental=False, vocal_language=req.vocal_language)
            if not plan.success:
                raise JobError("lyrics_failed", str(plan.error or plan.status_message))
            lyrics = plan.lyrics
            metas = {k: v for k, v in {"bpm": plan.bpm, "key": plan.keyscale, "time_signature": plan.timesignature}.items() if v}
        kwargs = dict(caption=req.prompt, lyrics=lyrics, bpm=req.bpm or metas.get("bpm"), duration=req.seconds,
                      seed=req.seed, inference_steps=self.steps, instrumental=not req.vocals,
                      vocal_language=req.vocal_language if req.vocals else "unknown",
                      keyscale=metas.get("key", ""), timesignature=metas.get("time_signature", ""))
        if "thinking" in GenerationParams.__dataclass_fields__:
            kwargs["thinking"] = False
        params = GenerationParams(**kwargs)
        config = GenerationConfig(batch_size=1, audio_format="wav")
        with tempfile.TemporaryDirectory(dir=out.parent) as tmp:
            try:
                result = generate_music(self.dit, self.llm, params, config, save_dir=tmp)
            except RuntimeError as e:
                if "out of memory" in str(e).lower():
                    raise JobError("oom", str(e)) from e
                raise
            if not result.success:
                raise JobError("generation_failed", str(result.error))
            shutil.move(result.audios[0]["path"], out)
        with wave.open(str(out)) as w:
            seconds = w.getnframes() / w.getframerate()
        if req.vocals and req.write_lyrics:
            return seconds, {"lyrics": lyrics, "metas": metas}
        return seconds, {"lyrics": lyrics} if req.vocals else {}
