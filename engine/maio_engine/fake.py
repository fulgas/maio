"""A backend that writes a sine tone instead of music, for tests and for running Maio without a GPU."""

import array
import math
import time
import wave
from pathlib import Path

from .server import GenRequest, JobError

RATE = 48_000


class FakeBackend:
    def __init__(self, rtf: float = 0.05, load_s: float = 0.0, vocals: bool = False):
        self.rtf, self.load_s, self.vocals = rtf, load_s, vocals
        self.info = {
            "id": "fake",
            "name": "Fake tone",
            "version": "1",
            "licence": {"name": "CC0-1.0", "commercial": "yes", "url": "", "note": "test tone"},
            "capabilities": {"min_seconds": 1, "max_seconds": 600, "vocals": vocals, "tempo": True, "concurrency": 1},
            "realtime_factor": rtf,
        }

    def load(self) -> None:
        time.sleep(self.load_s)

    def generate(self, req: GenRequest, out: Path) -> tuple[float, dict]:
        if "FAIL" in req.prompt:
            raise JobError("fake_failure", "prompt asked for a failure", retryable="RETRY" in req.prompt)
        time.sleep(req.seconds * self.rtf)
        freq = 220 + req.seed % 440  # whole hertz, so one second of samples loops cleanly
        one_second = array.array("h")
        for i in range(RATE):
            v = int(6000 * math.sin(2 * math.pi * freq * i / RATE))
            one_second.extend((v, v))
        data = one_second.tobytes()  # little-endian on arm64 and x86_64, as WAV needs
        with wave.open(str(out), "wb") as w:
            w.setnchannels(2)
            w.setsampwidth(2)
            w.setframerate(RATE)
            for _ in range(req.seconds):
                w.writeframes(data)
        if req.write_lyrics:
            return float(req.seconds), {"lyrics": "[verse]\nfake verse\n[chorus]\nfake chorus",
                                        "metas": {"bpm": 92, "key": "C major", "time_signature": "4"}}
        return float(req.seconds), {"lyrics": req.lyrics} if req.vocals else {}
