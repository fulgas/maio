//! A song's details (SPEC 12): how it was made, its words, downloads, and "Make one like this".

use leptos::prelude::*;
use leptos_router::hooks::{use_location, use_navigate};
use thaw::{Button, ButtonAppearance, Dialog, DialogActions, DialogBody, DialogContent, DialogSurface, DialogTitle};

use crate::{
    domain::{DownloadFormat, Song},
    ui::{
        api, format,
        make::{Draft, MakeForm},
    },
};

/// What the panel lists, in order; unknown fields are left out.
pub fn facts(s: &Song) -> Vec<(&'static str, String)> {
    let mut f: Vec<(&'static str, Option<String>)> = vec![
        ("Described as", Some(s.prompt.clone()).filter(|p| !p.is_empty())),
        ("About", s.description.clone()),
        ("Mood", s.mood.clone()),
        ("Genre", s.genre.clone()),
        ("Energy", s.energy.clone()),
        ("Length", Some(format::clock(s.seconds))),
        ("Tempo", s.bpm.map(|b| format!("{b} bpm"))),
        ("Key", s.key.clone()),
        ("Time signature", s.time_signature.clone()),
        (
            "Voice",
            Some(if s.vocals {
                format!("Sung in {}", s.vocal_language.clone().unwrap_or_else(|| "the default language".into()))
            } else {
                "Instrumental".into()
            }),
        ),
        ("Seed", Some(format!("{}: with the same settings, the same song again", s.seed))),
        ("Engine", Some(s.engine.clone())),
        ("Lyrics by", s.lyrics_engine.clone()),
        ("Named by", s.title_engine.clone()),
        ("Licence", Some(if s.licence.note.is_empty() { s.licence.name.clone() } else { format!("{}: {}", s.licence.name, s.licence.note) })),
        ("Made", Some(s.created_at.chars().take(10).collect())),
    ];
    f.retain(|(_, v)| v.is_some());
    f.into_iter().map(|(k, v)| (k, v.unwrap_or_default())).collect()
}

#[component]
pub fn SongDetails(open: RwSignal<bool>, song: RwSignal<Option<Song>>) -> impl IntoView {
    let draft = expect_context::<Draft>();
    let navigate = use_navigate();
    let path = use_location().pathname;
    let like_this = move || {
        let Some(s) = song.get_untracked() else { return };
        draft.0.set(Some(MakeForm::from_song(&s)));
        open.set(false);
        if path.get_untracked() != "/" {
            navigate("/", Default::default());
        }
    };
    view! {
        <Dialog open>
            <DialogSurface><DialogBody>
                <DialogTitle>{move || song.get().map(|s| s.title).unwrap_or_default()}</DialogTitle>
                <DialogContent>
                    {move || song.get().map(|s| view! {
                        <dl class="facts">
                            {facts(&s).into_iter().map(|(k, v)| view! { <dt>{k}</dt><dd>{v}</dd> }).collect_view()}
                        </dl>
                        {s.lyrics.clone().map(|l| view! { <h3 class="lyrics-head">"Lyrics"</h3><pre class="lyrics-text">{l}</pre> })}
                        // A visible label, not captions: the dialog would cut a caption off.
                        <div class="downloads" role="group" aria-labelledby="downloads-label">
                            <span id="downloads-label" class="downloads-label">"Download"</span>
                            {DownloadFormat::ALL.into_iter().map(|f| view! {
                                <a class="chip" href=api::download_url(&s.id, f) download aria-label=format!("Download as {}", f.label())>{f.label()}</a>
                            }).collect_view()}
                        </div>
                    })}
                </DialogContent>
                <DialogActions>
                    <Button on_click=move |_| open.set(false)>"Close"</Button>
                    <Button appearance=ButtonAppearance::Primary on_click=move |_| like_this()>"Make one like this"</Button>
                </DialogActions>
            </DialogBody></DialogSurface>
        </Dialog>
    }
}

#[cfg(test)]
mod tests {
    use super::facts;
    use crate::domain::testing::song;

    #[test]
    fn facts_say_how_a_song_was_made_and_skip_what_is_unknown() {
        let s = crate::domain::Song {
            prompt: "warm pads".into(),
            mood: Some("calm".into()),
            genre: None,
            bpm: Some(93),
            key: Some("A minor".into()),
            seed: 1234,
            vocals: true,
            vocal_language: Some("pt".into()),
            ..song("s1")
        };
        let f = facts(&s);
        let get = |k: &str| f.iter().find(|(n, _)| *n == k).map(|(_, v)| v.as_str());
        assert_eq!(get("Described as"), Some("warm pads"));
        assert_eq!(get("Mood"), Some("calm"));
        assert_eq!(get("Genre"), None, "unknown fields are left out");
        assert_eq!(get("Tempo"), Some("93 bpm"));
        assert_eq!(get("Key"), Some("A minor"));
        assert_eq!(get("Voice"), Some("Sung in pt"));
        assert_eq!(get("Seed"), Some("1234: with the same settings, the same song again"));
        assert!(get("Engine").is_some());
    }
}
