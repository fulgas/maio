use std::{collections::HashSet, path::PathBuf, time::Duration};

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    Read,
    Queue,
    Control,
    Admin,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default = "default_listen")]
    pub listen: String,
    pub data_dir: PathBuf,
    #[serde(default = "default_lookahead")]
    pub lookahead: usize,
    #[serde(default = "default_crossfade")]
    pub crossfade_s: f32,
    #[serde(default = "default_no_repeat")]
    pub no_repeat: u32,
    #[serde(default = "default_retention")]
    pub retention_days: u32,
    pub moods: Vec<String>,
    pub genres: Vec<String>,
    /// Rate limits for every signed-in person (SPEC 2, Rate limits).
    #[serde(default)]
    pub limits: LimitsConfig,
    #[serde(default)]
    pub relay: RelayConfig,
    /// Radios in total, main included (SPEC 10).
    #[serde(default = "default_max_radios")]
    pub max_radios: usize,
    #[serde(rename = "engine", default)]
    pub engines: Vec<EngineConfig>,
    #[serde(rename = "output", default)]
    pub outputs: Vec<OutputConfig>,
    #[serde(default)]
    pub serve: ServeConfig,
    /// Mark the session cookie `Secure`; set it when Maio is served over HTTPS.
    #[serde(default)]
    pub secure_cookies: bool,
    #[serde(default)]
    pub storage: StorageConfig,
    #[serde(default)]
    pub playlists: PlaylistsConfig,
    #[serde(rename = "text_engine", default)]
    pub text_engines: Vec<TextEngineConfig>,
    #[serde(default)]
    pub naming: NamingConfig,
    #[serde(default)]
    pub lyrics: LyricsConfig,
}

/// A MediaMTX service, so anyone can pull every radio over RTSP, SRT and RTMP (SPEC 7).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct RelayConfig {
    pub enabled: bool,
    /// Where Maio publishes, as seen from Maio's container.
    pub publish: String,
    /// MediaMTX's control API, as seen from Maio's container.
    pub api: String,
    /// The host name listeners use; it goes into the addresses Maio shows.
    pub host: String,
    /// The ports listeners use (as published on the host).
    pub rtsp_port: u16,
    pub srt_port: u16,
    pub rtmp_port: u16,
}

impl Default for RelayConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            publish: "rtsp://mediamtx:8554".into(),
            api: "http://mediamtx:9997".into(),
            host: "localhost".into(),
            rtsp_port: 8554,
            srt_port: 8890,
            rtmp_port: 1935,
        }
    }
}

/// How tracks are kept on disk (SPEC 6, Storage).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct StorageConfig {
    pub format: StoreFormat,
    /// Bitrate for the lossy formats, in kbit/s.
    pub bitrate_k: u32,
    /// Size budget for the crate in GB; 0 means no budget.
    pub max_gb: f64,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self { format: StoreFormat::Opus, bitrate_k: 160, max_gb: 20.0 }
    }
}

/// Playlist sizes (SPEC 10): the default for a new playlist and the largest anyone can set.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct PlaylistsConfig {
    pub max_tracks: u32,
    pub max_tracks_limit: u32,
}

impl Default for PlaylistsConfig {
    fn default() -> Self {
        Self { max_tracks: 50, max_tracks_limit: 500 }
    }
}

