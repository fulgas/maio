//! Text engines (SPEC 10): language models that write lyrics, titles and descriptions, never music.

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::{Context, bail};
use serde_json::{Value, json};

use crate::{
    config::{TextEngineConfig, TextKind},
    engine::Health,
};

use crate::request::Normalized;

/// How long a text engine may take (SPEC 10): 30 s for lyrics, 15 s for a title.
pub const LYRICS_TIMEOUT: Duration = Duration::from_secs(30);
pub const NAMING_TIMEOUT: Duration = Duration::from_secs(15);

const LYRICS_SYSTEM: &str = "You write song lyrics. Reply with the lyrics only: plain text, sections marked [verse] and [chorus] (and [bridge] if it helps), no title, no notes, no quotes around them.";
const TITLE_SYSTEM: &str = "You name songs. Reply with JSON only, no other text: {\"title\": \"at most 60 characters\", \"description\": \"one sentence about the song\"}.";

/// The structured fields, in words. Never the engine prompt Maio built.
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

/// Lyrics for a song (SPEC 10, Lyrics). Only the person's prompt and the structured fields go out.
pub async fn write_lyrics(te: &TextEngine, n: &Normalized, timeout: Duration) -> anyhow::Result<String> {
    let brief = n.user_prompt.as_deref().map(|p| format!("\nWhat the listener asked for: {p}")).unwrap_or_default();
    let user = format!("Write lyrics in the language with code {}.\n{}{brief}", n.vocal_language, fields(n));
    let text = te.complete(LYRICS_SYSTEM, &user, 3000, timeout).await?; // room for models that reason first
    let text = text.trim();
    if text.is_empty() {
        bail!("the text engine wrote no lyrics");
    }
    Ok(text.chars().take(4000).collect())
}

/// A title and a one-line description (SPEC 10, Titles, layer 2).
pub async fn name_song(te: &TextEngine, n: &Normalized, lyrics: Option<&str>, timeout: Duration) -> anyhow::Result<(String, String)> {
    let brief = n.user_prompt.as_deref().map(|p| format!("\nWhat the listener asked for: {p}")).unwrap_or_default();
    let words = lyrics.map(|l| format!("\nLyrics:\n{l}")).unwrap_or_default();
    let user = format!("Name this song.\n{}{brief}{words}", fields(n));
    let reply = te.complete(TITLE_SYSTEM, &user, 1000, timeout).await?; // room for models that reason first
    parse_title(&reply).context("the text engine gave no usable title")
}

/// `{"title", "description"}` from a reply that may wrap the JSON in other text.
pub fn parse_title(reply: &str) -> Option<(String, String)> {
    let (start, end) = (reply.find('{')?, reply.rfind('}')?);
    // Models sometimes put raw line breaks inside JSON strings; as plain spaces they parse.
    let json = reply.get(start..=end)?.replace(['\n', '\r', '\t'], " ");
    let v: Value = serde_json::from_str(&json).ok()?;
    let title = crate::titles::trim_to(v["title"].as_str()?.trim(), 60);
    if title.is_empty() {
        return None;
    }
    let description = v["description"].as_str().unwrap_or_default().split_whitespace().collect::<Vec<_>>().join(" ");
    Some((title, crate::titles::trim_to(&description, 200)))
}

pub struct TextEngine {
    pub cfg: TextEngineConfig,
    http: reqwest::Client,
    health: Mutex<Health>,
}

impl TextEngine {
    pub fn new(cfg: TextEngineConfig) -> Self {
        let http = reqwest::Client::builder().connect_timeout(Duration::from_secs(5)).build().expect("http client");
        Self { cfg, http, health: Mutex::new(Health::Unknown) }
    }

    pub fn health(&self) -> Health {
        *self.health.lock().unwrap()
    }

    /// Who wrote the words, for a track's credits.
    pub fn licence(&self) -> String {
        self.cfg.licence.clone().unwrap_or_else(|| match self.cfg.kind {
            TextKind::Anthropic => "Anthropic commercial terms".into(),
            TextKind::Openai => "unknown".into(),
        })
    }

    /// One chat turn: a fixed system prompt and the user text. Errors carry no key and no full body.
    pub async fn complete(&self, system: &str, user: &str, max_tokens: u32, timeout: Duration) -> anyhow::Result<String> {
        let req = match self.cfg.kind {
            TextKind::Anthropic => {
                let base = self.cfg.url.as_deref().unwrap_or("https://api.anthropic.com").trim_end_matches('/');
                self.http.post(format!("{base}/v1/messages"))
                    .header("x-api-key", self.cfg.api_key.clone().unwrap_or_default())
                    .header("anthropic-version", "2023-06-01")
                    .json(&json!({"model": self.cfg.model, "max_tokens": max_tokens, "system": system,
                                  "messages": [{"role": "user", "content": user}]}))
            }
            TextKind::Openai => {
                let base = self.cfg.url.as_deref().unwrap_or_default().trim_end_matches('/');
                let mut r = self.http.post(format!("{base}/chat/completions"))
                    .json(&json!({"model": self.cfg.model, "max_tokens": max_tokens,
                                  "messages": [{"role": "system", "content": system}, {"role": "user", "content": user}]}));
                if let Some(k) = &self.cfg.api_key {
                    r = r.bearer_auth(k);
                }
                r
            }
        };
        let res = req.timeout(timeout).send().await.context("calling the text engine")?;
        let status = res.status();
        if !status.is_success() {
            let body = res.text().await.unwrap_or_default();
            bail!("text engine answered {status}: {}", body.chars().take(200).collect::<String>());
        }
        let v: Value = res.json().await.context("text engine reply")?;
        let text = match self.cfg.kind {
            TextKind::Anthropic => v["content"].as_array().map(|blocks| {
                blocks.iter().filter(|b| b["type"] == "text").filter_map(|b| b["text"].as_str()).collect::<String>()
            }),
            TextKind::Openai => v["choices"][0]["message"]["content"].as_str().map(str::to_string),
        };
        text.map(|t| strip_thinking(&t)).context("text engine reply had no text")
    }

    /// A one-token call (SPEC 10): `up` when it answers, `down` when it does not.
    pub async fn check(&self) {
        let ok = self.complete("Reply with OK.", "OK", 1, Duration::from_secs(30)).await;
        if let Err(e) = &ok {
            tracing::warn!(text_engine = %self.cfg.id, "health check: {e:#}");
        }
        *self.health.lock().unwrap() = if ok.is_ok() { Health::Up } else { Health::Down };
    }
}

/// Drops `<think>...</think>` blocks that reasoning models (Qwen3 and others) put in their replies.
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

/// Checks at start and every 5 minutes.
pub async fn health_loop(e: Arc<TextEngine>) {
    loop {
        e.check().await;
        tokio::time::sleep(Duration::from_secs(300)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_from_engine_replies() {
        assert_eq!(parse_title(r#"{"title": "Night Bus", "description": "A slow ride home."}"#), Some(("Night Bus".into(), "A slow ride home.".into())));
        assert_eq!(parse_title("Sure! ```json\n{\"title\":\"X\",\"description\":\"two\nlines\"}\n```").unwrap().1, "two lines", "one line, fences ignored");
        assert_eq!(parse_title(&format!(r#"{{"title": "{}", "description": "d"}}"#, "a".repeat(90))).unwrap().0.chars().count(), 60);
        assert_eq!(parse_title(r#"{"title": "  ", "description": "d"}"#), None);
        assert_eq!(parse_title("no json here"), None);
    }
}
