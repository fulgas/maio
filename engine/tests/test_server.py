import time
import wave

import pytest
from fastapi.testclient import TestClient

from maio_engine.fake import FakeBackend
from maio_engine.server import create_app

TOKEN = "secret"
AUTH = {"Authorization": f"Bearer {TOKEN}"}


def make_client(tmp_path, backend=None, **kw):
    return TestClient(create_app(backend or FakeBackend(rtf=0.001), TOKEN, tmp_path, **kw))


def wait_for(client, job_id, states, timeout=5.0):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        body = client.get(f"/jobs/{job_id}", headers=AUTH).json()
        if body["state"] in states:
            return body
        time.sleep(0.02)
    raise AssertionError(f"job {job_id} never reached {states}, last {body}")


def wait_healthy(client, timeout=5.0):
    deadline = time.monotonic() + timeout
    while client.get("/health", headers=AUTH).status_code != 200:
        assert time.monotonic() < deadline, "engine never became healthy"
        time.sleep(0.02)


def post_job(client, seconds=2, prompt="calm", **extra):
    return client.post("/jobs", headers=AUTH, json={"request": {"prompt": prompt, "seconds": seconds, "seed": 7, **extra}})


@pytest.mark.parametrize("path", ["/info", "/health", "/jobs/x", "/jobs/x/audio"])
def test_every_endpoint_requires_the_token(tmp_path, path):
    client = make_client(tmp_path)
    assert client.get(path).status_code == 401
    assert client.get(path, headers={"Authorization": "Bearer wrong"}).status_code == 401


def test_post_and_delete_require_the_token(tmp_path):
    client = make_client(tmp_path)
    assert client.post("/jobs", json={"request": {"prompt": "x", "seconds": 2, "seed": 1}}).status_code == 401
    assert client.delete("/jobs/x").status_code == 401


def test_info_reports_protocol_licence_and_capabilities(tmp_path):
    body = make_client(tmp_path).get("/info", headers=AUTH).json()
    assert body["protocol"] == 1
    assert body["id"] == "fake"
    assert body["licence"]["commercial"] == "yes"
    assert body["capabilities"]["max_seconds"] == 600
    assert "realtime_factor" in body


def test_health_reports_loading_then_ok(tmp_path):
    client = make_client(tmp_path, FakeBackend(rtf=0.001, load_s=0.3))
    first = client.get("/health", headers=AUTH)
    assert first.status_code == 503 and first.json() == {"status": "loading"}
    wait_healthy(client)
    assert client.get("/health", headers=AUTH).json() == {"status": "ok"}


def test_job_lifecycle_writes_a_wav(tmp_path):
    client = make_client(tmp_path)
    created = post_job(client, seconds=2)
    assert created.status_code == 202
    job_id = created.json()["job_id"]
    body = wait_for(client, job_id, {"done"})
    assert body["seconds"] == 2.0 and body["seed"] == 7
    audio = client.get(f"/jobs/{job_id}/audio", headers=AUTH)
    assert audio.status_code == 200 and audio.headers["content-type"] == "audio/wav"
    path = tmp_path / "check.wav"
    path.write_bytes(audio.content)
    with wave.open(str(path)) as w:
        assert (w.getnchannels(), w.getframerate(), w.getnframes()) == (2, 48000, 96000)


def test_rejects_requests_outside_capabilities(tmp_path):
    client = make_client(tmp_path)
    assert post_job(client, seconds=0).status_code == 422
    assert post_job(client, seconds=601).status_code == 422
    assert post_job(client, vocals=True).status_code == 422  # fake cannot sing
    assert post_job(client, lyrics="la la").status_code == 422  # lyrics need vocals
    assert client.post("/jobs", headers=AUTH, json={"request": {"prompt": "x", "seconds": 2, "seed": 1, "bogus": 1}}).status_code == 422


def test_unknown_job_is_404(tmp_path):
    assert make_client(tmp_path).get("/jobs/nope", headers=AUTH).status_code == 404


import threading


class BlockingBackend(FakeBackend):
    """Holds every job in generate() until released, so tests control timing."""

    def __init__(self):
        super().__init__(rtf=0)
        self.release = threading.Event()

    def generate(self, req, out):
        self.release.wait(5)
        return super().generate(req, out)


class CrashOnceBackend(FakeBackend):
    def __init__(self):
        super().__init__(rtf=0)
        self.crashed = False

    def generate(self, req, out):
        if not self.crashed:
            self.crashed = True
            raise RuntimeError("boom")
        return super().generate(req, out)


class BrokenLoadBackend(FakeBackend):
    def load(self):
        raise RuntimeError("weights missing")


