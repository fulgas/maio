import sys
import types
import wave
from pathlib import Path
from types import SimpleNamespace

from maio_engine.server import GenRequest


def write_wav(path: Path, seconds: int) -> None:
    with wave.open(str(path), "wb") as w:
        w.setnchannels(2)
        w.setsampwidth(2)
        w.setframerate(48000)
        w.writeframes(b"\0\0\0\0" * 48000 * seconds)


def stub_acestep(monkeypatch, calls: dict, success: bool = True, init_ok: bool = True):
    class Params:
        __dataclass_fields__ = {"thinking": None}

        def __init__(self, **kw):
            calls["params"] = kw

    class Config:
        def __init__(self, **kw):
            calls["config"] = kw

    def generate_music(dit, llm, params, config, save_dir):
        path = Path(save_dir) / "out.wav"
        write_wav(path, 1)
        return SimpleNamespace(success=success, audios=[{"path": str(path)}], error=None if success else "bad input")

    class Handler:
        def initialize_service(self, **kw):
            calls["init"] = kw
            return ("[OK] ready" if init_ok else "[ERR] weights missing", init_ok)

    def create_sample(llm, query, instrumental=False, vocal_language=None, **kw):
        calls["sample"] = {"query": query, "instrumental": instrumental, "vocal_language": vocal_language}
        return SimpleNamespace(success=True, lyrics="[verse]\nplanned\n[chorus]\nplanned chorus", bpm=88,
                               keyscale="D minor", timesignature="3", error=None, status_message="")

    class LLM:
        llm_initialized = False

        def initialize(self, **kw):
            calls["lm_init"] = calls.get("lm_init", 0) + 1
            self.llm_initialized = True
            return ("[OK]", True)

    inference = types.ModuleType("acestep.inference")
    inference.GenerationParams, inference.GenerationConfig, inference.generate_music = Params, Config, generate_music
    inference.create_sample = create_sample
    handler = types.ModuleType("acestep.handler")
    handler.AceStepHandler = Handler
    llm = types.ModuleType("acestep.llm_inference")
    llm.LLMHandler = LLM
    for name, mod in {"acestep": types.ModuleType("acestep"), "acestep.inference": inference,
                      "acestep.handler": handler, "acestep.llm_inference": llm}.items():
        monkeypatch.setitem(sys.modules, name, mod)


def test_maps_request_to_ace_step(tmp_path, monkeypatch):
    calls = {}
    stub_acestep(monkeypatch, calls)
    from maio_engine.ace_step import AceStepBackend

    backend = AceStepBackend(tmp_path, "acestep-v15-turbo", "mps", 8)
    backend.load()
    out = tmp_path / "job.wav"
    seconds, extra = backend.generate(GenRequest(prompt="calm lo-fi, instrumental", seconds=30, bpm=75, seed=42), out)

    assert calls["init"] == {"project_root": str(tmp_path), "config_path": "acestep-v15-turbo", "device": "mps"}
    assert calls["params"] == {"caption": "calm lo-fi, instrumental", "lyrics": "", "bpm": 75, "duration": 30,
                               "seed": 42, "inference_steps": 8, "instrumental": True, "thinking": False,
                               "vocal_language": "unknown", "keyscale": "", "timesignature": ""}
    assert calls["config"] == {"batch_size": 1, "audio_format": "wav"}
    assert seconds == 1.0 and out.exists()
    assert extra == {}


def test_unsuccessful_generation_raises_job_error(tmp_path, monkeypatch):
    import pytest

    from maio_engine.server import JobError

    stub_acestep(monkeypatch, {}, success=False)
    from maio_engine.ace_step import AceStepBackend

    backend = AceStepBackend(tmp_path, "m", "mps", 8)
    backend.load()
    with pytest.raises(JobError) as err:
        backend.generate(GenRequest(prompt="x", seconds=10, seed=1), tmp_path / "o.wav")
    assert err.value.code == "generation_failed"


def test_failed_initialisation_raises(tmp_path, monkeypatch):
    import pytest

    stub_acestep(monkeypatch, {}, init_ok=False)
    from maio_engine.ace_step import AceStepBackend

    with pytest.raises(RuntimeError, match="weights missing"):
        AceStepBackend(tmp_path, "m", "mps", 8).load()


def test_write_lyrics_runs_the_planning_model_once_and_sings_its_lyrics(tmp_path, monkeypatch):
    calls = {}
    stub_acestep(monkeypatch, calls)
    from maio_engine.ace_step import AceStepBackend

    backend = AceStepBackend(tmp_path, "acestep-v15-turbo", "mps", 8)
    backend.load()
    req = GenRequest(prompt="rainy bus, sad", seconds=30, vocals=True, write_lyrics=True, vocal_language="pt", seed=3)
    seconds, extra = backend.generate(req, tmp_path / "a.wav")
    backend.generate(req, tmp_path / "b.wav")
    assert calls["lm_init"] == 1, "the planning model loads on first need only"
    assert calls["sample"] == {"query": "rainy bus, sad", "instrumental": False, "vocal_language": "pt"}
    assert calls["params"]["lyrics"] == "[verse]\nplanned\n[chorus]\nplanned chorus"
    assert (calls["params"]["keyscale"], calls["params"]["timesignature"], calls["params"]["vocal_language"]) == ("D minor", "3", "pt")
    assert extra == {"lyrics": "[verse]\nplanned\n[chorus]\nplanned chorus", "metas": {"bpm": 88, "key": "D minor", "time_signature": "3"}}
