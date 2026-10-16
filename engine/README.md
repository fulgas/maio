# Maio engine server

Wraps a music model behind the Maio engine contract (`../SPEC.md`, section 4).
Runs natively on the Mac so the model can use the GPU.

## Run for development

```bash
MAIO_ENGINE_TOKEN=dev uv run maio-engine --backend fake                     # sine tones, no GPU
MAIO_ENGINE_TOKEN=dev uv run --extra ace-step maio-engine --backend ace-step \
  --ace-root ~/Projects/fulgas/ACE-Step-1.5                                         # real model
uv run pytest -q
```

## Install as a service

The token is stored in the plist, so the file must be readable only by you.

```bash
TOKEN=$(openssl rand -hex 32)
sed -e "s|__MAIO__|$HOME/Projects/fulgas/maio|" \
    -e "s|__ACE_ROOT__|$HOME/Projects/fulgas/ACE-Step-1.5|" \
    -e "s|__TOKEN__|$TOKEN|" -e "s|__HOME__|$HOME|g" \
    com.maio.engine.ace-step.plist > ~/Library/LaunchAgents/com.maio.engine.ace-step.plist
chmod 600 ~/Library/LaunchAgents/com.maio.engine.ace-step.plist
launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.maio.engine.ace-step.plist
echo "MAIO_ACE_STEP_TOKEN=$TOKEN"   # put this in Maio's .env
```

Stop: `launchctl bootout gui/$(id -u)/com.maio.engine.ace-step`. Logs: `~/Library/Logs/maio-engine.log`.

## Lyrics

A job with `write_lyrics: true` makes the ACE-Step backend load its planning model (`acestep-5Hz-lm-1.7B`, already in
the checkpoints) the first time it is needed. It writes lyrics in `vocal_language` and the job reports `lyrics` and
`metas` (`bpm`, `key`, `time_signature`). Instrumental-only setups never load it.
