mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use common::*;
use serde_json::json;

#[tokio::test]
async fn signing_in_sets_a_strict_http_only_cookie() {
    let w = world();
    let req = browser("POST", "/api/v1/session", None, Some(json!({"username": "me", "password": "correct horse", "remember": true})));
    let (status, headers, _) = send(&w, req).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let cookie = headers[header::SET_COOKIE].to_str().unwrap();
    for part in ["maio_session=", "HttpOnly", "SameSite=Strict", "Path=/", "Max-Age=2592000"] {
        assert!(cookie.contains(part), "{cookie}");
    }
    assert!(!cookie.contains("Secure"), "secure_cookies is off");
}

#[tokio::test]
async fn wrong_credentials_are_401_and_the_sixth_try_waits() {
    let w = world();
    let wrong = || browser("POST", "/api/v1/session", None, Some(json!({"username": "me", "password": "wrong password"})));
    let (status, _, body) = send(&w, wrong()).await;
    assert_eq!((status, body["error"]["code"].as_str()), (StatusCode::UNAUTHORIZED, Some("bad_credentials")));
    for _ in 0..4 {
        send(&w, wrong()).await;
    }
    let right = browser("POST", "/api/v1/session", None, Some(json!({"username": "me", "password": "correct horse"})));
    let (status, headers, _) = send(&w, right).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(headers[header::RETRY_AFTER], "900");
}

#[tokio::test]
async fn the_api_needs_a_session_or_the_token() {
    let w = world();
    assert_eq!(send(&w, browser("GET", "/api/v1/status", None, None)).await.0, StatusCode::UNAUTHORIZED);
    let session = sign_in(&w).await;
    let (status, _, body) = send(&w, browser("GET", "/api/v1/status", Some(&session), None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!((body["moods"].clone(), body["genres"].clone()), (json!(["calm"]), json!(["jazz"])));
    assert_eq!((body["engines"][0]["id"].as_str(), body["engines"][0]["health"].as_str()), (Some("ace-step"), Some("up")));
    assert_eq!((body["live"].clone(), body["disk"]["songs_bytes"].clone()), (json!([]), json!(0)));
    assert_eq!(body["relay"], json!(null), "no MediaMTX set up: nothing to report");
    assert_eq!(send(&w, script("GET", "/api/v1/status", None)).await.0, StatusCode::OK);
    let wrong = Request::builder().uri("/api/v1/status").header(header::AUTHORIZATION, "Bearer nope").body(Body::empty()).unwrap();
    assert_eq!(send(&w, wrong).await.0, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn changes_by_cookie_need_a_matching_origin_but_the_token_does_not() {
    let w = world();
    let session = sign_in(&w).await;
    let forged = Request::builder()
        .method("DELETE")
        .uri("/api/v1/session")
        .header(header::HOST, "localhost:8080")
        .header(header::ORIGIN, "http://evil.example")
        .header(header::COOKIE, format!("maio_session={session}"))
        .body(Body::empty())
        .unwrap();
    let (status, _, body) = send(&w, forged).await;
    assert_eq!((status, body["error"]["code"].as_str()), (StatusCode::FORBIDDEN, Some("bad_origin")));
    assert_eq!(send(&w, browser("GET", "/api/v1/status", Some(&session), None)).await.0, StatusCode::OK, "the session survived");
    let no_origin = Request::builder()
        .method("POST")
        .uri("/api/v1/session")
        .header(header::HOST, "localhost:8080")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(json!({"username": "me", "password": "correct horse"}).to_string()))
        .unwrap();
    assert_eq!(send(&w, no_origin).await.0, StatusCode::FORBIDDEN, "signing in needs the origin too");
    assert_eq!(send(&w, script("DELETE", "/api/v1/session", None)).await.0, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn signing_out_and_changing_the_password_end_sessions() {
    let w = world();
    let (a, b) = (sign_in(&w).await, sign_in(&w).await);
    let weak = browser("POST", "/api/v1/me/password", Some(&a), Some(json!({"current": "correct horse", "new": "short"})));
    let (status, _, body) = send(&w, weak).await;
    assert_eq!((status, body["error"]["code"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some("weak_password")));
    let change = browser("POST", "/api/v1/me/password", Some(&a), Some(json!({"current": "correct horse", "new": "a new password"})));
    assert_eq!(send(&w, change).await.0, StatusCode::NO_CONTENT);
    assert_eq!(send(&w, browser("GET", "/api/v1/status", Some(&b), None)).await.0, StatusCode::UNAUTHORIZED, "other sessions end");
    let (status, headers, _) = send(&w, browser("DELETE", "/api/v1/session", Some(&a), None)).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(headers[header::SET_COOKIE].to_str().unwrap().contains("Max-Age=0"));
    assert_eq!(send(&w, browser("GET", "/api/v1/status", Some(&a), None)).await.0, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn bad_bodies_and_unknown_paths_answer_in_the_error_format() {
    let w = world();
    let (status, _, body) = send(&w, browser("POST", "/api/v1/session", None, Some(json!({"username": "me"})))).await;
    assert_eq!((status, body["error"]["code"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some("invalid_body")));
    let (status, _, body) = send(&w, script("GET", "/api/v1/nope", None)).await;
    assert_eq!((status, body["error"]["code"].as_str()), (StatusCode::NOT_FOUND, Some("not_found")));
}

#[tokio::test]
async fn behind_a_proxy_the_public_origin_is_what_counts() {
    let w = world_behind_proxy();
    let sign_in = |origin: &str| {
        Request::builder()
            .method("POST")
            .uri("/api/v1/session")
            .header(header::HOST, "maio:8080")
            .header(header::ORIGIN, origin)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json!({"username": "me", "password": "correct horse"}).to_string()))
            .unwrap()
    };
    assert_eq!(send(&w, sign_in("https://music.example")).await.0, StatusCode::NO_CONTENT);
    assert_eq!(send(&w, sign_in("http://maio:8080")).await.0, StatusCode::FORBIDDEN, "the inner host no longer counts");
}
