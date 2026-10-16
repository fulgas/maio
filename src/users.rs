use std::{collections::HashSet, sync::LazyLock};

use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::SaltString,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    config::{Limits, Principal, Scope},
    store::User,
};

/// A person's role; it decides their scopes (SPEC 8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Admin,
    Operator,
    Listener,
}

impl Role {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "admin" => Some(Self::Admin),
            "operator" => Some(Self::Operator),
            "listener" => Some(Self::Listener),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::Operator => "operator",
            Self::Listener => "listener",
        }
    }

    pub fn scopes(self) -> HashSet<Scope> {
        use Scope::*;
        match self {
            Self::Admin => [Read, Queue, Control, Admin].into(),
            Self::Operator => [Read, Queue, Control].into(),
            Self::Listener => [Read].into(),
        }
    }
}

pub fn valid_username(u: &str) -> bool {
    (2..=32).contains(&u.len()) && u.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._-".contains(&b))
}

pub fn valid_password(p: &str) -> bool {
    (10..=128).contains(&p.chars().count())
}

/// Argon2id with a fresh random salt, in PHC string form (`$argon2id$v=19$...`).
pub fn hash_password(p: &str) -> String {
    let salt = SaltString::encode_b64(&rand::random::<[u8; 16]>()).expect("16-byte salt encodes");
    Argon2::default().hash_password(p.as_bytes(), &salt).expect("argon2 hashing").to_string()
}

pub fn verify_password(p: &str, hash: &str) -> bool {
    PasswordHash::new(hash).is_ok_and(|h| Argon2::default().verify_password(p.as_bytes(), &h).is_ok())
}

static DUMMY: LazyLock<String> = LazyLock::new(|| hash_password("timing filler, never a real password"));

/// Spends the same time as a real check, so an unknown username looks like a wrong password.
pub fn verify_dummy(p: &str) {
    let _ = verify_password(p, &DUMMY);
}

/// 32 random bytes as hex: session ids.
pub fn new_secret() -> String {
    rand::random::<[u8; 32]>().iter().map(|b| format!("{b:02x}")).collect()
}

/// The secret part of a private radio's listen address (SPEC 7): 32 characters from A-Z a-z 0-9.
pub fn listen_key() -> String {
    use rand::Rng;
    rand::rng().sample_iter(&rand::distr::Alphanumeric).take(32).map(char::from).collect()
}

pub fn sha256_hex(s: &str) -> String {
    format!("{:x}", Sha256::digest(s.as_bytes()))
}

/// The principal a signed-in user acts as: their role's scopes and the configured limits.
pub fn principal(u: &User, limits: &Limits) -> Principal {
    Principal {
        name: format!("user:{}", u.username),
        scopes: Role::parse(&u.role).map_or_else(HashSet::new, Role::scopes),
        limits: limits.clone(),
    }
}

/// First start: with no users yet, creates the admin from `MAIO_ADMIN_USER` and
/// `MAIO_ADMIN_PASSWORD`, else refuses to start so a fresh install is never open (SPEC 8).
pub fn ensure_admin(store: &crate::store::Store, env: impl Fn(&str) -> Option<String>) -> anyhow::Result<()> {
    if !store.users()?.is_empty() {
        return Ok(());
    }
    let (Some(username), Some(password)) = (env("MAIO_ADMIN_USER"), env("MAIO_ADMIN_PASSWORD")) else {
        anyhow::bail!("no users yet: set MAIO_ADMIN_USER and MAIO_ADMIN_PASSWORD to create the first admin");
    };
    anyhow::ensure!(valid_username(&username), "MAIO_ADMIN_USER must be 2 to 32 characters from a-z 0-9 . _ -");
    anyhow::ensure!(valid_password(&password), "MAIO_ADMIN_PASSWORD must be 10 to 128 characters");
    store.insert_user(&User {
        name: username.clone(),
        username,
        role: Role::Admin.as_str().into(),
        hash: hash_password(&password),
        must_change: false,
        created_at: crate::store::now(),
        last_seen_at: None,
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Scope;

    #[test]
    fn roles_map_to_scopes() {
        assert!(Role::Admin.scopes().contains(&Scope::Admin));
        assert!(!Role::Admin.scopes().is_empty());
        let op = Role::Operator.scopes();
        assert!(op.contains(&Scope::Queue) && op.contains(&Scope::Control) && !op.contains(&Scope::Admin));
        assert_eq!(Role::Listener.scopes(), [Scope::Read].into());
        assert_eq!(Role::parse("operator"), Some(Role::Operator));
        assert_eq!(Role::parse("root"), None);
    }

    #[test]
    fn usernames_and_passwords_are_checked() {
        assert!(valid_username("ana.b-1_x"));
        for bad in ["a", "Ana", "ana b", "ana/x", &"a".repeat(33)] {
            assert!(!valid_username(bad), "{bad}");
        }
        assert!(!valid_password("123456789"));
        assert!(valid_password("1234567890"));
        assert!(!valid_password(&"x".repeat(129)));
    }

    #[test]
    fn hashes_are_argon2id_salted_and_verify() {
        let h1 = hash_password("correct horse battery");
        assert!(h1.starts_with("$argon2id$"), "{h1}");
        assert!(verify_password("correct horse battery", &h1));
        assert!(!verify_password("wrong horse battery", &h1));
        assert_ne!(h1, hash_password("correct horse battery"), "each hash has its own salt");
        assert!(!verify_password("anything", "not a hash"));
        verify_dummy("anything");
    }

    #[test]
    fn listen_keys_are_32_url_safe_characters() {
        let k = listen_key();
        assert_eq!(k.len(), 32);
        assert!(k.bytes().all(|b| b.is_ascii_alphanumeric()));
        assert_ne!(k, listen_key());
    }

    #[test]
    fn secrets_and_digests() {
        let a = new_secret();
        assert_eq!(a.len(), 64);
        assert!(a.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_ne!(a, new_secret());
        assert_eq!(sha256_hex("abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    #[test]
    fn a_user_becomes_a_principal_with_role_scopes() {
        let u = crate::store::User {
            username: "ana".into(), name: "Ana".into(), role: "operator".into(), hash: String::new(),
            must_change: false, created_at: String::new(), last_seen_at: None,
        };
        let limits = crate::config::resolve_limits(&crate::config::LimitsConfig::default()).unwrap();
        let t = principal(&u, &limits);
        assert_eq!(t.name, "user:ana");
        assert_eq!(t.scopes, Role::Operator.scopes());
        assert_eq!(t.limits.queue.max, 20);
    }
}

#[cfg(test)]
mod bootstrap_tests {
    use super::*;
    use crate::store::Store;

    #[test]
    fn first_admin_comes_from_env_or_start_is_refused() {
        let s = Store::open_in_memory().unwrap();
        let err = ensure_admin(&s, |_| None).unwrap_err();
        assert!(format!("{err:#}").contains("MAIO_ADMIN_USER"));
        assert!(ensure_admin(&s, |k| Some(if k == "MAIO_ADMIN_USER" { "nelson".into() } else { "short".into() })).is_err(), "weak password refused");
        ensure_admin(&s, |k| Some(if k == "MAIO_ADMIN_USER" { "nelson".into() } else { "a long enough password".into() })).unwrap();
        let u = s.user("nelson").unwrap().unwrap();
        assert_eq!(u.role, "admin");
        assert!(verify_password("a long enough password", &u.hash));
        ensure_admin(&s, |_| None).unwrap(); // users exist: nothing to do
    }
}
