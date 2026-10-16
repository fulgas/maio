"""HTTP engine server implementing the Maio engine contract (SPEC.md section 4)."""

from __future__ import annotations

import hmac
import queue
import threading
import time
import uuid
from dataclasses import dataclass, field
from pathlib import Path
from typing import Protocol

from fastapi import Depends, FastAPI, Header, HTTPException
from fastapi.responses import FileResponse, JSONResponse, Response
from pydantic import BaseModel, ConfigDict, Field

PROTOCOL = 1


class GenRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    prompt: str = Field(min_length=1, max_length=2000)
    seconds: int
    bpm: int | None = Field(default=None, ge=30, le=300)
    vocals: bool = False
    lyrics: str | None = None
    seed: int
    vocal_language: str = Field(default="en", min_length=2, max_length=12)
    write_lyrics: bool = False


class JobIn(BaseModel):
    model_config = ConfigDict(extra="forbid")
    request: GenRequest


class JobError(Exception):
    """A generation failure the backend can describe. Anything else becomes code `internal`."""

    def __init__(self, code: str, message: str, retryable: bool = False):
        super().__init__(message)
        self.code, self.message, self.retryable = code, message, retryable


class Backend(Protocol):
    info: dict

    def load(self) -> None: ...

    def generate(self, req: GenRequest, out: Path) -> float | tuple[float, dict]: ...


@dataclass
class Job:
    id: str
    request: GenRequest
    state: str = "queued"
    started: float | None = None
    finished: float | None = None
    seconds: float | None = None
    error: dict | None = None
    cancelled: bool = False
    path: Path | None = None
    extra: dict = field(default_factory=dict)


class Engine:
    def __init__(self, backend: Backend, work_dir: Path, max_queue: int, job_ttl_s: float):
        self.backend = backend
        self.work_dir = work_dir
        self.max_queue = max_queue
        self.job_ttl_s = job_ttl_s
        self.rtf = float(backend.info["realtime_factor"])
        self.status = "loading"
        self.load_error = ""
        self.jobs: dict[str, Job] = {}
        self.lock = threading.Lock()
        self.todo: queue.Queue[str] = queue.Queue()
        work_dir.mkdir(parents=True, exist_ok=True)

    def start(self) -> None:
        threading.Thread(target=self._load, daemon=True).start()
        for _ in range(self.backend.info["capabilities"]["concurrency"]):
            threading.Thread(target=self._worker, daemon=True).start()

    def _load(self) -> None:
        try:
            self.backend.load()
            self.status = "ok"
        except Exception as e:  # noqa: BLE001 - any load failure is reported through /health
            self.load_error = str(e)
            self.status = "error"

    def _worker(self) -> None:
        while True:
            job_id = self.todo.get()
            while self.status == "loading":
                time.sleep(0.1)
            with self.lock:
                job = self.jobs.get(job_id)
                if job is None or job.state != "queued":
                    continue
                job.state, job.started = "running", time.monotonic()
            out = self.work_dir / f"{job.id}.wav"
            seconds, error, extra = None, None, {}
            try:
                if self.status == "error":
                    raise JobError("model_not_loaded", self.load_error)
                result = self.backend.generate(job.request, out)
                seconds, extra = result if isinstance(result, tuple) else (result, {})
            except JobError as e:
                error = {"code": e.code, "message": e.message, "retryable": e.retryable}
            except Exception as e:  # noqa: BLE001 - keep the worker alive for the next job
                error = {"code": "internal", "message": str(e), "retryable": False}
            self._finish(job, out, seconds, error, extra)

    def _finish(self, job: Job, out: Path, seconds: float | None, error: dict | None, extra: dict) -> None:
        with self.lock:
            job.finished = time.monotonic()
            if job.cancelled:
                job.state = "cancelled"
                out.unlink(missing_ok=True)
            elif error:
                job.state, job.error = "failed", error
                out.unlink(missing_ok=True)
            else:
                job.state, job.seconds, job.path = "done", seconds, out
                job.extra = {k: v for k, v in extra.items() if v}  # lyrics and metas, when the backend knows them
                measured = (job.finished - job.started) / max(seconds, 1.0)
                self.rtf = 0.7 * self.rtf + 0.3 * measured

    def sweep(self) -> None:
        """Drops finished jobs older than the TTL, with their files."""
        now = time.monotonic()
        with self.lock:
            for job_id, job in list(self.jobs.items()):
                if job.finished is not None and now - job.finished >= self.job_ttl_s:
                    if job.path:
                        job.path.unlink(missing_ok=True)
                    del self.jobs[job_id]

    def view(self, job: Job) -> dict:
        body = {"job_id": job.id, "state": job.state}
        if job.state == "running":
            expected = max(self.rtf * job.request.seconds, 0.001)
            body["progress"] = round(min(0.95, (time.monotonic() - job.started) / expected), 3)
        elif job.state == "done":
            body |= {"seconds": job.seconds, "seed": job.request.seed} | job.extra
        elif job.state == "failed":
            body["error"] = job.error
        return body


