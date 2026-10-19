//! `maio.toml` (SPEC 11), checked once at start so a bad config never half-runs.
//! Secrets come from the environment and never appear in an error.

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    time::Duration,
};

use crate::adapters::{
    engine::EngineConfig,
    ffmpeg::StoreFormat,
    text::{TextEngineConfig, TextKind},
};
use anyhow::{Context, Result, ensure};
use crate::usecases::MakeSettings;
use crate::domain::{limits::Rate, request::valid_language};
use serde::Deserialize;

pub const API_TOKEN_ENV: &str = "MAIO_API_TOKEN";

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default = "default_listen")]
    pub listen: String,
    pub data_dir: PathBuf,
    /// Marks the session cookie `Secure`; set it when Maio is served over HTTPS.
    #[serde(default)]
    pub secure_cookies: bool,
    /// The address browsers use, such as `https://music.example`, when a proxy sits in front and changes `Host`.
    /// Without it, `Origin` must match the `Host` header (SPEC 4).
    #[serde(default)]
    pub public_origin: Option<String>,
    /// `text` (default) or `json` lines on stdout.
    #[serde(default)]
    pub log_format: LogFormat,
    pub moods: Vec<String>,
    #[serde(default)]
    pub genres: Vec<String>,
    #[serde(default)]
    pub limits: Limits,
    /// A job whose engine shows no change for this long fails, and is retried once elsewhere.
    #[serde(default = "default_stall")]
    pub engine_stall_s: u64,
    #[serde(default)]
    pub storage: Storage,
    #[serde(rename = "engine", default)]
    pub engines: Vec<Engine>,
    #[serde(rename = "text_engine", default)]
    pub text_engines: Vec<TextEngine>,
    #[serde(default)]
    pub naming: Naming,
    #[serde(default)]
    pub lyrics: Lyrics,
    /// From `MAIO_API_TOKEN`.
    #[serde(skip)]
    pub api_token: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    #[default]
    Text,
    Json,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Limits {
    pub make: String,
    pub pending_jobs: u32,
}

