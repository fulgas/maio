//! Text engines over HTTP (SPEC 7): the Anthropic Messages API and OpenAI-compatible chat APIs.

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use anyhow::{Context, Result, bail};
use crate::usecases::ports::{SongName, TextEngine};
use async_trait::async_trait;
use crate::domain::{MAX_TITLE, request::Normalized, titles::trim_to};
use serde_json::{Value, json};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextKind {
    Anthropic,
    Openai,
}

/// `[[text_engine]]` in config (SPEC 7), with the API key already read from its env var.
#[derive(Debug, Clone, PartialEq)]
pub struct TextEngineConfig {
    pub id: String,
    pub kind: TextKind,
    pub model: String,
    /// Required for `openai`; `anthropic` defaults to the Anthropic API.
    pub url: Option<String>,
    pub api_key: Option<String>,
}

const LYRICS_SYSTEM: &str = "You write song lyrics. Reply with the lyrics only: plain text, sections marked [verse] and [chorus] (and [bridge] if it helps), no title, no notes, no quotes around them.";
const TITLE_SYSTEM: &str = "You name songs. Reply with JSON only, no other text: {\"title\": \"at most 60 characters\", \"description\": \"one sentence about the song\"}.";

/// The structured fields in words. Never the engine prompt Maio built.
fn fields(n: &Normalized) -> String {
    let mut f = Vec::new();
    f.extend(n.mood.as_ref().map(|m| format!("mood: {m}")));
    f.extend(n.genre.as_ref().map(|g| format!("genre: {g}")));
    f.extend(n.energy.map(|e| format!("energy: {}", e.as_str())));
    f.extend(n.bpm.map(|b| format!("tempo: {b} bpm")));
    f.push(format!("length: about {} seconds", n.seconds));
    f.push(if n.vocals { format!("sung in: {}", n.vocal_language) } else { "instrumental".into() });
    f.join("\n")
}

fn brief(n: &Normalized) -> String {
    n.prompt.as_deref().map(|p| format!("\nWhat the listener asked for: {p}")).unwrap_or_default()
}

/// `{"title", "description"}` from a reply that may wrap the JSON in other text.
pub fn parse_title(reply: &str) -> Option<SongName> {
    let (start, end) = (reply.find('{')?, reply.rfind('}')?);
    // Models sometimes put raw line breaks inside JSON strings; as spaces they parse.
    let json = reply.get(start..=end)?.replace(['\n', '\r', '\t'], " ");
    let v: Value = serde_json::from_str(&json).ok()?;
    let title = trim_to(v["title"].as_str()?.trim(), MAX_TITLE);
    if title.is_empty() {
        return None;
    }
    let description = v["description"].as_str().unwrap_or_default().split_whitespace().collect::<Vec<_>>().join(" ");
    Some(SongName { title, description: trim_to(&description, 200) })
}

/// Drops `<think>...</think>` blocks that reasoning models put in their replies (SPEC 7).
fn strip_thinking(text: &str) -> String {
    let mut out = text.to_string();
    while let (Some(a), Some(b)) = (out.find("<think>"), out.find("</think>")) {
        if b < a {
            break;
        }
        out.replace_range(a..b + "</think>".len(), "");
    }
    out.trim().to_string()
}

pub struct HttpTextEngine {
    cfg: TextEngineConfig,
    http: reqwest::Client,
    healthy: AtomicBool,
}

