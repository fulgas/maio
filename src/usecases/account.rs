//! Signing in (SPEC 4): one account from `.env`, sessions, sign-in limits, the API token.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard},
};

use crate::domain::{
    Error as Rule,
    account::{REMEMBER_S, SESSION_S, SIGN_IN_FAILS, SIGN_IN_WINDOW_S, valid_password, valid_username},
    limits::Failures,
};
use anyhow::{Context, bail};

use crate::usecases::{
    Result,
    ports::{AccountStore, Clock, Ids, Passwords},
};

/// Failed sign-ins allowed per client address in the window, across usernames.
const CLIENT_FAILS: usize = 20;
/// Failure keys tracked before stale ones are swept.
const MAX_KEYS: usize = 1000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedIn {
    pub session: String,
    pub max_age_s: u64,
}

pub struct Account {
    store: Arc<dyn AccountStore>,
    passwords: Arc<dyn Passwords>,
    ids: Arc<dyn Ids>,
    clock: Arc<dyn Clock>,
    api_token: Option<String>,
    fails: Mutex<HashMap<String, Failures>>,
    /// Sign-ins are checked one at a time: Argon2 takes about 19 MiB per check, and a failure must be
    /// recorded before the next try is judged, or parallel guesses would slip past the limit.
    checks: Mutex<()>,
}

