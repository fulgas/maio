use std::path::Path;

use anyhow::{Context, bail};
use tokio::process::Command;

use crate::config::{StorageConfig, StoreFormat};

async fn run(cmd: &mut Command, what: &str) -> anyhow::Result<Vec<u8>> {
    let out = cmd.output().await.with_context(|| format!("running {what}"))?;
    if !out.status.success() {
        bail!("{what} failed: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(out.stdout)
}

/// Transcodes any audio file into a loudness-normalised 48 kHz stereo file in the storage format;
/// returns its length in seconds. The container is set explicitly, so the file name never picks it.
pub async fn import(src: &Path, dst: &Path, storage: &StorageConfig) -> anyhow::Result<f64> {
    let bitrate = format!("{}k", storage.bitrate_k);
    let encode: Vec<&str> = match storage.format {
        StoreFormat::Opus => vec!["-c:a", "libopus", "-b:a", &bitrate, "-f", "ogg"],
        StoreFormat::Flac => vec!["-c:a", "flac", "-f", "flac"],
        StoreFormat::Mp3 => vec!["-c:a", "libmp3lame", "-b:a", &bitrate, "-f", "mp3"],
        StoreFormat::Aac => vec!["-c:a", "aac", "-b:a", &bitrate, "-f", "mp4"],
    };
    run(
        Command::new("ffmpeg")
            .args(["-hide_banner", "-v", "error", "-y", "-i"])
            .arg(src)
            // loudnorm emits huge frames; asetnsamples keeps them under FLAC's 65535-sample block limit
            .args(["-af", "loudnorm=I=-16:TP=-1.5:LRA=11,aresample=48000,asetnsamples=n=4096:p=0", "-ac", "2"])
            .args(&encode)
            .arg(dst),
        "ffmpeg import",
    )
    .await?;
    duration(dst).await
}

pub async fn duration(path: &Path) -> anyhow::Result<f64> {
    let out = run(
        Command::new("ffprobe").args(["-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0"]).arg(path),
        "ffprobe",
    )
    .await?;
    String::from_utf8_lossy(&out).trim().parse::<f64>().context("ffprobe duration")
}

/// What goes into a file's tags (SPEC 10, Tags in the files).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Tags {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: Option<String>,
    pub bpm: Option<u32>,
    pub lyrics: Option<String>,
    pub comment: String,
}

impl From<&crate::store::Track> for Tags {
    fn from(t: &crate::store::Track) -> Self {
        Self {
            title: t.title.clone(),
            artist: t.artist.clone(),
            album: t.album.clone(),
            genre: t.genre.clone(),
            bpm: t.bpm,
            lyrics: t.lyrics.clone(),
            comment: format!("engine {}, seed {}, licence {}", t.engine, t.seed, t.licence),
        }
    }
}

/// Rewrites a file's tags in place, copying the audio as it is (no re-encode).
/// Ogg keeps Vorbis comments on the stream; the other containers keep them on the file.
pub async fn write_tags(path: &Path, tags: &Tags) -> anyhow::Result<()> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or_default();
    let format = match ext {
        "opus" | "ogg" => "ogg",
        "flac" => "flac",
        "mp3" => "mp3",
        "m4a" | "mp4" => "mp4",
        other => bail!("cannot tag a .{other} file"),
    };
    // A temp name of its own, so two writes at once never share (and mix) one file.
    let tmp = path.with_extension(format!("tagging-{}.{ext}", crate::jobs::new_id("t")));
    let target = if format == "ogg" { "-metadata:s:a:0" } else { "-metadata" };
    let mut pairs = vec![("title", tags.title.clone()), ("artist", tags.artist.clone()), ("album", tags.album.clone()), ("comment", tags.comment.clone())];
    pairs.extend(tags.genre.clone().map(|g| ("genre", g)));
    pairs.extend(tags.bpm.map(|b| ("bpm", b.to_string())));
    pairs.extend(tags.lyrics.clone().map(|l| ("lyrics", l)));
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-hide_banner", "-v", "error", "-y", "-i"]).arg(path).args(["-map", "0", "-c", "copy", "-map_metadata", "-1"]);
    for (k, v) in pairs {
        cmd.arg(target).arg(format!("{k}={v}"));
    }
    if format == "mp3" {
        cmd.args(["-id3v2_version", "3"]);
    }
    cmd.args(["-f", format]).arg(&tmp);
    if let Err(e) = run(&mut cmd, "ffmpeg tags").await {
        let _ = tokio::fs::remove_file(&tmp).await;
        return Err(e);
    }
    tokio::fs::rename(&tmp, path).await.context("replacing the tagged file")?;
    Ok(())
}

/// A file's tags, from the container and its first audio stream, keys lowercased.
pub async fn read_tags(path: &Path) -> anyhow::Result<std::collections::HashMap<String, String>> {
    let out = run(
        Command::new("ffprobe").args(["-v", "error", "-show_entries", "format_tags:stream_tags", "-of", "json"]).arg(path),
        "ffprobe tags",
    )
    .await?;
    let v: serde_json::Value = serde_json::from_slice(&out).context("ffprobe json")?;
    let mut tags = std::collections::HashMap::new();
    let streams = v["streams"].as_array().cloned().unwrap_or_default();
    for t in streams.iter().map(|s| &s["tags"]).chain([&v["format"]["tags"]]) {
        if let Some(map) = t.as_object() {
            for (k, val) in map {
                if let Some(s) = val.as_str() {
                    tags.insert(k.to_lowercase(), s.to_string());
                }
            }
        }
    }
    Ok(tags)
}