impl HttpTextEngine {
    pub fn new(cfg: TextEngineConfig) -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(60))
            .build()
            .expect("an HTTP client builds");
        Self { cfg, http, healthy: AtomicBool::new(true) }
    }

    /// One chat turn: fixed instructions and the user text. Errors carry neither the key nor a whole reply.
    async fn complete(&self, system: &str, user: &str, max_tokens: u32) -> Result<String> {
        let request = match self.cfg.kind {
            TextKind::Anthropic => {
                let base = self.cfg.url.as_deref().unwrap_or("https://api.anthropic.com").trim_end_matches('/');
                self.http
                    .post(format!("{base}/v1/messages"))
                    .header("x-api-key", self.cfg.api_key.clone().unwrap_or_default())
                    .header("anthropic-version", "2023-06-01")
                    .json(&json!({"model": self.cfg.model, "max_tokens": max_tokens, "system": system,
                                  "messages": [{"role": "user", "content": user}]}))
            }
            TextKind::Openai => {
                let base = self.cfg.url.as_deref().unwrap_or_default().trim_end_matches('/');
                let r = self.http.post(format!("{base}/chat/completions")).json(&json!({"model": self.cfg.model, "max_tokens": max_tokens,
                    "messages": [{"role": "system", "content": system}, {"role": "user", "content": user}]}));
                match &self.cfg.api_key {
                    Some(key) => r.bearer_auth(key),
                    None => r,
                }
            }
        };
        let res = request.send().await.context("calling the text engine")?;
        let status = res.status();
        if !status.is_success() {
            let body: String = res.text().await.unwrap_or_default().chars().take(200).collect();
            bail!("text engine answered {status}: {body}");
        }
        let v: Value = res.json().await.context("reading the text engine's reply")?;
        let text = match self.cfg.kind {
            TextKind::Anthropic => v["content"].as_array().map(|blocks| blocks.iter().filter(|b| b["type"] == "text").filter_map(|b| b["text"].as_str()).collect::<String>()),
            TextKind::Openai => v["choices"][0]["message"]["content"].as_str().map(str::to_string),
        };
        text.map(|t| strip_thinking(&t)).context("the text engine's reply had no text")
    }

    /// A one-token call (SPEC 7): healthy when it answers.
    pub async fn check(&self) {
        let ok = self.complete("Reply with OK.", "OK", 1).await.is_ok();
        if self.healthy.swap(ok, Ordering::SeqCst) != ok {
            if ok {
                tracing::info!(text_engine = %self.cfg.id, "text engine is up");
            } else {
                tracing::warn!(text_engine = %self.cfg.id, "text engine is down");
            }
        }
    }
}

/// Checks at start and every 5 minutes (SPEC 7).
pub async fn watch(engine: Arc<HttpTextEngine>) {
    loop {
        engine.check().await;
        tokio::time::sleep(Duration::from_secs(300)).await;
    }
}

#[async_trait]
impl TextEngine for HttpTextEngine {
    fn id(&self) -> &str {
        &self.cfg.id
    }

    fn healthy(&self) -> bool {
        self.healthy.load(Ordering::SeqCst)
    }

    async fn write_lyrics(&self, n: &Normalized) -> Result<String> {
        let user = format!("Write lyrics in the language with code {}.\n{}{}", n.vocal_language, fields(n), brief(n));
        // Room for models that reason before they answer.
        let lyrics = self.complete(LYRICS_SYSTEM, &user, 3000).await?;
        if lyrics.trim().is_empty() {
            bail!("the text engine wrote no lyrics");
        }
        Ok(lyrics.trim().chars().take(crate::domain::request::MAX_LYRICS).collect())
    }

