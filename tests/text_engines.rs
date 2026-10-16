mod common;

use std::{sync::Arc, time::Duration};

use common::{FakeReply, spawn_fake_text, text_engine_toml};
use maio::{config, engine::Health, text::TextEngine};

fn engine(kind: &str, url: &str) -> Arc<TextEngine> {
    let text = format!("data_dir = \"/tmp/x\"\nmoods = [\"calm\"]\ngenres = [\"lo-fi\"]\n{}", text_engine_toml("t", kind, url));
    let cfg = config::parse(&text, |k| Some(format!("tok-{k}"))).unwrap();
    Arc::new(TextEngine::new(cfg.text_engines[0].clone()))
}

#[tokio::test]
async fn anthropic_engines_use_the_messages_api_with_their_key() {
    let fake = spawn_fake_text(FakeReply::Text("hello there")).await;
    let e = engine("anthropic", &fake.url);
    let got = e.complete("be brief", "say hello", 50, Duration::from_secs(5)).await.unwrap();
    assert_eq!(got, "hello there");
    let (path, headers, body) = fake.seen.lock().unwrap()[0].clone();
    assert_eq!(path, "/v1/messages");
    assert!(headers.contains("x-api-key: tok-T_TEXT") && headers.contains("anthropic-version: 2023-06-01"), "{headers}");
    assert_eq!((body["system"].as_str(), body["messages"][0]["content"].as_str(), body["max_tokens"].as_u64()), (Some("be brief"), Some("say hello"), Some(50)));
}

#[tokio::test]
async fn openai_compatible_engines_use_chat_completions() {
    let fake = spawn_fake_text(FakeReply::Text("hi")).await;
    let e = engine("openai", &fake.url);
    assert_eq!(e.complete("sys", "user", 10, Duration::from_secs(5)).await.unwrap(), "hi");
    let (path, headers, body) = fake.seen.lock().unwrap()[0].clone();
    assert_eq!(path, "/v1/chat/completions");
    assert!(!headers.contains("authorization"), "no key, no header");
    assert_eq!((body["messages"][0]["role"].as_str(), body["messages"][1]["content"].as_str()), (Some("system"), Some("user")));
}

#[tokio::test]
async fn errors_and_timeouts_fail_and_health_follows() {
    let down = spawn_fake_text(FakeReply::Status(500)).await;
    let e = engine("openai", &down.url);
    assert!(e.complete("s", "u", 5, Duration::from_secs(5)).await.is_err());
    e.check().await;
    assert_eq!(e.health(), Health::Down);
    let slow = spawn_fake_text(FakeReply::Hang).await;
    let started = std::time::Instant::now();
    assert!(engine("openai", &slow.url).complete("s", "u", 5, Duration::from_millis(200)).await.is_err());
    assert!(started.elapsed() < Duration::from_secs(2));
    let up = spawn_fake_text(FakeReply::Text("OK")).await;
    let e = engine("anthropic", &up.url);
    e.check().await;
    assert_eq!((e.health(), e.licence().as_str()), (Health::Up, "Anthropic commercial terms"));
}

#[tokio::test]
async fn a_hanging_naming_engine_times_out() {
    let slow = spawn_fake_text(FakeReply::Hang).await;
    let e = engine("openai", &slow.url);
    let n = maio::request::Normalized {
        prompt: "p".into(), seconds: 30, bpm: None, vocals: false, lyrics: None, seed: 1, engine: None, allow_noncommercial: false,
        mood: Some("calm".into()), genre: None, energy: None, title: "calm".into(), vocal_language: "en".into(), lyrics_by: None, user_prompt: None,
    };
    let started = std::time::Instant::now();
    assert!(maio::text::name_song(&e, &n, None, Duration::from_millis(200)).await.is_err());
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[tokio::test]
async fn thinking_is_never_kept_as_lyrics_or_titles() {
    let fake = spawn_fake_text(FakeReply::Text("<think>\nlet me think about rain\n</think>\n\n[verse]\nwet windows")).await;
    let e = engine("openai", &fake.url);
    let n = maio::request::Normalized {
        prompt: "p".into(), seconds: 30, bpm: None, vocals: true, lyrics: None, seed: 1, engine: None, allow_noncommercial: false,
        mood: Some("calm".into()), genre: None, energy: None, title: "calm".into(), vocal_language: "en".into(), lyrics_by: None, user_prompt: None,
    };
    assert_eq!(maio::text::write_lyrics(&e, &n, Duration::from_secs(5)).await.unwrap(), "[verse]\nwet windows");
    let named = spawn_fake_text(FakeReply::Text("<think>{\"title\": \"Wrong\"}</think>{\"title\": \"Night Bus\", \"description\": \"d\"}")).await;
    assert_eq!(maio::text::name_song(&engine("openai", &named.url), &n, None, Duration::from_secs(5)).await.unwrap().0, "Night Bus");
}