/// Decodes a file into interleaved 48 kHz stereo s16 samples.
/// ponytail: whole track in memory (~35 MB per 3 minutes); stream it if tracks get much longer.
pub async fn decode(path: &Path) -> anyhow::Result<Vec<i16>> {
    let out = run(
        Command::new("ffmpeg")
            .args(["-hide_banner", "-v", "error", "-i"])
            .arg(path)
            .args(["-f", "s16le", "-ar", "48000", "-ac", "2", "pipe:1"]),
        "ffmpeg decode",
    )
    .await?;
    Ok(out.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect())
}

/// Writes a 440 Hz mono 44.1 kHz WAV: a different format from the bus on purpose, so tests prove conversion.
pub async fn test_tone(path: &Path, secs: u32) -> anyhow::Result<()> {
    run(
        Command::new("ffmpeg")
            .args(["-hide_banner", "-v", "error", "-y", "-f", "lavfi", "-i"])
            .arg(format!("sine=frequency=440:duration={secs}"))
            .args(["-ac", "1", "-ar", "44100"])
            .arg(path),
        "ffmpeg tone",
    )
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{StorageConfig, StoreFormat};

    async fn codec(path: &Path) -> String {
        let out = Command::new("ffprobe").args(["-v", "error", "-select_streams", "a:0", "-show_entries", "stream=codec_name", "-of", "csv=p=0"]).arg(path).output().await.unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    #[tokio::test]
    async fn import_writes_each_storage_format() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("in.wav");
        test_tone(&src, 4).await.unwrap();
        let mut sizes = Vec::new();
        for (format, want) in [(StoreFormat::Opus, "opus"), (StoreFormat::Flac, "flac"), (StoreFormat::Mp3, "mp3"), (StoreFormat::Aac, "aac")] {
            let storage = StorageConfig { format, ..Default::default() };
            let dst = dir.path().join(format!("out.{}", format.extension()));
            let secs = import(&src, &dst, &storage).await.unwrap();
            assert!((secs - 4.0).abs() < 0.1, "{want}: {secs}");
            assert_eq!(codec(&dst).await, want);
            let samples = decode(&dst).await.unwrap();
            assert!(samples.iter().any(|&s| s.abs() > 1000), "{want} decodes to sound");
            sizes.push((want, std::fs::metadata(&dst).unwrap().len()));
        }
        let size = |n: &str| sizes.iter().find(|(w, _)| *w == n).unwrap().1;
        assert!(size("opus") * 2 < size("flac"), "a pure test tone compresses unusually well in FLAC; music is ~8x: {sizes:?}");
    }

    #[tokio::test]
    async fn decode_of_missing_file_is_an_error() {
        assert!(decode(Path::new("/nonexistent/track.flac")).await.is_err());
    }

    #[tokio::test]
    async fn tags_are_written_into_every_format_without_re_encoding() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("in.wav");
        test_tone(&src, 2).await.unwrap();
        for format in [StoreFormat::Opus, StoreFormat::Flac, StoreFormat::Mp3, StoreFormat::Aac] {
            let storage = StorageConfig { format, ..Default::default() };
            let dst = dir.path().join(format!("t.{}", format.extension()));
            import(&src, &dst, &storage).await.unwrap();
            let before = (codec(&dst).await, duration(&dst).await.unwrap());
            let tags = Tags { title: "Night Bus".into(), artist: "Late night".into(), album: "Late night".into(),
                genre: Some("lo-fi".into()), bpm: Some(75), lyrics: Some("[chorus]\nnight bus".into()), comment: "engine fake, seed 1, licence MIT".into() };
            write_tags(&dst, &tags).await.unwrap();
            let got = read_tags(&dst).await.unwrap();
            assert_eq!(got.get("title").map(String::as_str), Some("Night Bus"), "{format:?}: {got:?}");
            assert_eq!(got.get("artist").map(String::as_str), Some("Late night"), "{format:?}");
            assert_eq!(got.get("album").map(String::as_str), Some("Late night"), "{format:?}");
            if matches!(format, StoreFormat::Opus | StoreFormat::Flac) {
                assert!(got.get("lyrics").is_some_and(|l| l.contains("night bus")), "{format:?}: {got:?}");
            }
            let after = (codec(&dst).await, duration(&dst).await.unwrap());
            assert_eq!(before.0, after.0);
            assert!((before.1 - after.1).abs() < 0.05, "{format:?}: same audio");
            write_tags(&dst, &Tags { title: "Renamed".into(), ..tags }).await.unwrap();
            assert_eq!(read_tags(&dst).await.unwrap().get("title").map(String::as_str), Some("Renamed"));
        }
        assert!(write_tags(Path::new("/nonexistent/x.opus"), &Tags::default()).await.is_err());
    }

    #[tokio::test]
    async fn concurrent_tag_writes_never_mix_their_files() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("in.wav");
        test_tone(&src, 30).await.unwrap();
        let dst = dir.path().join("t.opus");
        import(&src, &dst, &StorageConfig::default()).await.unwrap();
        let writes = (0..16).map(|i| {
            let dst = dst.clone();
            tokio::spawn(async move { write_tags(&dst, &Tags { title: format!("Title {i}"), ..Default::default() }).await })
        });
        let results: Vec<_> = futures::future::join_all(writes).await.into_iter().map(|r| r.unwrap()).collect();
        assert!(results.iter().all(Result::is_ok), "every write succeeds: {results:?}");
        assert!((duration(&dst).await.unwrap() - 30.0).abs() < 0.1, "the audio is intact");
        assert!(read_tags(&dst).await.unwrap().get("title").is_some_and(|t| t.starts_with("Title ")));
        assert!(std::fs::read_dir(dir.path()).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().contains("tagging")), "no temp files left");
    }
}