    async fn name_song(&self, n: &Normalized, lyrics: Option<&str>) -> Result<SongName> {
        let words = lyrics.map(|l| format!("\nLyrics:\n{l}")).unwrap_or_default();
        let user = format!("Name this song.\n{}{}{words}", fields(n), brief(n));
        let reply = self.complete(TITLE_SYSTEM, &user, 1000).await?;
        parse_title(&reply).context("the text engine gave no usable title")
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use axum::{
        Json, Router,
        extract::State,
        http::{HeaderMap, StatusCode},
        routing::post,
    };
    use crate::domain::testing::normalized;
    use serde_json::{Value, json};

    use super::*;

    #[derive(Clone, Default)]
    struct Seen {
        bodies: Arc<Mutex<Vec<Value>>>,
        headers: Arc<Mutex<Vec<(String, String)>>>,
        fail: Arc<Mutex<bool>>,
    }

    async fn serve(seen: Seen, reply: Value) -> String {
        let handler = move |State(s): State<Seen>, headers: HeaderMap, Json(body): Json<Value>| {
            let reply = reply.clone();
            async move {
                for key in ["authorization", "x-api-key", "anthropic-version"] {
                    if let Some(v) = headers.get(key) {
                        s.headers.lock().unwrap().push((key.into(), v.to_str().unwrap().into()));
                    }
                }
                s.bodies.lock().unwrap().push(body);
                if *s.fail.lock().unwrap() {
                    return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "boom"})));
                }
                (StatusCode::OK, Json(reply))
            }
        };
        let app = Router::new().route("/chat/completions", post(handler.clone())).route("/v1/messages", post(handler)).with_state(seen);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        url
    }

    fn sung() -> Normalized {
        Normalized { prompt: Some("rainy bus ride".into()), vocals: true, vocal_language: "pt".into(), ..normalized() }
    }

    fn openai(url: String, key: &str) -> HttpTextEngine {
        HttpTextEngine::new(TextEngineConfig { id: "local".into(), kind: TextKind::Openai, model: "qwen3-8b".into(), url: Some(url), api_key: Some(key.into()) })
    }

    #[tokio::test]
    async fn openai_compatible_engines_write_lyrics_without_their_thinking() {
        let seen = Seen::default();
        let url = serve(seen.clone(), json!({"choices": [{"message": {"content": "<think>let me see</think>\n[verse]\nchuva no autocarro"}}]})).await;
        let e = openai(url, "sk-local");
        assert_eq!(e.write_lyrics(&sung()).await.unwrap(), "[verse]\nchuva no autocarro");
        let body = seen.bodies.lock().unwrap()[0].clone();
        assert_eq!(body["model"], "qwen3-8b");
        let user = body["messages"][1]["content"].as_str().unwrap();
        assert!(user.contains("code pt") && user.contains("rainy bus ride") && user.contains("mood: calm"), "{user}");
        assert!(!user.contains(&sung().engine_prompt), "the engine prompt Maio built is never sent");
        assert!(seen.headers.lock().unwrap().contains(&("authorization".into(), "Bearer sk-local".into())));
    }

    #[tokio::test]
    async fn anthropic_names_songs() {
        let seen = Seen::default();
        let url = serve(seen.clone(), json!({"content": [{"type": "text", "text": "{\"title\": \"Night Bus\", \"description\": \"A slow ride home.\"}"}]})).await;
        let e = HttpTextEngine::new(TextEngineConfig {
            id: "claude".into(),
            kind: TextKind::Anthropic,
            model: "claude-haiku-4-5-20251001".into(),
            url: Some(url),
            api_key: Some("key".into()),
        });
        let name = e.name_song(&sung(), Some("[chorus]\nnight bus")).await.unwrap();
        assert_eq!(name, SongName { title: "Night Bus".into(), description: "A slow ride home.".into() });
        let headers = seen.headers.lock().unwrap().clone();
        assert!(headers.contains(&("x-api-key".into(), "key".into())) && headers.contains(&("anthropic-version".into(), "2023-06-01".into())));
        let body = seen.bodies.lock().unwrap()[0].clone();
        assert!(body["system"].as_str().unwrap().contains("JSON") && body["messages"][0]["content"].as_str().unwrap().contains("night bus"));
    }

    #[tokio::test]
    async fn a_failed_check_marks_the_engine_unhealthy_and_errors_never_carry_the_key() {
        let seen = Seen::default();
        let e = openai(serve(seen.clone(), json!({"choices": [{"message": {"content": "OK"}}]})).await, "sk-secret");
        assert!(e.healthy(), "healthy until a check fails");
        *seen.fail.lock().unwrap() = true;
        e.check().await;
        assert!(!e.healthy());
        let err = format!("{:#}", e.write_lyrics(&sung()).await.unwrap_err());
        assert!(err.contains("500") && !err.contains("sk-secret"), "{err}");
        *seen.fail.lock().unwrap() = false;
        e.check().await;
        assert!(e.healthy());
    }

    #[test]
    fn titles_from_engine_replies() {
        let name = |t: &str, d: &str| Some(SongName { title: t.into(), description: d.into() });
        assert_eq!(parse_title(r#"{"title": "Night Bus", "description": "A slow ride home."}"#), name("Night Bus", "A slow ride home."));
        assert_eq!(parse_title("Sure! ```json\n{\"title\":\"X\",\"description\":\"two\nlines\"}\n```"), name("X", "two lines"), "one line, fences ignored");
        assert_eq!(parse_title(&format!(r#"{{"title": "{}", "description": "d"}}"#, "a".repeat(90))).unwrap().title.chars().count(), 60);
        assert_eq!(parse_title(r#"{"title": "  ", "description": "d"}"#), None);
        assert_eq!(parse_title("no json here"), None);
    }

    #[test]
    fn thinking_is_dropped() {
        assert_eq!(strip_thinking("<think>a</think> hi <think>b</think>"), "hi");
        assert_eq!(strip_thinking("</think> odd <think>"), "</think> odd <think>", "an end before a start is left alone");
    }
}
