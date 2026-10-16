//! Time and ids from the system.

use app::ports::{Clock, Ids};
use rand::{Rng, distr::Alphanumeric};

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> String {
        chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
    }
}

pub struct RandomIds;

impl Ids for RandomIds {
    /// ULIDs sort by creation time, which keeps same-second rows in order.
    fn id(&self) -> String {
        ulid::Ulid::generate().to_string()
    }

    fn key(&self) -> String {
        rand::rng().sample_iter(&Alphanumeric).take(32).map(char::from).collect()
    }
}

#[cfg(test)]
mod tests {
    use app::ports::{Clock, Ids};

    use super::*;

    #[test]
    fn the_clock_writes_utc_seconds() {
        let now = SystemClock.now();
        assert_eq!(now.len(), 20, "{now}");
        assert!(now.ends_with('Z') && now.as_bytes()[10] == b'T', "{now}");
    }

    #[test]
    fn ids_are_unique_and_keys_are_32_url_safe_characters() {
        assert_ne!(RandomIds.id(), RandomIds.id());
        let key = RandomIds.key();
        assert_eq!(key.len(), 32);
        assert!(key.bytes().all(|c| c.is_ascii_alphanumeric()), "{key}");
        assert_ne!(key, RandomIds.key());
    }
}
