//! The music engine's model: ACE-Step 1.5 as Maio sees it (arc42 3.3), and the choice of engine (arc42 8.2).

use serde::{Deserialize, Serialize};

use crate::domain::{Commercial, Error, Licence, request::Normalized};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Caps {
    pub min_seconds: u32,
    pub max_seconds: u32,
    pub vocals: bool,
    pub tempo: bool,
    pub concurrency: u32,
}

/// `GET /info` (arc42 3.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub licence: Licence,
    pub capabilities: Caps,
    /// Generation time divided by audio length.
    pub realtime_factor: f32,
    /// The DiT model ACE-Step has loaded, the planning (lyrics) model if loaded, and every DiT model installed.
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub lm_model: Option<String>,
    #[serde(default)]
    pub models: Vec<String>,
    /// Songs waiting and being made inside ACE-Step right now.
    #[serde(default)]
    pub queued: u32,
    #[serde(default)]
    pub running: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Health {
    Unknown,
    Up,
    Loading,
    Down,
    Incompatible,
}

/// One configured engine as Maio last saw it; `id` is its config id.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineStatus {
    pub id: String,
    pub info: Option<EngineInfo>,
    pub health: Health,
}

pub fn can_serve(info: &EngineInfo, n: &Normalized) -> bool {
    (info.capabilities.min_seconds..=info.capabilities.max_seconds).contains(&n.seconds) && (!n.vocals || info.capabilities.vocals)
}

/// Generation time from the realtime factor, plus 30 s to download and import.
/// The factor comes from the engine: the float-to-int cast saturates (NaN and negatives give 0), and so does the sum.
pub fn eta_s(info: &EngineInfo, seconds: u32) -> u32 {
    ((info.realtime_factor * seconds as f32) as u32).saturating_add(30)
}

/// The first healthy engine in config order that can meet the request, or the one the request names (arc42 8.2).
/// Returns its index in `engines`.
pub fn choose(engines: &[EngineStatus], n: &Normalized, exclude: &[String]) -> Result<usize, Error> {
    if let Some(want) = n.engine.as_ref().filter(|w| !engines.iter().any(|e| &e.id == *w)) {
        return Err(Error::invalid("unknown_engine", format!("no engine {want:?}")));
    }
    let mut blocked_noncommercial = false;
    for (i, e) in engines.iter().enumerate() {
        if exclude.contains(&e.id) || n.engine.as_ref().is_some_and(|w| w != &e.id) {
            continue;
        }
        let Some(info) = &e.info else { continue };
        if !can_serve(info, n) {
            if n.engine.is_some() {
                return Err(Error::invalid("engine_cannot_serve", format!("engine {} cannot make this request", e.id)));
            }
            continue;
        }
        if e.health != Health::Up {
            continue;
        }
        if info.licence.commercial == Commercial::No && !n.allow_noncommercial {
            blocked_noncommercial = true;
            continue;
        }
        return Ok(i);
    }
    Err(if blocked_noncommercial { Error::NoncommercialEngine } else { Error::EngineUnavailable })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::testing::{engine_info, normalized};

    fn engine(id: &str, commercial: Commercial, health: Health) -> EngineStatus {
        EngineStatus { id: id.into(), info: Some(engine_info(id, commercial)), health }
    }

    #[test]
    fn the_first_healthy_engine_in_order_is_chosen() {
        let engines =
            [engine("down", Commercial::Yes, Health::Down), engine("a", Commercial::Yes, Health::Up), engine("b", Commercial::Yes, Health::Up)];
        assert_eq!(choose(&engines, &normalized(), &[]), Ok(1));
        assert_eq!(choose(&engines, &normalized(), &["a".into()]), Ok(2), "excluded after a failure");
    }

    #[test]
    fn engines_without_info_or_in_another_protocol_are_never_chosen() {
        let unknown = EngineStatus { id: "new".into(), info: None, health: Health::Unknown };
        let other = engine("v2", Commercial::Yes, Health::Incompatible);
        assert_eq!(choose(&[unknown, other], &normalized(), &[]), Err(Error::EngineUnavailable));
    }

    #[test]
    fn a_noncommercial_engine_needs_the_flag() {
        let engines = [engine("nc", Commercial::No, Health::Up)];
        assert_eq!(choose(&engines, &normalized(), &[]), Err(Error::NoncommercialEngine));
        assert_eq!(choose(&engines, &Normalized { allow_noncommercial: true, ..normalized() }, &[]), Ok(0));
        let limited = [engine("sao", Commercial::Limited, Health::Up)];
        assert_eq!(choose(&limited, &normalized(), &[]), Ok(0), "limited is allowed");
    }

    #[test]
    fn a_named_engine_must_exist_and_be_able_to_meet_the_request() {
        let engines = [engine("a", Commercial::Yes, Health::Up)];
        let unknown = Normalized { engine: Some("zzz".into()), ..normalized() };
        assert_eq!(choose(&engines, &unknown, &[]).unwrap_err().code(), "unknown_engine");
        let too_long = Normalized { engine: Some("a".into()), seconds: 601, ..normalized() };
        assert_eq!(choose(&engines, &too_long, &[]).unwrap_err().code(), "engine_cannot_serve");
        let busy_elsewhere = [engine("b", Commercial::Yes, Health::Up), engine("a", Commercial::Yes, Health::Up)];
        assert_eq!(choose(&busy_elsewhere, &Normalized { engine: Some("a".into()), ..normalized() }, &[]), Ok(1));
    }

    #[test]
    fn can_serve_checks_length_and_vocals() {
        let mut info = engine_info("a", Commercial::Yes);
        info.capabilities.max_seconds = 47;
        info.capabilities.vocals = false;
        assert!(can_serve(&info, &Normalized { seconds: 30, ..normalized() }));
        assert!(!can_serve(&info, &Normalized { seconds: 48, ..normalized() }));
        assert!(!can_serve(&info, &Normalized { seconds: 30, vocals: true, ..normalized() }));
    }

    #[test]
    fn eta_is_generation_time_plus_thirty_seconds() {
        assert_eq!(eta_s(&engine_info("a", Commercial::Yes), 180), 120);
    }

    #[test]
    fn an_engine_reporting_a_silly_realtime_factor_cannot_overflow_the_eta() {
        // The factor comes from the engine's /info: untrusted.
        let mut info = engine_info("a", Commercial::Yes);
        info.realtime_factor = f32::MAX;
        assert_eq!(eta_s(&info, 600), u32::MAX);
        info.realtime_factor = -5.0;
        assert_eq!(eta_s(&info, 600), 30);
        info.realtime_factor = f32::NAN;
        assert_eq!(eta_s(&info, 600), 30);
    }

    #[test]
    fn info_parses_from_the_wire_without_a_licence_note() {
        let info: EngineInfo = serde_json::from_str(
            r#"{"protocol":1,"id":"ace-step","name":"ACE-Step","version":"1.5",
                "licence":{"name":"MIT","commercial":"yes","url":"https://x"},
                "capabilities":{"min_seconds":10,"max_seconds":600,"vocals":true,"tempo":true,"concurrency":1},
                "realtime_factor":0.43}"#,
        )
        .unwrap();
        assert_eq!((info.licence.note.as_str(), info.capabilities.concurrency), ("", 1));
    }
}
