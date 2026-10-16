//! In-stream titles for MP3 and AAC listeners that ask for them (SPEC 10, Tags in the streams):
//! a metadata block after every `METAINT` audio bytes.

/// Audio bytes between metadata blocks (`icy-metaint`).
pub const METAINT: usize = 16_000;

const MAX_TEXT: usize = 255 * 16;

/// One metadata block: a length byte in 16-byte units, then `StreamTitle='...';` padded with zeros.
/// `None` is the empty block (one zero byte). Plain quotes inside the title become typographic ones,
/// so players that read up to the next `';` see the whole title.
pub fn block(title: Option<&str>) -> Vec<u8> {
    let Some(title) = title else { return vec![0] };
    let clean: String = title.replace('\'', "’").chars().filter(|c| !c.is_control()).collect();
    let mut body = clean.as_str();
    while format!("StreamTitle='{body}';").len() > MAX_TEXT {
        let cut = body.char_indices().last().map_or(0, |(i, _)| i);
        body = &body[..cut];
    }
    let text = format!("StreamTitle='{body}';");
    let units = text.len().div_ceil(16);
    let mut out = Vec::with_capacity(1 + units * 16);
    out.push(units as u8);
    out.extend_from_slice(text.as_bytes());
    out.resize(1 + units * 16, 0);
    out
}

/// Inserts a block after every `METAINT` audio bytes of one listener's stream.
pub struct Interleaver {
    left: usize,
    last: Option<String>,
}

impl Default for Interleaver {
    fn default() -> Self {
        Self::new()
    }
}

impl Interleaver {
    pub fn new() -> Self {
        Self { left: METAINT, last: None }
    }

    /// The chunk with blocks inserted where they fall; `title` is read at each block.
    pub fn push(&mut self, mut chunk: &[u8], title: impl Fn() -> String) -> Vec<u8> {
        let mut out = Vec::with_capacity(chunk.len() + 64);
        while !chunk.is_empty() {
            let n = self.left.min(chunk.len());
            out.extend_from_slice(&chunk[..n]);
            chunk = &chunk[n..];
            self.left -= n;
            if self.left == 0 {
                let t = title();
                let changed = self.last.as_deref() != Some(t.as_str());
                out.extend(block(changed.then_some(t.as_str())));
                self.last = Some(t);
                self.left = METAINT;
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_block_is_a_length_byte_and_padded_text() {
        let b = block(Some("Night Bus - Maio"));
        let text = b"StreamTitle='Night Bus - Maio';";
        assert_eq!(b[0] as usize, text.len().div_ceil(16));
        assert_eq!(b.len(), 1 + b[0] as usize * 16);
        assert_eq!(&b[1..1 + text.len()], text);
        assert!(b[1 + text.len()..].iter().all(|&x| x == 0));
        assert_eq!(block(None), vec![0]);
    }

    #[test]
    fn quotes_and_long_titles_stay_inside_one_block() {
        let b = block(Some("Don't Stop - It's Maio"));
        let text = String::from_utf8_lossy(&b[1..]).trim_end_matches('\0').to_string();
        assert_eq!(text.matches('\'').count(), 2, "only the delimiters are plain quotes: {text}");
        let long = block(Some(&"é".repeat(5000)));
        assert!(long.len() <= 1 + 255 * 16);
        assert_eq!(long[0] as usize * 16 + 1, long.len());
        assert!(String::from_utf8(long[1..].iter().copied().filter(|&x| x != 0).collect()).unwrap().ends_with("';"));
    }

    #[test]
    fn metadata_lands_every_metaint_bytes_and_repeats_only_on_change() {
        let mut i = Interleaver::new();
        let title = std::cell::RefCell::new("A - B".to_string());
        let mut out = i.push(&vec![1u8; METAINT + 100], || title.borrow().clone());
        let first = block(Some("A - B"));
        assert_eq!(&out[METAINT..METAINT + first.len()], &first[..]);
        assert_eq!(out.len(), METAINT + 100 + first.len());
        out = i.push(&vec![2u8; METAINT - 100], || title.borrow().clone());
        assert_eq!(*out.last().unwrap(), 0, "same title: an empty block");
        assert_eq!(out.len(), METAINT - 100 + 1);
        *title.borrow_mut() = "C - D".into();
        out = i.push(&vec![3u8; METAINT], || title.borrow().clone());
        assert_eq!(&out[METAINT..], &block(Some("C - D"))[..]);
    }
}
