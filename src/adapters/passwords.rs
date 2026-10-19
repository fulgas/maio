//! Password hashes with Argon2id (SPEC 4).

use std::sync::LazyLock;

use crate::usecases::ports::Passwords;
use anyhow::{Result, anyhow};
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier, password_hash::SaltString};

pub struct Argon2Passwords;

fn hash(password: &str) -> Result<String> {
    let salt = SaltString::encode_b64(&rand::random::<[u8; 16]>()).map_err(|e| anyhow!("making a salt: {e}"))?;
    Ok(Argon2::default().hash_password(password.as_bytes(), &salt).map_err(|e| anyhow!("hashing a password: {e}"))?.to_string())
}

/// A real hash to check against when the username does not exist, so both cases take the same time.
static DUMMY: LazyLock<String> = LazyLock::new(|| hash("timing filler, never a real password").expect("argon2 hashes a fixed string"));

impl Passwords for Argon2Passwords {
    fn hash(&self, password: &str) -> Result<String> {
        hash(password)
    }

    fn verify(&self, password: &str, hash: &str) -> bool {
        PasswordHash::new(hash).is_ok_and(|h| Argon2::default().verify_password(password.as_bytes(), &h).is_ok())
    }

    fn verify_nothing(&self, password: &str) {
        let _ = self.verify(password, &DUMMY);
    }
}

#[cfg(test)]
mod tests {
    use crate::usecases::ports::Passwords;

    use super::*;

    #[test]
    fn hashes_are_salted_argon2id_and_verify() {
        let a = Argon2Passwords.hash("correct horse").unwrap();
        assert!(a.starts_with("$argon2id$"), "{a}");
        assert_ne!(a, Argon2Passwords.hash("correct horse").unwrap(), "a fresh salt each time");
        assert!(Argon2Passwords.verify("correct horse", &a));
        assert!(!Argon2Passwords.verify("wrong horse", &a));
        assert!(!Argon2Passwords.verify("correct horse", "not a hash"));
        Argon2Passwords.verify_nothing("anything");
    }
}
