//! The one account (SPEC 4): username and password rules, how long sessions last, sign-in limits.

/// A session without "keep me signed in": 12 hours.
pub const SESSION_S: u64 = 12 * 3600;
/// A session with "keep me signed in": 30 days.
pub const REMEMBER_S: u64 = 30 * 24 * 3600;
/// Failed sign-ins allowed per username in [`SIGN_IN_WINDOW_S`].
pub const SIGN_IN_FAILS: usize = 5;
pub const SIGN_IN_WINDOW_S: u64 = 15 * 60;

/// 2 to 32 characters from `a-z 0-9 . _ -`.
pub fn valid_username(u: &str) -> bool {
    (2..=32).contains(&u.len()) && u.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._-".contains(&b))
}

/// 10 to 128 characters.
pub fn valid_password(p: &str) -> bool {
    (10..=128).contains(&p.chars().count())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usernames_and_passwords_follow_the_rules() {
        assert!(valid_username("me") && valid_username("nelson.s_1-x"));
        for bad in ["m", "Me", "a b", "é", &"a".repeat(33)] {
            assert!(!valid_username(bad), "{bad}");
        }
        assert!(valid_password(&"é".repeat(10)) && valid_password(&"x".repeat(128)), "characters, not bytes");
        assert!(!valid_password(&"x".repeat(9)) && !valid_password(&"x".repeat(129)));
    }
}