/// A language model that writes words, never music (SPEC 10, Text engines).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextEngineConfig {
    pub id: String,
    pub kind: TextKind,
    pub model: String,
    /// Required for `openai`; `anthropic` defaults to the Anthropic API.
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub api_key_env: Option<String>,
    /// Who wrote the words, for credits; defaults by kind.
    #[serde(default)]
    pub licence: Option<String>,
    #[serde(skip)]
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TextKind {
    Anthropic,
    Openai,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct NamingConfig {
    /// A text engine id; `None` names songs from lyrics and word lists only.
    pub engine: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct LyricsConfig {
    /// "ace-step" or a text engine id.
    pub default_engine: String,
    pub default_language: String,
}

impl Default for LyricsConfig {
    fn default() -> Self {
        Self { default_engine: "ace-step".into(), default_language: "en".into() }
    }
}

/// A language code such as `en` or `pt` (ISO 639-1 or -3, lowercase).
pub fn valid_language(s: &str) -> bool {
    (2..=3).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_lowercase())
}

impl Config {
    pub fn text_engine_ids(&self) -> Vec<String> {
        self.text_engines.iter().map(|t| t.id.clone()).collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum StoreFormat {
    Opus,
    Flac,
    Mp3,
    Aac,
}

impl StoreFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Opus => "opus",
            Self::Flac => "flac",
            Self::Mp3 => "mp3",
            Self::Aac => "m4a",
        }
    }
}

