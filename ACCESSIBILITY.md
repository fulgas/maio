# Accessibility

Maio is for anyone who wants to make songs, keep them and put them on air, and that includes people who use a keyboard instead of a mouse, a screen reader, a phone, zoom, or a light or dark screen. This page says what Maio aims for, what contributors do to keep it that way, and how to tell us when something gets in your way.

## What Maio aims for

- **Target:** we work toward [WCAG 2.1 Level AA](https://www.w3.org/TR/WCAG21/) on every screen of the web app, in both themes. This is the goal that guides the work, not a claim that an outside evaluation has confirmed it.
- **Keyboard:** everything the web app does works from the keyboard alone, in a sensible order, with a focus ring you can see. Dialogs are the browser's own `<dialog>`, so focus moves into them and back out.
- **Screen readers:** every control has a name that says what it does. Icon buttons have a caption, shown on hover and on keyboard focus, that is also their name. Toasts and errors are announced.
- **Seeing and touching:** text contrast of at least 4.5:1, targets of at least 24 px, and 44 px for icon buttons. The web app works from 360 px wide.
- **Not by colour alone:** a status shown by a coloured dot, such as the engine's, is also said in its caption and its name.

## When you contribute

The rules are in [Project rules](docs/project-rules.md#accessibility). In short:

- **A change to the web app gets a check in `e2e/smoke.mjs`.** `just e2e` drives a real browser through every screen and checks keyboard focus, target sizes, accessible names and narrow screens. It runs on every pull request.
- **Do a keyboard pass** on anything interactive you add or change: Tab and Shift+Tab reach every control in order, Enter and Space work, Escape leaves dialogs and menus, and focus is always visible.
- **Name every control.** Icon buttons go through the `IconButton` component, which gives them a caption and a name in one.
- **New colours get a token in [DESIGN.md](DESIGN.md)** with its contrast checked; a test fails a colour that has no token.
- **In the documentation:** headings in order, real lists and tables, link text that makes sense on its own, and alternative text on every picture.
- **Before a release** the web app gets an accessibility review: axe-core on every screen and dialog in both themes, a keyboard pass, and 200 % zoom ([arc42 10](docs/architecture/10-quality-requirements.md)).

If a pull request leaves a barrier in place, say which one and why in its description.

## Telling us about a barrier

If something in Maio gets in your way, please [open an accessibility issue](https://github.com/fulgas/maio/issues/new?template=accessibility.yml). A report like this is expertise about how Maio works for real people, and it is welcome.

It helps to include, if you can:

- what you were trying to do, and what happened instead;
- the screen (Songs, a playlist, the player, sign-in);
- your browser and operating system, and any assistive technology, with versions;
- a screenshot or a recording, only if you are comfortable sharing one.

You never need to tell us about a disability or anything personal.

### How serious it is

You do not need to choose; we set it when we read the report.

- **Critical:** you cannot do something Maio is for, such as making, playing or sharing a song.
- **Serious:** you can, but only with a lot of difficulty or a workaround.
- **Moderate:** it is annoying or inconsistent.
- **Minor:** it barely gets in the way.

### What happens next

Maio is a one-person project, so there are no promised dates. Reports are read as soon as possible and fixed in the order above: a critical barrier comes before any new feature. We say what we found, offer a workaround where there is one, keep the issue updated, and may ask you to check the fix before it is closed.

## Who looks after this

The maintainer, [@fulgas](https://github.com/fulgas), reads accessibility reports, keeps track of known barriers, and keeps this page and the rules up to date.

## Where Maio is checked

- **Browsers:** current versions of Chrome, Edge, Firefox and Safari, on desktop and phone. The automated checks run in Chromium only.
- **Assistive technology:** screen readers are tried by hand, not on every change.

## Known limitations

- On phones, the engine and MediaMTX icons are hidden from the header, so their status cannot be checked there.
- Songs are music: they have no captions. Lyrics, when a song is sung, are in its details.
- Screen readers are not tested on every change, so a regression there can reach a release.

Found another? Please [open an accessibility issue](https://github.com/fulgas/maio/issues/new?template=accessibility.yml).

## Improving this page

Suggestions for this page or for how Maio handles accessibility are welcome as an issue or a pull request.
