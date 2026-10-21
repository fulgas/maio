# Run ACE-Step on another machine

Use this when the GPU is on another machine than the one running Maio's Docker Compose project, for example a Linux box with an NVIDIA card.

1. On the GPU machine, install ACE-Step 1.5 as [its own instructions](https://github.com/ace-step/ACE-Step-1.5#readme) say.
2. Start `acestep-api` listening on an address the Docker host can reach, not only on `127.0.0.1`: set ACE-Step's `ACESTEP_API_HOST` (for example `0.0.0.0`) and `ACESTEP_API_PORT` (Maio's example config uses `7801`).
3. Protect it with a key, since it now listens on the network: start ACE-Step with `ACESTEP_API_KEY`, and put the same key in Maio's `.env` as `ACE_STEP_API_KEY`.
4. In `maio.toml`, point the engine at the GPU machine and name the key:

   ```toml
   [[engine]]
   id = "ace-step"
   url = "http://gpu-box.lan:7801"
   api_key_env = "ACE_STEP_API_KEY"
   ```

5. Restart Maio: `docker compose up -d --force-recreate maio`.
6. Check the header: the engine icon turns green when ACE-Step answers. Its panel says what is wrong when it does not.

How Maio talks to ACE-Step, and what it expects from it: [arc42 3.3](../architecture/03-system-scope-and-context.md#33-the-music-engine-ace-step-15).