/// Equal length and equal bytes, taking the same time wherever they differ.
fn same_bytes(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

impl Account {
    pub fn new(
        store: Arc<dyn AccountStore>,
        passwords: Arc<dyn Passwords>,
        ids: Arc<dyn Ids>,
        clock: Arc<dyn Clock>,
        api_token: Option<String>,
    ) -> Self {
        Self { store, passwords, ids, clock, api_token, fails: Mutex::default(), checks: Mutex::default() }
    }

    /// On first start the account comes from `MAIO_ADMIN_USER` and `MAIO_ADMIN_PASSWORD`; without them
    /// and without an account, Maio refuses to start (SPEC 4). Later starts ignore them.
    pub fn ensure(&self, username: Option<&str>, password: Option<&str>) -> anyhow::Result<()> {
        if self.store.account()?.is_some() {
            return Ok(());
        }
        let (Some(username), Some(password)) = (username, password) else {
            bail!("there is no account yet: set MAIO_ADMIN_USER and MAIO_ADMIN_PASSWORD in .env");
        };
        if !valid_username(username) {
            bail!("MAIO_ADMIN_USER must be 2 to 32 characters from a-z, 0-9, '.', '_' and '-'");
        }
        if !valid_password(password) {
            bail!("MAIO_ADMIN_PASSWORD must be 10 to 128 characters");
        }
        self.store.set_account(username, &self.passwords.hash(password)?)
    }

    /// Checks the username and password and starts a session. Slow on purpose: call it on a blocking thread.
    pub fn sign_in(&self, username: &str, password: &str, remember: bool, client: Option<&str>) -> Result<SignedIn> {
        let _one_at_a_time = self.checks.lock().unwrap_or_else(|e| e.into_inner());
        let now = self.clock.epoch_s();
        // Junk usernames track nothing of their own, but they count against the client.
        let real_name = valid_username(username);
        let mut keys: Vec<(String, usize)> = Vec::new();
        if real_name {
            keys.push((format!("user:{username}"), SIGN_IN_FAILS));
        }
        keys.extend(client.map(|c| (format!("client:{c}"), CLIENT_FAILS)));
        {
            let mut fails = self.fails();
            for (key, max) in &keys {
                if let Some(retry_after_s) = fails.get_mut(key).and_then(|f| f.blocked(*max, SIGN_IN_WINDOW_S, now)) {
                    return Err(Rule::RateLimited { retry_after_s }.into());
                }
            }
        }
        let verified = match self.store.account()? {
            Some((name, hash)) if real_name && name == username => self.passwords.verify(password, &hash),
            _ => {
                self.passwords.verify_nothing(password);
                false
            }
        };
        if !verified {
            let mut fails = self.fails();
            if fails.len() > MAX_KEYS {
                fails.retain(|_, f| {
                    f.blocked(usize::MAX, SIGN_IN_WINDOW_S, now);
                    !f.is_empty()
                });
            }
            for (key, _) in &keys {
                fails.entry(key.clone()).or_default().fail(now);
            }
            return Err(Rule::BadCredentials.into());
        }
        self.fails().remove(&keys[0].0);
        self.store.delete_expired_sessions(now)?;
        let max_age_s = if remember { REMEMBER_S } else { SESSION_S };
        let session = self.ids.secret();
        self.store.create_session(&session, now + max_age_s)?;
        Ok(SignedIn { session, max_age_s })
    }

    pub fn is_signed_in(&self, session: &str) -> Result<bool> {
        Ok(self.store.session_valid(session, self.clock.epoch_s())?)
    }

    pub fn sign_out(&self, session: &str) -> Result<()> {
        Ok(self.store.delete_session(session)?)
    }

    /// Changes the password and ends every other session (SPEC 4). `session` is `None` for the API token, which
    /// ends them all. Slow on purpose: call it on a blocking thread.
    pub fn change_password(&self, session: Option<&str>, current: &str, new: &str) -> Result<()> {
        let (username, hash) = self.store.account()?.context("there is no account")?;
        if !self.passwords.verify(current, &hash) {
            return Err(Rule::BadCredentials.into());
        }
        if !valid_password(new) {
            return Err(Rule::invalid("weak_password", "a password is 10 to 128 characters").into());
        }
        self.store.set_account(&username, &self.passwords.hash(new)?)?;
        self.store.delete_other_sessions(session.unwrap_or(""))?;
        Ok(())
    }

    /// `Authorization: Bearer <token>` (SPEC 4), compared in constant time. With no token configured, none works.
    pub fn is_api_token(&self, token: &str) -> bool {
        self.api_token.as_deref().is_some_and(|t| same_bytes(t.as_bytes(), token.as_bytes()))
    }

    fn fails(&self) -> MutexGuard<'_, HashMap<String, Failures>> {
        self.fails.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, atomic::Ordering};

    use super::*;
    use crate::usecases::testing::{FixedClock, MemAccount, PlainPasswords, SeqIds};

    struct World {
        account: Account,
        store: Arc<MemAccount>,
        passwords: Arc<PlainPasswords>,
    }

    fn bare(token: Option<&str>) -> World {
        let (store, passwords) = (Arc::new(MemAccount::default()), Arc::new(PlainPasswords::default()));
        let account = Account::new(store.clone(), passwords.clone(), Arc::new(SeqIds::default()), Arc::new(FixedClock("t")), token.map(String::from));
        World { account, store, passwords }
    }

    fn world(token: Option<&str>) -> World {
        let w = bare(token);
        w.account.ensure(Some("me"), Some("correct horse")).unwrap();
        w
    }

    fn code(e: crate::usecases::Error) -> &'static str {
        e.rule().map(Rule::code).unwrap_or("port")
    }

    #[test]
    fn the_account_is_made_once_from_the_env() {
        let w = world(None);
        assert_eq!(*w.store.account.lock().unwrap(), Some(("me".into(), "plain:correct horse".into())));
        w.account.ensure(Some("other"), Some("another password")).unwrap();
        assert_eq!(w.store.account.lock().unwrap().as_ref().unwrap().0, "me", "later starts ignore the env");
        let empty = bare(None);
        assert!(empty.account.ensure(None, None).unwrap_err().to_string().contains("MAIO_ADMIN_USER"));
        assert!(empty.account.ensure(Some("me"), Some("short")).unwrap_err().to_string().contains("10 to 128"));
        assert!(empty.account.ensure(Some("Me!"), Some("long enough pw")).is_err());
        assert!(empty.store.account.lock().unwrap().is_none());
    }

    #[test]
    fn signing_in_starts_a_session_of_12_hours_or_30_days() {
        let w = world(None);
        let s = w.account.sign_in("me", "correct horse", false, None).unwrap();
        assert_eq!((s.session.len(), s.max_age_s), (64, 12 * 3600));
        assert_eq!(w.store.sessions.lock().unwrap()[0], (s.session.clone(), 1_000_000 + 12 * 3600));
        assert!(w.account.is_signed_in(&s.session).unwrap());
        assert_eq!(w.account.sign_in("me", "correct horse", true, None).unwrap().max_age_s, 30 * 24 * 3600);
        w.account.sign_out(&s.session).unwrap();
        assert!(!w.account.is_signed_in(&s.session).unwrap());
        w.store.sessions.lock().unwrap().push(("old".into(), 999_999));
        assert!(!w.account.is_signed_in("old").unwrap(), "an expired session is no session");
    }

    #[test]
    fn a_wrong_username_and_a_wrong_password_look_the_same() {
        let w = world(None);
        assert_eq!(code(w.account.sign_in("me", "wrong password", false, None).unwrap_err()), "bad_credentials");
        assert_eq!(code(w.account.sign_in("you", "correct horse", false, None).unwrap_err()), "bad_credentials");
        assert_eq!(code(w.account.sign_in("No Such!", "x", false, None).unwrap_err()), "bad_credentials");
        assert_eq!(w.passwords.dummy_checks.load(Ordering::SeqCst), 2, "unknown names spend a password check too");
        assert!(w.store.sessions.lock().unwrap().is_empty());
    }

    #[test]
    fn five_failures_in_15_minutes_block_further_tries() {
        let w = world(None);
        for _ in 0..5 {
            w.account.sign_in("me", "wrong password", false, None).unwrap_err();
        }
        let e = w.account.sign_in("me", "correct horse", false, None).unwrap_err();
        assert_eq!(e.rule(), Some(&Rule::RateLimited { retry_after_s: 900 }), "even the right password waits");
    }

    #[test]
    fn one_client_is_limited_across_usernames() {
        let w = world(None);
        for i in 0..20 {
            w.account.sign_in(&format!("guess{i}"), "x", false, Some("10.0.0.9")).unwrap_err();
        }
        assert_eq!(code(w.account.sign_in("me", "correct horse", false, Some("10.0.0.9")).unwrap_err()), "rate_limited");
        assert!(w.account.sign_in("me", "correct horse", false, Some("10.0.0.10")).is_ok(), "another client still gets in");
    }

    #[test]
    fn changing_the_password_ends_every_other_session() {
        let w = world(None);
        let mine = w.account.sign_in("me", "correct horse", false, None).unwrap().session;
        let other = w.account.sign_in("me", "correct horse", false, None).unwrap().session;
        assert_eq!(code(w.account.change_password(Some(&mine), "wrong", "a new password").unwrap_err()), "bad_credentials");
        assert_eq!(code(w.account.change_password(Some(&mine), "correct horse", "short").unwrap_err()), "weak_password");
        w.account.change_password(Some(&mine), "correct horse", "a new password").unwrap();
        assert!(w.account.is_signed_in(&mine).unwrap() && !w.account.is_signed_in(&other).unwrap());
        assert!(w.account.sign_in("me", "a new password", false, None).is_ok());
        w.account.change_password(None, "a new password", "a third password").unwrap();
        assert!(w.store.sessions.lock().unwrap().is_empty(), "the API token ends them all");
    }

    #[test]
    fn the_api_token_must_match_exactly() {
        let token = "t".repeat(32);
        let w = world(Some(&token));
        assert!(w.account.is_api_token(&token));
        assert!(!w.account.is_api_token(&"t".repeat(31)) && !w.account.is_api_token(&"u".repeat(32)) && !w.account.is_api_token(""));
        assert!(!world(None).account.is_api_token(""), "with no token configured, none works");
    }

    /// Checks take a while, as Argon2 does, so parallel tries overlap.
    struct SlowPasswords(PlainPasswords);

    impl Passwords for SlowPasswords {
        fn hash(&self, p: &str) -> anyhow::Result<String> {
            self.0.hash(p)
        }

        fn verify(&self, p: &str, h: &str) -> bool {
            std::thread::sleep(std::time::Duration::from_millis(5));
            self.0.verify(p, h)
        }

        fn verify_nothing(&self, p: &str) {
            std::thread::sleep(std::time::Duration::from_millis(5));
            self.0.verify_nothing(p)
        }
    }

    #[test]
    fn parallel_guesses_still_get_only_five_tries() {
        let account = Account::new(
            Arc::new(MemAccount::default()),
            Arc::new(SlowPasswords(PlainPasswords::default())),
            Arc::new(SeqIds::default()),
            Arc::new(FixedClock("t")),
            None,
        );
        account.ensure(Some("me"), Some("correct horse")).unwrap();
        let wrong = std::thread::scope(|s| {
            let tries: Vec<_> = (0..20).map(|_| s.spawn(|| account.sign_in("me", "wrong password", false, None))).collect();
            tries.into_iter().map(|t| t.join().unwrap()).filter(|r| matches!(r.as_ref().unwrap_err().rule(), Some(Rule::BadCredentials))).count()
        });
        assert_eq!(wrong, 5, "the other 15 are told to wait");
    }

    #[test]
    fn junk_usernames_count_against_the_client() {
        let w = world(None);
        for _ in 0..20 {
            w.account.sign_in("!!", "x", false, Some("10.0.0.9")).unwrap_err();
        }
        assert_eq!(code(w.account.sign_in("me", "correct horse", false, Some("10.0.0.9")).unwrap_err()), "rate_limited");
    }
}
