# Run ACE-Step for Maio

Maio makes songs with ACE-Step 1.5's own server, `acestep-api`. Maio neither installs nor starts it: run it on the machine with the GPU, which can be the one running Maio or another one. This guide gives the settings Maio expects.

1. Install ACE-Step 1.5 as [its own instructions](https://github.com/ace-step/ACE-Step-1.5#readme) say; they cover CUDA, Apple silicon, AMD ROCm, Intel XPU and CPU, and end with `uv sync` in ACE-Step's folder. The first start downloads the models, about 9.4 GB.
2. In ACE-Step's folder, start `acestep-api` with these settings:

   ```bash
   export ACESTEP_API_PORT=7801          # the port in Maio's example config (ACE-Step's own default is 8001)
   export ACESTEP_API_HOST=127.0.0.1     # 0.0.0.0 when Maio runs on another machine
   export ACESTEP_NO_INIT=false          # load the models at start, so Maio sees the engine ready
   export ACESTEP_LM_MODEL_PATH=acestep-5Hz-lm-1.7B   # the planning model that writes lyrics
   uv run acestep-api
   ```

   On a Mac with Apple silicon, also set `ACESTEP_DEVICE=mps` and `ACESTEP_LM_BACKEND=pt`: ACE-Step's default language-model backend, vLLM, needs CUDA.
3. When it listens on the network (`0.0.0.0`), protect it with a key: add `export ACESTEP_API_KEY=<a long random string>` before starting it, put the same key in Maio's `.env` as `ACE_STEP_API_KEY`, and uncomment `api_key_env = "ACE_STEP_API_KEY"` under `[[engine]]` in `maio.toml`.
4. Point the engine's `url` in `maio.toml` at it:
   - **The same machine as Maio**: `http://host.docker.internal:7801`, as in the example config.
   - **Another machine**: that machine's address, such as `http://gpu-box.lan:7801`.
5. Restart Maio: `docker compose up -d --force-recreate maio`.
6. Check the header: the engine icon turns green when ACE-Step answers with its models loaded. Its panel says what is wrong when it does not.

To trace ACE-Step together with Maio (arc42 8.6), start it under OpenTelemetry's zero-code instrumentation instead, with the same `OTEL_EXPORTER_OTLP_ENDPOINT` Maio uses:

```bash
export OTEL_SERVICE_NAME=ace-step OTEL_EXPORTER_OTLP_PROTOCOL=http/protobuf
uv run --with opentelemetry-distro --with opentelemetry-exporter-otlp --with opentelemetry-instrumentation-fastapi \
  opentelemetry-instrument acestep-api
```

How Maio talks to ACE-Step, and what it expects from it: [arc42 3.3](../architecture/03-system-scope-and-context.md#33-the-music-engine-ace-step-15).