def test_failed_job_reports_the_error(tmp_path):
    client = make_client(tmp_path)
    plain = wait_for(client, post_job(client, prompt="FAIL").json()["job_id"], {"failed"})
    assert plain["error"] == {"code": "fake_failure", "message": "prompt asked for a failure", "retryable": False}
    retry = wait_for(client, post_job(client, prompt="FAIL RETRY").json()["job_id"], {"failed"})
    assert retry["error"]["retryable"] is True


def test_unexpected_exception_fails_job_and_worker_survives(tmp_path):
    client = make_client(tmp_path, CrashOnceBackend())
    first = wait_for(client, post_job(client).json()["job_id"], {"failed"})
    assert first["error"]["code"] == "internal" and "boom" in first["error"]["message"]
    assert wait_for(client, post_job(client).json()["job_id"], {"done"})["state"] == "done"


def test_full_queue_returns_429(tmp_path):
    backend = BlockingBackend()
    client = make_client(tmp_path, backend, max_queue=2)
    wait_healthy(client)
    running = post_job(client).json()["job_id"]
    wait_for(client, running, {"running"})
    assert post_job(client).status_code == 202
    assert post_job(client).status_code == 202
    assert post_job(client).status_code == 429
    backend.release.set()


def test_cancel_queued_job(tmp_path):
    backend = BlockingBackend()
    client = make_client(tmp_path, backend)
    wait_healthy(client)
    first = post_job(client).json()["job_id"]
    wait_for(client, first, {"running"})
    second = post_job(client).json()["job_id"]
    assert client.delete(f"/jobs/{second}", headers=AUTH).status_code == 204
    assert client.get(f"/jobs/{second}", headers=AUTH).json()["state"] == "cancelled"
    backend.release.set()
    assert wait_for(client, first, {"done"})["state"] == "done"


def test_cancel_running_job_discards_audio(tmp_path):
    backend = BlockingBackend()
    client = make_client(tmp_path, backend)
    wait_healthy(client)
    job = post_job(client).json()["job_id"]
    wait_for(client, job, {"running"})
    assert client.delete(f"/jobs/{job}", headers=AUTH).status_code == 204
    backend.release.set()
    assert wait_for(client, job, {"cancelled"})["state"] == "cancelled"
    assert client.get(f"/jobs/{job}/audio", headers=AUTH).status_code == 409
    assert not (tmp_path / f"{job}.wav").exists()


def test_delete_finished_job_frees_it(tmp_path):
    client = make_client(tmp_path)
    job = post_job(client).json()["job_id"]
    wait_for(client, job, {"done"})
    assert client.delete(f"/jobs/{job}", headers=AUTH).status_code == 204
    assert client.get(f"/jobs/{job}", headers=AUTH).status_code == 404
    assert not (tmp_path / f"{job}.wav").exists()


def test_finished_jobs_expire(tmp_path):
    client = make_client(tmp_path, job_ttl_s=0)
    old = post_job(client).json()["job_id"]
    wait_for(client, old, {"done"})
    post_job(client)  # creating a job sweeps expired ones
    assert client.get(f"/jobs/{old}", headers=AUTH).status_code == 404


def test_load_failure_reports_error_and_fails_jobs(tmp_path):
    client = make_client(tmp_path, BrokenLoadBackend(rtf=0))
    deadline = time.monotonic() + 5
    while client.get("/health", headers=AUTH).json() != {"status": "error"}:
        assert time.monotonic() < deadline
        time.sleep(0.02)
    assert client.get("/health", headers=AUTH).status_code == 503
    body = wait_for(client, post_job(client).json()["job_id"], {"failed"})
    assert body["error"]["code"] == "model_not_loaded" and "weights missing" in body["error"]["message"]


def test_write_lyrics_reports_lyrics_and_metas(tmp_path):
    client = make_client(tmp_path, FakeBackend(rtf=0.001, vocals=True))
    with client:
        wait_healthy(client)
        r = post_job(client, vocals=True, write_lyrics=True, vocal_language="pt")
        assert r.status_code == 202
        body = wait_for(client, r.json()["job_id"], {"done"})
        assert "[chorus]" in body["lyrics"]
        assert body["metas"] == {"bpm": 92, "key": "C major", "time_signature": "4"}


def test_given_lyrics_are_reported_back_and_instrumental_jobs_report_none(tmp_path):
    client = make_client(tmp_path, FakeBackend(rtf=0.001, vocals=True))
    with client:
        wait_healthy(client)
        sung = wait_for(client, post_job(client, vocals=True, lyrics="[verse]\nla la").json()["job_id"], {"done"})
        assert sung["lyrics"] == "[verse]\nla la" and "metas" not in sung
        plain = wait_for(client, post_job(client).json()["job_id"], {"done"})
        assert "lyrics" not in plain and "metas" not in plain


def test_write_lyrics_needs_vocals(tmp_path):
    client = make_client(tmp_path, FakeBackend(rtf=0.001, vocals=True))
    with client:
        assert post_job(client, write_lyrics=True).status_code == 422
