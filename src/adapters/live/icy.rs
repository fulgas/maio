//! In-stream titles for MP3 and AAC listeners that ask for them (SPEC 9):
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
    fn a_block_is_padded_to_16_bytes_with_its_length_first() {
        let b = block(Some("Low Tide - Maio"));
        assert_eq!(b[0] as usize * 16 + 1, b.len());
        assert!(String::from_utf8_lossy(&b).contains("StreamTitle='Low Tide - Maio';"));
        assert_eq!(block(None), [0]);
    }

    #[test]
    fn quotes_in_titles_cannot_end_the_field() {
        let b = block(Some("Don't stop"));
        assert!(String::from_utf8_lossy(&b).contains("StreamTitle='Don’t stop';"));
    }

    #[test]
    fn blocks_land_every_metaint_bytes_and_repeat_only_on_change() {
        let mut i = Interleaver::new();
        let out = i.push(&vec![1u8; METAINT * 2], || "T".to_string());
        assert_eq!(out[METAINT] as usize, block(Some("T"))[0] as usize, "a title block after the first METAINT bytes");
        let second = METAINT + block(Some("T")).len() + METAINT;
        assert_eq!(out[second], 0, "same title: an empty block");
    }
}
