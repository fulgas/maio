//! Rate limits (SPEC 5, limits): at most `max` calls in any `window_s` seconds.

use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rate {
    pub max: u32,
    pub window_s: u64,
}

impl Rate {
    /// `"20/10m"`: 20 calls per 10 minutes. Units: s, m, h.
    pub fn parse(s: &str) -> Result<Self, String> {
        let bad = || format!("rate {s:?} must look like 20/10m: numbers above 0, unit s, m or h");
        let (max, window) = s.split_once('/').ok_or_else(bad)?;
        let max: u32 = max.trim().parse().map_err(|_| bad())?;
        let window = window.trim();
        let unit = match window.chars().last() {
            Some('s') => 1,
            Some('m') => 60,
            Some('h') => 3600,
            _ => return Err(bad()),
        };
        let count: u64 = window[..window.len() - 1].parse().map_err(|_| bad())?;
        let window_s = count.checked_mul(unit).ok_or_else(bad)?;
        if max == 0 || window_s == 0 {
            return Err(bad());
        }
        Ok(Self { max, window_s })
    }
}

/// When recent allowed calls happened, in seconds.
#[derive(Debug, Default)]
pub struct Window {
    hits: VecDeque<u64>,
}

impl Window {
    /// Allows a call at `now_s` and remembers it, or says how many seconds until one is allowed.
    pub fn allow(&mut self, rate: Rate, now_s: u64) -> Result<(), u64> {
        while self.hits.front().is_some_and(|&t| t + rate.window_s <= now_s) {
            self.hits.pop_front();
        }
        if self.hits.len() >= rate.max as usize {
            let oldest = self.hits.front().copied().unwrap_or(now_s);
            return Err((oldest + rate.window_s).saturating_sub(now_s).max(1));
        }
        self.hits.push_back(now_s);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rates_parse_like_the_config_writes_them() {
        assert_eq!(Rate::parse("20/10m"), Ok(Rate { max: 20, window_s: 600 }));
        assert_eq!(Rate::parse(" 5 / 30s "), Ok(Rate { max: 5, window_s: 30 }));
        assert_eq!(Rate::parse("1/2h"), Ok(Rate { max: 1, window_s: 7200 }));
        for bad in ["20", "x/10m", "20/10", "20/10d", "0/10m", "20/0m", "1/99999999999999999999h", "1/m"] {
            assert!(Rate::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_window_allows_max_calls_then_says_when_to_retry() {
        let rate = Rate { max: 2, window_s: 60 };
        let mut w = Window::default();
        assert_eq!(w.allow(rate, 100), Ok(()));
        assert_eq!(w.allow(rate, 110), Ok(()));
        assert_eq!(w.allow(rate, 120), Err(40), "the call at 100 leaves the window at 160");
        assert_eq!(w.allow(rate, 159), Err(1));
        assert_eq!(w.allow(rate, 160), Ok(()), "exactly one window later is allowed");
        assert_eq!(w.allow(rate, 161), Err(9));
    }
}
