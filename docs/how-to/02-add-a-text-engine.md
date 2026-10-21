# Add a text engine

Use this to have a language model write lyrics and titles. Without one, ACE-Step writes the lyrics and titles come from word lists.

1. Add a `[[text_engine]]` to `maio.toml`. For Anthropic:

   ```toml
   [[text_engine]]
   id = "claude"
   kind = "anthropic"
   model = "claude-haiku-4-5-20251001"
   api_key_env = "ANTHROPIC_API_KEY"
   ```

   For a local model with an OpenAI-compatible API (Ollama, LM Studio, llama.cpp, vLLM), the `url` is required, as Maio's container reaches it:

   ```toml
   [[text_engine]]
   id = "local"
   kind = "openai"
   url = "http://host.docker.internal:11434/v1"
   model = "qwen3:8b"
   ```

2. Put the key in `.env` if the engine needs one: `ANTHROPIC_API_KEY=…`.
3. Choose what it writes: lyrics with `default_engine = "claude"` under `[lyrics]`, titles with `engine = "claude"` under `[naming]`.
4. Restart Maio: `docker compose up -d --force-recreate maio`. A text engine Maio cannot reach shows in the engine panel; it is checked at start and every 5 minutes.

What text engines receive, and the kinds Maio supports: [arc42 3.4](../architecture/03-system-scope-and-context.md#34-text-engines).