def create_app(backend: Backend, token: str, work_dir: Path, max_queue: int = 4, job_ttl_s: float = 3600) -> FastAPI:
    expected = f"Bearer {token}".encode()

    def check_token(authorization: str | None = Header(default=None)) -> None:
        if authorization is None or not hmac.compare_digest(authorization.encode(), expected):
            raise HTTPException(401, "missing or wrong token")

    engine = Engine(backend, work_dir, max_queue, job_ttl_s)
    engine.start()
    app = FastAPI(dependencies=[Depends(check_token)])

    def get_job(job_id: str) -> Job:
        job = engine.jobs.get(job_id)
        if job is None:
            raise HTTPException(404, "no such job")
        return job

    @app.get("/info")
    def info() -> dict:
        return {"protocol": PROTOCOL, **backend.info, "realtime_factor": round(engine.rtf, 3)}

    @app.get("/health")
    def health():
        if engine.status == "ok":
            return {"status": "ok"}
        return JSONResponse({"status": engine.status}, status_code=503)

    @app.post("/jobs", status_code=202)
    def create(body: JobIn) -> dict:
        caps, r = backend.info["capabilities"], body.request
        if not caps["min_seconds"] <= r.seconds <= caps["max_seconds"]:
            raise HTTPException(422, f"seconds must be {caps['min_seconds']} to {caps['max_seconds']}")
        if r.vocals and not caps["vocals"]:
            raise HTTPException(422, "this engine cannot sing")
        if r.lyrics and not r.vocals:
            raise HTTPException(422, "lyrics need vocals")
        if r.write_lyrics and not r.vocals:
            raise HTTPException(422, "write_lyrics needs vocals")
        engine.sweep()
        with engine.lock:
            if sum(j.state == "queued" for j in engine.jobs.values()) >= engine.max_queue:
                raise HTTPException(429, "queue full")
            job = Job(id=uuid.uuid4().hex, request=r)
            engine.jobs[job.id] = job
        engine.todo.put(job.id)
        return {"job_id": job.id}

    @app.get("/jobs/{job_id}")
    def status(job_id: str) -> dict:
        with engine.lock:
            return engine.view(get_job(job_id))

    @app.get("/jobs/{job_id}/audio")
    def audio(job_id: str):
        job = get_job(job_id)
        if job.state != "done" or job.path is None:
            raise HTTPException(409, f"job is {job.state}")
        return FileResponse(job.path, media_type="audio/wav")

    @app.delete("/jobs/{job_id}", status_code=204)
    def delete(job_id: str) -> Response:
        with engine.lock:
            job = get_job(job_id)
            if job.state == "queued":
                job.state, job.cancelled, job.finished = "cancelled", True, time.monotonic()
            elif job.state == "running":
                job.cancelled = True
            else:
                if job.path:
                    job.path.unlink(missing_ok=True)
                del engine.jobs[job_id]
        return Response(status_code=204)

    return app
