use std::{
    collections::{HashMap, VecDeque},
    sync::Mutex,
    time::Instant,
};

use crate::{
    config::{Principal, Rate, Scope},
    error::ApiError,
    store::User,
};

/// The caller: a signed-in person (SPEC 2, 8). There is no machine access.
#[derive(Debug, Clone)]
pub struct Auth {
    pub principal: Principal,
    pub user: User,
    pub session: String,
}

impl Auth {
    pub fn has(&self, s: Scope) -> bool {
        self.principal.scopes.contains(&s)
    }

    pub fn require(&self, s: Scope) -> Result<(), ApiError> {
        if self.has(s) {
            Ok(())
        } else {
            Err(ApiError::forbidden("missing_scope", format!("this caller lacks scope {s:?}").to_lowercase()))
        }
    }

    pub fn name(&self) -> &str {
        &self.principal.name
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LimitKind {
    Queue,
    Generate,
    Station,
    Write,
}

/// Sliding-window limits per person and kind (SPEC 2, Rate limits). Counters live in memory.
#[derive(Default)]
pub struct Limiter {
    hits: Mutex<HashMap<(String, LimitKind), VecDeque<Instant>>>,
}

impl Limiter {
    pub fn check(&self, t: &Principal, kind: LimitKind, now: Instant) -> Result<(), ApiError> {
        let rate: Rate = match kind {
            LimitKind::Queue => t.limits.queue,
            LimitKind::Generate => t.limits.generate,
            LimitKind::Station => t.limits.station,
            LimitKind::Write => t.limits.write,
        };
        let mut hits = self.hits.lock().unwrap();
        let q = hits.entry((t.name.clone(), kind)).or_default();
        while q.front().is_some_and(|&f| now.duration_since(f) >= rate.window) {
            q.pop_front();
        }
        if q.len() as u32 >= rate.max {
            let oldest = *q.front().expect("len >= max > 0");
            let wait = rate.window - now.duration_since(oldest);
            return Err(ApiError::rate_limited(Some(wait.as_secs().max(1))));
        }
        q.push_back(now);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Limits, parse_rate};
    use std::time::Duration;

    fn principal(name: &str) -> Principal {
        Principal {
            name: name.into(),
            scopes: Default::default(),
            limits: Limits {
                queue: parse_rate("2/10m").unwrap(),
                generate: parse_rate("20/10m").unwrap(),
                station: parse_rate("1/1h").unwrap(),
                write: parse_rate("60/1m").unwrap(),
                pending_jobs: 1,
            },
        }
    }

    #[test]
    fn limiter_blocks_after_max_and_reports_retry_after() {
        let l = Limiter::default();
        let t = principal("p");
        let t0 = Instant::now();
        assert!(l.check(&t, LimitKind::Queue, t0).is_ok());
        assert!(l.check(&t, LimitKind::Queue, t0 + Duration::from_secs(60)).is_ok());
        let err = l.check(&t, LimitKind::Queue, t0 + Duration::from_secs(120)).unwrap_err();
        assert_eq!(err.code, "rate_limited");
        assert_eq!(err.retry_after, Some(480));
    }

    #[test]
    fn limiter_window_slides_and_kinds_and_tokens_are_separate() {
        let l = Limiter::default();
        let (a, b) = (principal("a"), principal("b"));
        let t0 = Instant::now();
        assert!(l.check(&a, LimitKind::Station, t0).is_ok());
        assert!(l.check(&a, LimitKind::Station, t0).is_err());
        assert!(l.check(&b, LimitKind::Station, t0).is_ok());
        assert!(l.check(&a, LimitKind::Queue, t0).is_ok());
        assert!(l.check(&a, LimitKind::Station, t0 + Duration::from_secs(3600)).is_ok());
    }
}
