//! Time and ids from the system.

use crate::usecases::ports::{Clock, Ids};
use rand::{Rng, distr::Alphanumeric};

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> String {
        chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
    }

    fn epoch_s(&self) -> u64 {
        chrono::Utc::now().timestamp().max(0) as u64
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

    fn seed(&self) -> i64 {
        rand::random::<u32>() as i64
    }

    fn secret(&self) -> String {
        rand::random::<[u8; 32]>().iter().map(|b| format!("{b:02x}")).collect()
    }
}

#[cfg(test)]
mod tests {
    use crate::usecases::ports::{Clock, Ids};

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

    #[test]
    fn epoch_seconds_and_seeds_are_in_range() {
        assert!(SystemClock.epoch_s() > 1_700_000_000);
        assert!((0..=u32::MAX as i64).contains(&RandomIds.seed()));
    }

    #[test]
    fn secrets_are_64_hex_characters_and_never_repeat() {
        let s = RandomIds.secret();
        assert_eq!(s.len(), 64);
        assert!(s.bytes().all(|b| b.is_ascii_hexdigit()), "{s}");
        assert_ne!(s, RandomIds.secret());
    }
}
