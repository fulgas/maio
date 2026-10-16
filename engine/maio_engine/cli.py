import argparse
import os
import sys
from pathlib import Path

import uvicorn

from .fake import FakeBackend
from .server import create_app


def main() -> None:
    ap = argparse.ArgumentParser(prog="maio-engine", description="Maio engine server (SPEC.md section 4)")
    ap.add_argument("--backend", choices=["fake", "ace-step"], required=True)
    ap.add_argument("--host", default="127.0.0.1")
    ap.add_argument("--port", type=int, default=7801)
    ap.add_argument("--work-dir", type=Path, default=Path.home() / "Library/Application Support/maio-engine")
    ap.add_argument("--max-queue", type=int, default=4)
    ap.add_argument("--fake-rtf", type=float, default=0.05)
    ap.add_argument("--ace-root", type=Path, help="ACE-Step 1.5 checkout that holds checkpoints/")
    ap.add_argument("--ace-model", default="acestep-v15-turbo")
    ap.add_argument("--ace-device", default="mps")
    ap.add_argument("--ace-steps", type=int, default=8)
    a = ap.parse_args()

    token = os.environ.get("MAIO_ENGINE_TOKEN")
    if not token:
        sys.exit("MAIO_ENGINE_TOKEN is not set")

    if a.backend == "fake":
        backend = FakeBackend(rtf=a.fake_rtf)
    else:
        if a.ace_root is None:
            sys.exit("--ace-root is required for --backend ace-step")
        from .ace_step import AceStepBackend

        backend = AceStepBackend(a.ace_root, a.ace_model, a.ace_device, a.ace_steps)

    uvicorn.run(create_app(backend, token, a.work_dir / a.backend, a.max_queue), host=a.host, port=a.port)