impl Default for Limits {
    fn default() -> Self {
        Self { make: "20/10m".into(), pending_jobs: 4 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Opus,
    Flac,
    Mp3,
    Aac,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Storage {
    pub format: Format,
    pub bitrate_k: u32,
}

impl Default for Storage {
    fn default() -> Self {
        Self { format: Format::Opus, bitrate_k: 160 }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Engine {
    pub id: String,
    pub url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Anthropic,
    Openai,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextEngine {
    pub id: String,
    pub kind: Kind,
    pub model: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub api_key_env: Option<String>,
    /// Read from `api_key_env` at load.
    #[serde(skip)]
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Naming {
    pub engine: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Lyrics {
    pub default_engine: String,
    pub default_language: String,
}

impl Default for Lyrics {
    fn default() -> Self {
        Self { default_engine: "ace-step".into(), default_language: "en".into() }
    }
}

fn default_listen() -> String {
    "0.0.0.0:8080".into()
}

fn default_stall() -> u64 {
    600
}

/// Reads and checks the config file. `env` reads environment variables.
pub fn load(path: &Path, env: impl Fn(&str) -> Option<String>) -> Result<Config> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    parse(&text, env).with_context(|| format!("in {}", path.display()))
}

fn unique<'a>(ids: impl Iterator<Item = &'a String>, what: &str) -> Result<()> {
    let mut seen = HashSet::new();
    for id in ids {
        ensure!(!id.is_empty(), "{what} ids cannot be empty");
        ensure!(seen.insert(id), "two {what}s share the id {id:?}");
    }
    Ok(())
}

pub fn parse(text: &str, env: impl Fn(&str) -> Option<String>) -> Result<Config> {
    let mut cfg: Config = toml::from_str(text)?;
    unique(cfg.engines.iter().map(|e| &e.id), "engine")?;
    unique(cfg.text_engines.iter().map(|t| &t.id), "text engine")?;
    for t in &mut cfg.text_engines {
        ensure!(t.id != "ace-step", "a text engine cannot be called \"ace-step\": that name means ACE-Step writes the lyrics");
        ensure!(t.kind != Kind::Openai || t.url.is_some(), "text engine {}: kind \"openai\" needs a url", t.id);
        if let Some(var) = &t.api_key_env {
            t.api_key = Some(env(var).with_context(|| format!("text engine {}: {var} is not set", t.id))?);
        }
    }
    let text_ids: Vec<&String> = cfg.text_engines.iter().map(|t| &t.id).collect();
    if let Some(n) = &cfg.naming.engine {
        ensure!(text_ids.contains(&n), "[naming] engine {n:?} is not a text engine");
    }
    let by = &cfg.lyrics.default_engine;
    ensure!(by == "ace-step" || text_ids.contains(&by), "[lyrics] default_engine {by:?} is neither \"ace-step\" nor a text engine");
    ensure!(valid_language(&cfg.lyrics.default_language), "[lyrics] default_language is a code such as \"en\"");
    Rate::parse(&cfg.limits.make).map_err(anyhow::Error::msg).context("[limits] make")?;
    ensure!(cfg.limits.pending_jobs >= 1, "[limits] pending_jobs must be at least 1");
    ensure!(cfg.engine_stall_s >= 1, "engine_stall_s must be at least 1");
    if let Some(origin) = &mut cfg.public_origin {
        *origin = origin.trim_end_matches('/').to_string();
        let rest = origin.strip_prefix("https://").or_else(|| origin.strip_prefix("http://"));
        ensure!(rest.is_some_and(|r| !r.is_empty() && !r.contains('/')), "public_origin is an origin such as \"https://music.example\", with no path");
    }
    cfg.api_token = env(API_TOKEN_ENV);
    if let Some(token) = &cfg.api_token {
        ensure!(token.chars().count() >= 32, "{API_TOKEN_ENV} must be at least 32 characters");
    }
    Ok(cfg)
}

impl Config {
    pub fn engine_configs(&self) -> Vec<EngineConfig> {
        self.engines.iter().map(|e| EngineConfig { id: e.id.clone(), url: e.url.clone() }).collect()
    }

    pub fn text_engine_configs(&self) -> Vec<TextEngineConfig> {
        self.text_engines
            .iter()
            .map(|t| TextEngineConfig {
                id: t.id.clone(),
                kind: match t.kind {
                    Kind::Anthropic => TextKind::Anthropic,
                    Kind::Openai => TextKind::Openai,
                },
                model: t.model.clone(),
                url: t.url.clone(),
                api_key: t.api_key.clone(),
            })
            .collect()
    }

    pub fn store_format(&self) -> StoreFormat {
        match self.storage.format {
            Format::Opus => StoreFormat::Opus,
            Format::Flac => StoreFormat::Flac,
            Format::Mp3 => StoreFormat::Mp3,
            Format::Aac => StoreFormat::Aac,
        }
    }

    /// SPEC 5 and 6 fix the poll (2 s) and the text engines' limits (30 s lyrics, 15 s titles).
    pub fn make_settings(&self) -> MakeSettings {
        MakeSettings {
            moods: self.moods.clone(),
            genres: self.genres.clone(),
            default_language: self.lyrics.default_language.clone(),
            default_lyrics_by: self.lyrics.default_engine.clone(),
            naming_engine: self.naming.engine.clone(),
            make_rate: Rate::parse(&self.limits.make).expect("checked when the config was loaded"),
            pending_jobs: self.limits.pending_jobs,
            poll: Duration::from_secs(2),
            lyrics_timeout: Duration::from_secs(30),
            naming_timeout: Duration::from_secs(15),
            stall: Duration::from_secs(self.engine_stall_s),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = "data_dir = \"/data\"\nmoods = [\"calm\"]\n";

    fn no_env(_: &str) -> Option<String> {
        None
    }

    fn error(text: &str) -> String {
        format!("{:#}", parse(text, no_env).unwrap_err())
    }

    #[test]
    fn a_minimal_config_takes_the_spec_defaults() {
        let cfg = parse(MINIMAL, no_env).unwrap();
        assert_eq!((cfg.listen.as_str(), cfg.secure_cookies, cfg.engine_stall_s), ("0.0.0.0:8080", false, 600));
        assert_eq!((cfg.limits.make.as_str(), cfg.limits.pending_jobs), ("20/10m", 4));
        assert_eq!((cfg.storage.format, cfg.storage.bitrate_k), (Format::Opus, 160));
        assert_eq!((cfg.lyrics.default_engine.as_str(), cfg.lyrics.default_language.as_str()), ("ace-step", "en"));
        assert!(cfg.engines.is_empty() && cfg.api_token.is_none());
        let s = cfg.make_settings();
        assert_eq!((s.make_rate, s.pending_jobs, s.stall), (Rate { max: 20, window_s: 600 }, 4, Duration::from_secs(600)));
        assert_eq!((s.poll, s.lyrics_timeout, s.naming_timeout), (Duration::from_secs(2), Duration::from_secs(30), Duration::from_secs(15)));
    }

    #[test]
    fn engines_and_text_engines_become_adapter_configs() {
        let text = format!(
            "{MINIMAL}[[engine]]\nid = \"ace-step\"\nurl = \"http://host.docker.internal:7801\"\n\
             [[text_engine]]\nid = \"claude\"\nkind = \"anthropic\"\nmodel = \"m\"\napi_key_env = \"KEY\"\n\
             [naming]\nengine = \"claude\"\n[lyrics]\ndefault_engine = \"claude\"\n[storage]\nformat = \"flac\"\n"
        );
        let cfg = parse(&text, |k| (k == "KEY").then(|| "sk-secret".to_string())).unwrap();
        assert_eq!(cfg.engine_configs(), [EngineConfig { id: "ace-step".into(), url: "http://host.docker.internal:7801".into() }]);
        let t = &cfg.text_engine_configs()[0];
        assert_eq!((t.kind, t.api_key.as_deref(), t.url.as_deref()), (TextKind::Anthropic, Some("sk-secret"), None));
        assert_eq!(cfg.store_format(), StoreFormat::Flac);
        assert_eq!(cfg.make_settings().naming_engine.as_deref(), Some("claude"));
    }

    #[test]
    fn mistakes_are_refused_with_a_reason() {
        assert!(error(&format!("{MINIMAL}colour = \"red\"\n")).contains("colour"), "unknown fields");
        assert!(error(&format!("{MINIMAL}[[engine]]\nid = \"a\"\nurl = \"x\"\n[[engine]]\nid = \"a\"\nurl = \"y\"\n")).contains("share the id"));
        assert!(error(&format!("{MINIMAL}[naming]\nengine = \"ghost\"\n")).contains("ghost"));
        assert!(error(&format!("{MINIMAL}[lyrics]\ndefault_engine = \"ghost\"\n")).contains("ghost"));
        assert!(error(&format!("{MINIMAL}[lyrics]\ndefault_language = \"English\"\n")).contains("default_language"));
        assert!(error(&format!("{MINIMAL}[limits]\nmake = \"lots\"\n")).contains("[limits] make"));
        assert!(error(&format!("{MINIMAL}[limits]\npending_jobs = 0\n")).contains("pending_jobs"));
        assert!(error(&format!("{MINIMAL}[[text_engine]]\nid = \"local\"\nkind = \"openai\"\nmodel = \"m\"\n")).contains("needs a url"));
        assert!(error(&format!("{MINIMAL}[[text_engine]]\nid = \"ace-step\"\nkind = \"anthropic\"\nmodel = \"m\"\n")).contains("ace-step"));
        assert!(error(&format!("{MINIMAL}[[text_engine]]\nid = \"c\"\nkind = \"anthropic\"\nmodel = \"m\"\napi_key_env = \"NOPE\"\n")).contains("NOPE is not set"));
    }

    #[test]
    fn the_api_token_is_at_least_32_characters_and_never_echoed() {
        let short = "too-short-secret";
        let e = format!("{:#}", parse(MINIMAL, |k| (k == API_TOKEN_ENV).then(|| short.to_string())).unwrap_err());
        assert!(e.contains("at least 32") && !e.contains(short), "{e}");
        let token = "x".repeat(32);
        assert_eq!(parse(MINIMAL, |k| (k == API_TOKEN_ENV).then(|| token.clone())).unwrap().api_token, Some(token));
    }

    #[test]
    fn the_example_config_parses() {
        let cfg = parse(include_str!("../../maio.example.toml"), no_env).unwrap();
        assert_eq!((cfg.engines[0].id.as_str(), cfg.data_dir.to_str()), ("ace-step", Some("/data")));
    }

    #[test]
    fn logs_are_text_unless_the_config_asks_for_json() {
        assert_eq!(parse(MINIMAL, no_env).unwrap().log_format, LogFormat::Text);
        assert_eq!(parse(&format!("log_format = \"json\"\n{MINIMAL}"), no_env).unwrap().log_format, LogFormat::Json);
        assert!(error(&format!("log_format = \"xml\"\n{MINIMAL}")).contains("xml"));
    }

    #[test]
    fn the_public_origin_is_an_http_origin_without_a_path() {
        let cfg = parse(&format!("public_origin = \"https://music.example/\"\n{MINIMAL}"), no_env).unwrap();
        assert_eq!(cfg.public_origin.as_deref(), Some("https://music.example"));
        assert!(error(&format!("public_origin = \"music.example\"\n{MINIMAL}")).contains("public_origin"));
        assert!(error(&format!("public_origin = \"https://music.example/app\"\n{MINIMAL}")).contains("public_origin"));
    }
}
