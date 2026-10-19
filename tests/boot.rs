//! The real wiring: a config file, SQLite on disk, Argon2, and a restart.

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use serde_json::json;
use tower::ServiceExt;

fn sign_in() -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/v1/session")
        .header(header::HOST, "localhost:8080")
        .header(header::ORIGIN, "http://localhost:8080")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(json!({"username": "me", "password": "correct horse"}).to_string()))
        .unwrap()
}

#[tokio::test]
async fn the_server_boots_from_a_config_file_and_keeps_its_account() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("maio.toml");
    let toml = format!("data_dir = {:?}\nmoods = [\"calm\"]\n[[engine]]\nid = \"ace-step\"\nurl = \"http://127.0.0.1:9\"\n", dir.path().join("data"));
    std::fs::write(&path, toml).unwrap();
    let cfg = maio::web::config::load(&path, |_| None).unwrap();

    assert!(maio::web::start(&cfg, (None, None)).is_err(), "no account and no admin env: refuse to start");
    let app = maio::web::start(&cfg, (Some("me"), Some("correct horse"))).unwrap();
    assert!(dir.path().join("data/library.db").exists() && dir.path().join("data/songs").is_dir());
    let res = maio::web::router(app).oneshot(sign_in()).await.unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    let again = maio::web::start(&cfg, (None, None)).expect("a restart needs no admin env once the account exists");
    assert_eq!(maio::web::router(again).oneshot(sign_in()).await.unwrap().status(), StatusCode::NO_CONTENT);
}
