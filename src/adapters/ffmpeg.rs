//! Song files with ffmpeg (arc42 8.2): import, tags, delete.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

use crate::domain::{DownloadFormat, Song};
use crate::usecases::ports::{AudioFiles, Exported, Imported};
use anyhow::{Context, Result, bail, ensure};

/// `[storage] format` (arc42 7.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreFormat {
    Opus,
    Flac,
    Mp3,
    Aac,
}

impl StoreFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Opus => "opus",
            Self::Flac => "flac",
            Self::Mp3 => "mp3",
            Self::Aac => "m4a",
        }
    }

    /// Set explicitly, so the file name never picks the container.
    fn container(self) -> &'static str {
        match self {
            Self::Opus => "ogg",
            Self::Flac => "flac",
            Self::Mp3 => "mp3",
            Self::Aac => "mp4",
        }
    }
}

fn run(cmd: &mut Command, what: &str) -> Result<Vec<u8>> {
    let out = cmd.output().with_context(|| format!("running {what}"))?;
    if !out.status.success() {
        bail!("{what} failed: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(out.stdout)
}

/// A file's length in seconds.
pub fn duration(path: &Path) -> Result<f64> {
    let out = run(Command::new("ffprobe").args(["-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0"]).arg(path), "ffprobe")?;
    String::from_utf8_lossy(&out).trim().parse().context("reading ffprobe's duration")
}

/// What goes into a file's tags (arc42 8.2): title, artist, genre, BPM, lyrics, and a comment with engine, seed and licence.
fn tags(song: &Song) -> Vec<(&'static str, String)> {
    let mut t = vec![
        ("title", song.title.clone()),
        ("artist", song.artist.clone()),
        ("comment", format!("engine {}, seed {}, licence {}", song.engine, song.seed, song.licence.name)),
    ];
    t.extend(song.genre.clone().map(|g| ("genre", g)));
    t.extend(song.bpm.map(|b| ("bpm", b.to_string())));
    t.extend(song.lyrics.clone().map(|l| ("lyrics", l)));
    t
}

/// Rewrites a file's tags, copying the audio as it is. Ogg keeps Vorbis comments on the stream;
/// the other containers keep them on the file. Each write has its own temp file, so two never mix.
fn write_tags(path: &Path, pairs: &[(&str, String)]) -> Result<()> {
    static WRITES: AtomicUsize = AtomicUsize::new(0);
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or_default().to_string();
    let format = match ext.as_str() {
        "opus" | "ogg" => "ogg",
        "flac" => "flac",
        "mp3" => "mp3",
        "m4a" | "mp4" => "mp4",
        other => bail!("cannot tag a .{other} file"),
    };
    let tmp = path.with_extension(format!("tagging-{}-{}.{ext}", std::process::id(), WRITES.fetch_add(1, Ordering::SeqCst)));
    let target = if format == "ogg" { "-metadata:s:a:0" } else { "-metadata" };
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-hide_banner", "-v", "error", "-y", "-i"]).arg(path).args(["-map", "0", "-c", "copy", "-map_metadata", "-1"]);
    for (k, v) in pairs {
        cmd.arg(target).arg(format!("{k}={v}"));
    }
    if format == "mp3" {
        cmd.args(["-id3v2_version", "3"]);
    }
    cmd.args(["-f", format]).arg(&tmp);
    if let Err(e) = run(&mut cmd, "ffmpeg tags") {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    fs::rename(&tmp, path).context("replacing the tagged file")
}

/// Song files live under `songs/`; anything else (absolute, `..`, another folder) is refused.
fn song_path(data_dir: &Path, file: &str) -> Result<PathBuf> {
    let path = Path::new(file);
    let inside =
        path.starts_with("songs") && path.components().count() > 1 && path.components().all(|c| matches!(c, std::path::Component::Normal(_)));
    ensure!(inside, "refusing to touch {file:?}: song files live under songs/");
    Ok(data_dir.join(path))
}

pub struct Ffmpeg {
    data_dir: PathBuf,
    format: StoreFormat,
    bitrate_k: u32,
}

impl Ffmpeg {
    /// Creates `songs/` and `tmp/` under `data_dir`.
    pub fn new(data_dir: impl Into<PathBuf>, format: StoreFormat, bitrate_k: u32) -> Result<Self> {
        let data_dir = data_dir.into();
        for dir in ["songs", "tmp"] {
            fs::create_dir_all(data_dir.join(dir)).with_context(|| format!("creating {dir}/ in {}", data_dir.display()))?;
        }
        Ok(Self { data_dir, format, bitrate_k })
    }
}

impl AudioFiles for Ffmpeg {
    fn scratch(&self, name: &str) -> PathBuf {
        self.data_dir.join("tmp").join(name)
    }

    /// Loudness-normalised to -16 LUFS, 48 kHz stereo, in the storage format (arc42 8.2).
    fn import(&self, wav: &Path, song_id: &str) -> Result<Imported> {
        let file = format!("songs/{song_id}.{}", self.format.extension());
        let dst = self.data_dir.join(&file);
        let bitrate = format!("{}k", self.bitrate_k);
        let encode: Vec<&str> = match self.format {
            StoreFormat::Opus => vec!["-c:a", "libopus", "-b:a", &bitrate],
            StoreFormat::Flac => vec!["-c:a", "flac"],
            StoreFormat::Mp3 => vec!["-c:a", "libmp3lame", "-b:a", &bitrate],
            StoreFormat::Aac => vec!["-c:a", "aac", "-b:a", &bitrate],
        };
        run(
            Command::new("ffmpeg")
                .args(["-hide_banner", "-v", "error", "-y", "-i"])
                .arg(wav)
                // loudnorm emits huge frames; asetnsamples keeps them under FLAC's 65535-sample block limit
                .args(["-af", "loudnorm=I=-16:TP=-1.5:LRA=11,aresample=48000,asetnsamples=n=4096:p=0", "-ac", "2"])
                .args(&encode)
                .args(["-f", self.format.container()])
                .arg(&dst),
            "ffmpeg import",
        )?;
        let seconds = duration(&dst)?;
        let bytes = fs::metadata(&dst).context("reading the imported file's size")?.len();
        Ok(Imported { file, seconds, bytes })
    }

    fn retag(&self, song: &Song) -> Result<()> {
        write_tags(&song_path(&self.data_dir, &song.file)?, &tags(song))
    }

    /// The stored file when it is already in `format`, else a converted copy in `tmp/` with the song's tags (arc42 3.5).
    fn export(&self, song: &Song, format: DownloadFormat) -> Result<Exported> {
        let src = song_path(&self.data_dir, &song.file)?;
        ensure!(src.is_file(), "song {} has no file at {}", song.id, song.file);
        if src.extension().and_then(|e| e.to_str()) == Some(format.extension()) {
            return Ok(Exported { path: src, temporary: false });
        }
        static N: AtomicUsize = AtomicUsize::new(0);
        let dst = self.scratch(&format!("export-{}-{}-{}.{}", song.id, std::process::id(), N.fetch_add(1, Ordering::SeqCst), format.extension()));
        fs::create_dir_all(dst.parent().expect("scratch files sit in tmp/")).context("making the tmp folder")?;
        let (codec, container): (&[&str], &str) = match format {
            DownloadFormat::Mp3 => (&["-c:a", "libmp3lame", "-b:a", "192k", "-id3v2_version", "3"], "mp3"),
            DownloadFormat::Aac => (&["-c:a", "aac", "-b:a", "192k"], "ipod"),
            DownloadFormat::Flac => (&["-c:a", "flac"], "flac"),
            DownloadFormat::Wav => (&["-c:a", "pcm_s16le"], "wav"),
            DownloadFormat::Opus => (&["-c:a", "libopus", "-b:a", "160k"], "ogg"),
        };
        let target = if format == DownloadFormat::Opus { "-metadata:s:a:0" } else { "-metadata" };
        let mut cmd = Command::new("ffmpeg");
        cmd.args(["-hide_banner", "-v", "error", "-y", "-i"]).arg(&src).args(["-map", "0:a", "-map_metadata", "-1"]).args(codec);
        for (k, v) in tags(song) {
            cmd.arg(target).arg(format!("{k}={v}"));
        }
        cmd.args(["-f", container]).arg(&dst);
        if let Err(e) = run(&mut cmd, "ffmpeg export") {
            let _ = fs::remove_file(&dst);
            return Err(e);
        }
        Ok(Exported { path: dst, temporary: true })
    }

    fn delete(&self, file: &str) -> Result<()> {
        match fs::remove_file(song_path(&self.data_dir, file)?) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e).with_context(|| format!("deleting {file}")),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::domain::testing::song;

    use super::*;

    /// A 440 Hz mono 44.1 kHz WAV: a different shape from what is stored, so tests prove the conversion.
    fn tone(path: &Path, secs: u32) {
        let mut cmd = Command::new("ffmpeg");
        cmd.args(["-hide_banner", "-v", "error", "-y", "-f", "lavfi", "-i"])
            .arg(format!("sine=frequency=440:duration={secs}"))
            .args(["-ac", "1", "-ar", "44100"])
            .arg(path);
        run(&mut cmd, "ffmpeg tone").unwrap();
    }

    fn probe(path: &Path, entries: &str) -> String {
        let out = run(
            Command::new("ffprobe").args(["-v", "error", "-select_streams", "a:0", "-show_entries", entries, "-of", "csv=p=0"]).arg(path),
            "ffprobe",
        )
        .unwrap();
        String::from_utf8_lossy(&out).trim().to_string()
    }

    fn tags(path: &Path) -> HashMap<String, String> {
        let out =
            run(Command::new("ffprobe").args(["-v", "error", "-show_entries", "format_tags:stream_tags", "-of", "json"]).arg(path), "ffprobe tags")
                .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
        let mut found = HashMap::new();
        let streams = v["streams"].as_array().cloned().unwrap_or_default();
        for t in streams.iter().map(|s| &s["tags"]).chain([&v["format"]["tags"]]) {
            for (k, val) in t.as_object().into_iter().flatten() {
                found.insert(k.to_lowercase(), val.as_str().unwrap_or_default().to_string());
            }
        }
        found
    }

    #[test]
    fn import_writes_each_storage_format_at_48_khz_stereo() {
        let dir = tempfile::tempdir().unwrap();
        let wav = dir.path().join("in.wav");
        tone(&wav, 4);
        for (format, codec) in [(StoreFormat::Opus, "opus"), (StoreFormat::Flac, "flac"), (StoreFormat::Mp3, "mp3"), (StoreFormat::Aac, "aac")] {
            let files = Ffmpeg::new(dir.path().join(codec), format, 160).unwrap();
            let imported = files.import(&wav, "s1").unwrap();
            assert_eq!(imported.file, format!("songs/s1.{}", format.extension()));
            let path = dir.path().join(codec).join(&imported.file);
            assert!((imported.seconds - 4.0).abs() < 0.1, "{codec}: {}", imported.seconds);
            assert_eq!(imported.bytes, std::fs::metadata(&path).unwrap().len());
            assert_eq!(probe(&path, "stream=codec_name"), codec);
            assert_eq!(probe(&path, "stream=channels"), "2");
            if format != StoreFormat::Mp3 {
                assert_eq!(probe(&path, "stream=sample_rate"), "48000", "{codec}");
            }
        }
    }

    #[test]
    fn tags_are_rewritten_in_every_format_without_re_encoding() {
        let dir = tempfile::tempdir().unwrap();
        let wav = dir.path().join("in.wav");
        tone(&wav, 2);
        for format in [StoreFormat::Opus, StoreFormat::Flac, StoreFormat::Mp3, StoreFormat::Aac] {
            let files = Ffmpeg::new(dir.path().join(format.extension()), format, 160).unwrap();
            let imported = files.import(&wav, "s1").unwrap();
            let path = dir.path().join(format.extension()).join(&imported.file);
            let before = probe(&path, "stream=codec_name");
            let mut s = Song { file: imported.file.clone(), lyrics: Some("[chorus]\nnight bus".into()), genre: Some("lo-fi".into()), ..song("s1") };
            s.title = "Night Bus".into();
            files.retag(&s).unwrap();
            let got = tags(&path);
            assert_eq!(got.get("title").map(String::as_str), Some("Night Bus"), "{format:?}: {got:?}");
            assert_eq!(got.get("artist").map(String::as_str), Some("Maio"), "{format:?}");
            assert!(got.get("comment").is_some_and(|c| c.contains("engine ace-step, seed 7, licence MIT")), "{format:?}: {got:?}");
            if matches!(format, StoreFormat::Opus | StoreFormat::Flac) {
                assert!(got.get("lyrics").is_some_and(|l| l.contains("night bus")), "{format:?}: {got:?}");
            }
            assert_eq!(probe(&path, "stream=codec_name"), before);
            assert!((duration(&path).unwrap() - imported.seconds).abs() < 0.05, "{format:?}: same audio");
            s.title = "Renamed".into();
            files.retag(&s).unwrap();
            assert_eq!(tags(&path).get("title").map(String::as_str), Some("Renamed"));
        }
    }

    #[test]
    fn concurrent_tag_writes_never_mix_their_files() {
        let dir = tempfile::tempdir().unwrap();
        let wav = dir.path().join("in.wav");
        tone(&wav, 20);
        let files = std::sync::Arc::new(Ffmpeg::new(dir.path(), StoreFormat::Opus, 160).unwrap());
        let imported = files.import(&wav, "s1").unwrap();
        let writers: Vec<_> = (0..8)
            .map(|i| {
                let (files, file) = (files.clone(), imported.file.clone());
                std::thread::spawn(move || files.retag(&Song { title: format!("Title {i}"), file, ..song("s1") }))
            })
            .collect();
        for w in writers {
            w.join().unwrap().unwrap();
        }
        let path = dir.path().join(&imported.file);
        assert!((duration(&path).unwrap() - 20.0).abs() < 0.1, "the audio is intact");
        assert!(tags(&path).get("title").is_some_and(|t| t.starts_with("Title ")));
        let leftovers: Vec<_> = std::fs::read_dir(dir.path().join("songs")).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(leftovers.len(), 1, "no temp files left: {leftovers:?}");
    }

    #[test]
    fn delete_removes_the_file_and_a_missing_one_is_fine() {
        let dir = tempfile::tempdir().unwrap();
        let files = Ffmpeg::new(dir.path(), StoreFormat::Opus, 160).unwrap();
        std::fs::write(dir.path().join("songs/s1.opus"), b"x").unwrap();
        files.delete("songs/s1.opus").unwrap();
        assert!(!dir.path().join("songs/s1.opus").exists());
        files.delete("songs/s1.opus").unwrap();
        assert_eq!(files.scratch("j1.wav"), dir.path().join("tmp/j1.wav"));
        assert!(files.import(&dir.path().join("missing.wav"), "s2").is_err());
    }

    #[test]
    fn files_outside_songs_are_never_touched() {
        let dir = tempfile::tempdir().unwrap();
        let files = Ffmpeg::new(dir.path().join("data"), StoreFormat::Opus, 160).unwrap();
        std::fs::write(dir.path().join("keep.txt"), b"x").unwrap();
        for bad in ["../keep.txt", "songs/../../keep.txt", "/etc/hosts", "tmp/x.opus", "songs"] {
            assert!(files.delete(bad).is_err(), "{bad}");
            assert!(files.retag(&Song { file: bad.into(), ..song("s1") }).is_err(), "{bad}");
        }
        assert!(dir.path().join("keep.txt").exists());
    }

    fn stored(dir: &Path) -> (Ffmpeg, Song) {
        let f = Ffmpeg::new(dir, StoreFormat::Opus, 160).unwrap();
        let wav = dir.join("in.wav");
        tone(&wav, 2);
        let imported = f.import(&wav, "s1").unwrap();
        let song = Song { file: imported.file, title: "Soft Static".into(), ..song("s1") };
        (f, song)
    }

    #[test]
    fn exports_convert_with_tags_and_the_stored_format_is_sent_as_it_is() {
        let dir = tempfile::tempdir().unwrap();
        let (f, song) = stored(dir.path());
        for (format, codec) in
            [(DownloadFormat::Mp3, "mp3"), (DownloadFormat::Aac, "aac"), (DownloadFormat::Flac, "flac"), (DownloadFormat::Wav, "pcm_s16le")]
        {
            let e = f.export(&song, format).unwrap();
            assert!(e.temporary, "{format:?} is converted");
            assert_eq!(probe(&e.path, "stream=codec_name"), codec);
            assert_eq!(tags(&e.path).get("title").map(String::as_str), Some("Soft Static"), "{format:?} keeps the title");
            std::fs::remove_file(&e.path).unwrap();
        }
        let same = f.export(&song, DownloadFormat::Opus).unwrap();
        assert!(!same.temporary);
        assert_eq!(same.path, dir.path().join(&song.file));
    }

    #[test]
    fn two_exports_never_share_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let (f, song) = stored(dir.path());
        let (a, b) = (f.export(&song, DownloadFormat::Mp3).unwrap(), f.export(&song, DownloadFormat::Mp3).unwrap());
        assert_ne!(a.path, b.path);
    }

    #[test]
    fn a_missing_file_answers_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let f = Ffmpeg::new(dir.path(), StoreFormat::Opus, 160).unwrap();
        let gone = Song { file: "songs/gone.opus".into(), ..song("gone") };
        assert!(f.export(&gone, DownloadFormat::Mp3).is_err());
        assert!(f.export(&gone, DownloadFormat::Opus).is_err(), "the stored format too: no file, no download");
        assert!(std::fs::read_dir(dir.path().join("tmp")).map_or(true, |d| d.count() == 0), "no half-written file left");
    }
}