fn default_listen() -> String {
    "0.0.0.0:8080".into()
}
fn default_lookahead() -> usize {
    2
}
fn default_crossfade() -> f32 {
    6.0
}
fn default_no_repeat() -> u32 {
    20
}
fn default_retention() -> u32 {
    7
}
fn default_max_radios() -> usize {
    4
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LimitsConfig {
    pub queue: Option<String>,
    pub generate: Option<String>,
    pub station: Option<String>,
    pub write: Option<String>,
    pub pending_jobs: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EngineConfig {
    pub id: String,
    pub url: String,
    pub token_env: String,
    #[serde(skip)]
    pub token: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputKind {
    Rtsp,
    Srt,
    Rtmp,
    Icecast,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Codec {
    Opus,
    Aac,
    Mp3,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputConfig {
    pub kind: OutputKind,
    pub url: String,
    pub codec: Codec,
    #[serde(default = "default_bitrate")]
    pub bitrate_k: u32,
    /// The radio this output carries (SPEC 10).
    #[serde(default = "default_radio")]
    pub radio: String,
}

fn default_radio() -> String {
    "main".into()
}

fn default_bitrate() -> u32 {
    160
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServeConfig {
    #[serde(default)]
    pub mp3: bool,
    #[serde(default)]
    pub aac: bool,
    #[serde(default)]
    pub hls: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rate {
    pub max: u32,
    pub window: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Limits {
    pub queue: Rate,
    pub generate: Rate,
    pub station: Rate,
    pub write: Rate,
    pub pending_jobs: u32,
}

/// Who is calling: a signed-in person, with their role's scopes and the configured limits.
#[derive(Debug, Clone)]
pub struct Principal {
    pub name: String,
    pub scopes: HashSet<Scope>,
    pub limits: Limits,
}

/// Parses a rate such as `6/10m`: at most 6 calls per 10 minutes. Units: s, m, h.
pub fn parse_rate(s: &str) -> anyhow::Result<Rate> {
    let (max, window) = s.split_once('/').with_context(|| format!("rate {s:?} must look like 6/10m"))?;
    let max: u32 = max.trim().parse().with_context(|| format!("rate {s:?}: bad count"))?;
    let window = window.trim();
    let (num, unit) = window.split_at(window.len().saturating_sub(1));
    let num: u64 = num.parse().with_context(|| format!("rate {s:?}: bad window"))?;
    let secs = match unit {
        "s" => num,
        "m" => num * 60,
        "h" => num * 3600,
        _ => bail!("rate {s:?}: window unit must be s, m or h"),
    };
    if max == 0 || secs == 0 {
        bail!("rate {s:?}: count and window must be above 0");
    }
    Ok(Rate { max, window: Duration::from_secs(secs) })
}

pub fn resolve_limits(c: &LimitsConfig) -> anyhow::Result<Limits> {
    let rate = |v: &Option<String>, default: &str| parse_rate(v.as_deref().unwrap_or(default));
    Ok(Limits {
        queue: rate(&c.queue, "20/10m")?,
        generate: rate(&c.generate, "20/10m")?,
        station: rate(&c.station, "10/1h")?,
        write: rate(&c.write, "60/1m")?,
        pending_jobs: c.pending_jobs.unwrap_or(4),
    })
}

/// Parses config text and resolves secrets through `env`, so tests can supply a fake environment.
pub fn parse(text: &str, env: impl Fn(&str) -> Option<String>) -> anyhow::Result<Config> {
    let raw: toml::Table = toml::from_str(text).context("maio.toml")?;
    if raw.contains_key("token") {
        bail!("[[token]] sections are no longer supported: Maio has no machine access (SPEC 2); remove them from maio.toml");
    }
    let mut cfg: Config = toml::from_str(text).context("maio.toml")?;
    if cfg.lookahead == 0 {
        bail!("lookahead must be at least 1");
    }
    if cfg.moods.is_empty() {
        bail!("moods must not be empty");
    }
    resolve_limits(&cfg.limits).context("limits")?;
    if !(1..=32).contains(&cfg.max_radios) {
        bail!("max_radios must be 1 to 32");
    }
    for e in &mut cfg.engines {
        e.token = env(&e.token_env)
            .filter(|s| !s.is_empty())
            .with_context(|| format!("engine {}: env var {} is not set", e.id, e.token_env))?;
    }
    if !(cfg.storage.max_gb >= 0.0 && cfg.storage.max_gb.is_finite()) {
        bail!("storage.max_gb must be 0 or more");
    }
    if !(32..=512).contains(&cfg.storage.bitrate_k) {
        bail!("storage.bitrate_k must be 32 to 512");
    }
    let p = &cfg.playlists;
    if p.max_tracks == 0 || p.max_tracks > p.max_tracks_limit || p.max_tracks_limit > 10_000 {
        bail!("playlists: need 1 <= max_tracks <= max_tracks_limit <= 10000");
    }
    let mut seen = HashSet::new();
    for t in &mut cfg.text_engines {
        if t.id == "ace-step" || t.id.is_empty() || !seen.insert(t.id.clone()) {
            bail!("text_engine ids must be unique, not empty, and not \"ace-step\"");
        }
        if t.kind == TextKind::Openai && t.url.is_none() {
            bail!("text_engine {}: kind openai needs a url", t.id);
        }
        if t.kind == TextKind::Anthropic && t.api_key_env.is_none() {
            bail!("text_engine {}: kind anthropic needs api_key_env", t.id);
        }
        if let Some(var) = &t.api_key_env {
            t.api_key = Some(env(var).filter(|s| !s.is_empty()).with_context(|| format!("text_engine {}: env var {var} is not set", t.id))?);
        }
    }
    if let Some(n) = &cfg.naming.engine {
        if !seen.contains(n) {
            bail!("naming.engine {n:?} is not a text_engine id");
        }
    }
    if cfg.lyrics.default_engine != "ace-step" && !seen.contains(&cfg.lyrics.default_engine) {
        bail!("lyrics.default_engine must be \"ace-step\" or a text_engine id");
    }
    if !valid_language(&cfg.lyrics.default_language) {
        bail!("lyrics.default_language must be a language code such as \"en\"");
    }
    for o in &cfg.outputs {
        if o.kind == OutputKind::Rtmp && o.codec != Codec::Aac {
            bail!("output {}: rtmp needs codec aac", o.url);
        }
    }
    Ok(cfg)
}

pub fn load(path: &str) -> anyhow::Result<Config> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {path}"))?;
    parse(&text, |k| std::env::var(k).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: &str = r#"
data_dir = "/tmp/maio-test"
moods = ["calm"]
genres = ["lo-fi"]

[[engine]]
id = "ace-step"
url = "http://host.docker.internal:7801"
token_env = "T_ACE"
"#;

    fn env(k: &str) -> Option<String> {
        Some(format!("secret-{k}"))
    }

    #[test]
    fn playlists_default_to_50_of_at_most_500_and_must_be_ordered() {
        let cfg = parse(MIN, env).unwrap();
        assert_eq!((cfg.playlists.max_tracks, cfg.playlists.max_tracks_limit), (50, 500));
        let cfg = parse(&format!("{MIN}\n[playlists]\nmax_tracks = 10\nmax_tracks_limit = 20\n"), env).unwrap();
        assert_eq!((cfg.playlists.max_tracks, cfg.playlists.max_tracks_limit), (10, 20));
        assert!(parse(&format!("{MIN}\n[playlists]\nmax_tracks = 30\nmax_tracks_limit = 20\n"), env).is_err());
        assert!(parse(&format!("{MIN}\n[playlists]\nmax_tracks = 0\n"), env).is_err());
    }

    #[test]
    fn storage_defaults_to_opus_with_a_budget_and_can_be_changed() {
        let cfg = parse(MIN, env).unwrap();
        assert_eq!((cfg.storage.format, cfg.storage.bitrate_k, cfg.storage.max_gb), (StoreFormat::Opus, 160, 20.0));
        assert_eq!(cfg.storage.format.extension(), "opus");
        let text = format!("{MIN}\n[storage]\nformat = \"flac\"\nmax_gb = 0\n");
        let cfg = parse(&text, env).unwrap();
        assert_eq!((cfg.storage.format, cfg.storage.max_gb), (StoreFormat::Flac, 0.0));
        assert!(parse(&format!("{MIN}\n[storage]\nformat = \"wav\"\n"), env).is_err());
        assert!(parse(&format!("{MIN}\n[storage]\nmax_gb = -1\n"), env).is_err());
    }

    #[test]
    fn parses_rates() {
        assert_eq!(parse_rate("6/10m").unwrap(), Rate { max: 6, window: Duration::from_secs(600) });
        assert_eq!(parse_rate("2/1h").unwrap().window, Duration::from_secs(3600));
        assert_eq!(parse_rate("60/30s").unwrap().window, Duration::from_secs(30));
        for bad in ["6/10x", "x/1m", "0/1m", "6", "6/m"] {
            assert!(parse_rate(bad).is_err(), "{bad} should be rejected");
        }
    }

    #[test]
    fn limits_default_and_can_be_changed() {
        let cfg = parse(MIN, env).unwrap();
        assert_eq!(cfg.lookahead, 2);
        assert_eq!(cfg.listen, "0.0.0.0:8080");
        let l = resolve_limits(&cfg.limits).unwrap();
        assert_eq!((l.queue, l.pending_jobs), (parse_rate("20/10m").unwrap(), 4));
        let cfg = parse(&format!("limits = {{ queue = \"6/10m\", pending_jobs = 1 }}\n{MIN}"), env).unwrap();
        let l = resolve_limits(&cfg.limits).unwrap();
        assert_eq!((l.queue.max, l.pending_jobs), (6, 1));
        assert!(parse(&format!("limits = {{ queue = \"6/10x\" }}\n{MIN}"), env).is_err());
        assert_eq!(cfg.engines[0].token, "secret-T_ACE");
    }

    #[test]
    fn token_sections_are_refused_with_a_clear_message() {
        let text = format!("{MIN}\n[[token]]\nname = \"p\"\nenv = \"T\"\nscopes = [\"read\"]\n");
        let err = format!("{:#}", parse(&text, env).unwrap_err());
        assert!(err.contains("[[token]]") && err.contains("machine access"), "{err}");
    }

    #[test]
    fn missing_engine_secret_names_the_variable() {
        let err = parse(MIN, |k| (k != "T_ACE").then(|| format!("x{k}"))).unwrap_err();
        assert!(format!("{err:#}").contains("T_ACE"));
    }

    #[test]
    fn max_radios_defaults_to_4_and_stays_sane() {
        assert_eq!(parse(MIN, env).unwrap().max_radios, 4);
        assert_eq!(parse(&format!("max_radios = 2\n{MIN}"), env).unwrap().max_radios, 2);
        assert!(parse(&format!("max_radios = 0\n{MIN}"), env).is_err());
        assert!(parse(&format!("max_radios = 33\n{MIN}"), env).is_err());
    }

    #[test]
    fn the_relay_is_off_by_default_and_configurable() {
        let r = parse(MIN, env).unwrap().relay;
        assert_eq!((r.enabled, r.publish.as_str(), r.api.as_str()), (false, "rtsp://mediamtx:8554", "http://mediamtx:9997"));
        assert_eq!((r.host.as_str(), r.rtsp_port, r.srt_port, r.rtmp_port), ("localhost", 8554, 8890, 1935));
        let r = parse(&format!("relay = {{ enabled = true, host = \"radio.lan\" }}\n{MIN}"), env).unwrap().relay;
        assert_eq!((r.enabled, r.host.as_str(), r.rtsp_port), (true, "radio.lan", 8554));
    }

    #[test]
    fn rtmp_needs_aac() {
        let text = format!("{MIN}\n[[output]]\nkind = \"rtmp\"\nurl = \"rtmp://x/live\"\ncodec = \"opus\"\n");
        assert!(parse(&text, env).is_err());
    }

    #[test]
    fn unknown_keys_are_rejected() {
        assert!(parse(&format!("bogus = 1\n{MIN}"), env).is_err());
    }

    #[test]
    fn text_engines_naming_and_lyrics_are_checked() {
        let cfg = parse(MIN, env).unwrap();
        assert!(cfg.text_engines.is_empty());
        assert_eq!((cfg.naming.engine, cfg.lyrics.default_engine.as_str(), cfg.lyrics.default_language.as_str()), (None, "ace-step", "en"));
        let ok = format!("{MIN}
[[text_engine]]
id = \"claude\"
kind = \"anthropic\"
model = \"claude-haiku-4-5\"
api_key_env = \"T_ANTHROPIC\"

[[text_engine]]
id = \"local\"
kind = \"openai\"
url = \"http://host.docker.internal:8081/v1\"
model = \"qwen3-8b\"

[naming]
engine = \"local\"

[lyrics]
default_engine = \"claude\"
default_language = \"pt\"
");
        let cfg = parse(&ok, env).unwrap();
        assert_eq!(cfg.text_engines[0].api_key.as_deref(), Some("secret-T_ANTHROPIC"));
        assert_eq!(cfg.text_engines[1].api_key, None, "openai keys are optional");
        assert_eq!(cfg.text_engine_ids(), ["claude", "local"]);
        let bad = |extra: &str| parse(&format!("{MIN}\n{extra}"), env).is_err();
        assert!(bad("[[text_engine]]\nid = \"c\"\nkind = \"anthropic\"\nmodel = \"m\"\n"), "anthropic needs a key");
        assert!(bad("[[text_engine]]\nid = \"l\"\nkind = \"openai\"\nmodel = \"m\"\n"), "openai needs a url");
        assert!(bad("[[text_engine]]\nid = \"ace-step\"\nkind = \"openai\"\nurl = \"http://x\"\nmodel = \"m\"\n"), "ace-step is taken");
        assert!(bad("[naming]\nengine = \"nope\"\n"));
        assert!(bad("[naming]\nengine = \"ace-step\"\n"), "ace-step writes lyrics only");
        assert!(bad("[lyrics]\ndefault_engine = \"nope\"\n"));
        assert!(bad("[lyrics]\ndefault_language = \"English\"\n"));
        assert!(parse(&format!("{MIN}\n[[text_engine]]\nid = \"c\"\nkind = \"anthropic\"\nmodel = \"m\"\napi_key_env = \"T\"\n"), |_| None).is_err(), "the key env var must be set");
    }

    #[test]
    fn the_example_config_parses() {
        let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/maio.example.toml")).unwrap();
        let cfg = parse(&text, env).unwrap();
        assert_eq!(cfg.lyrics.default_engine, "ace-step");
        assert!(text.contains("[[text_engine]]"), "the example shows text engines");
    }
}
