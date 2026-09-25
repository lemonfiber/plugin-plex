# AGENTS.md — plugin-plex

Guidance for any AI agent working in this repo.

> **Common rules for every lemonfiber repo are canonical in the spec:**
> [50-governance/ai-contributors.md](https://github.com/lemonfiber/spec/blob/main/50-governance/ai-contributors.md).
> Read them. This file is the `plugin-plex`-specific header only.

## What this repo is

Plex as a lemonfiber plugin, and **the repository that demonstrates a gap in the
plugin contract rather than working around it**. Read [SPEC.md](SPEC.md) before
changing `plugin.toml`; the headline section is not background.

The short version, as it stands: a probe asks for JSON (`accept`), an
expectation reaches into a `MediaContainer`, and the setting that matters — one
of 151, in a list whose order nobody promised — is reachable by the `id` it
carries rather than by an index. `media.serve`'s `catalogue` probe names no
credential: the vocabulary says it is asked with the operator's, and how the
runner presents a Plex token is open question 0 in `SPEC.md`.

Every file in `fixtures/` is written by `.github/record.py` off the pinned image,
never claimed, because claiming needs a plex.tv account and none is held for this
repository. **Never write or edit a recording by hand**; run `just record`, and
`just recordings` (the `recordings` CI job) checks the committed ones against a
fresh run.

`.github/reader/` is a **copy** of the harness whose canonical home is
`plugin-template`. The `harness` job compares the two byte for byte, so it is
changed there and copied here, never the other way round.

## The rules you cannot break

- **Nothing here executes.** A plugin is declarative data (`F3-R1`), and
  contributed code is never run, under any opt-in (`F3-R6`). The Python under
  `.github/reader/` is CI harness and is not part of what an operator installs:
  it fetches the lemonfiber release `targets.toml` names and asks it.
- **The plugin is `plugin.toml` and `fixtures/`.** Proofs, claims and
  contributions live in the manifest, not beside it: an installer reads one file,
  and something the installer never reads cannot be what `F3-R4` refuses an
  install over.
- **The image is named by digest** (`F3-R8`). Moving the pin means re-recording
  every fixture against the new image in the same change.
- **A proof asserts a body, never only a status.** Docker's port proxy accepts
  before anything inside is listening. The one exception is a refusal: a `401`
  is not something a port proxy can produce.
- **A proof that could not be run is unproven** (`F3-R5`), never a pass. The
  three verdicts stay three.
- **No field beyond the contract's set.** A manifest carrying one is refused by
  name rather than ignored (`ARCH-R84`).
- **The format is lemonfiber's to describe, and nothing here describes it.**
  `reader.py` carries no list of tables, fields, kinds, closed sets or capability
  names, and decides no verdict: `F10-R2` forbids a second description of the
  format, and every verdict CI reports is `lemonfiber plugin claims`'s own.

## Checks

```
just ci
```

Every gate CI runs over the contents of this repository, in CI's order. The jobs
it leaves out are named in the `justfile` beside the recipe, with what covers
each. `just` lists the recipes it is made of.

`proofs.json` is generated and committed; CI fails when the committed one is not
what the run would write. `reader.py proofs` writes it on every run.

### `proofs` is red here, on one check

`proofs` fails on `plex:claimed`. Its only recording is of a server nobody has
claimed, which holds `claimed: false` — the answer the check exists to catch —
and the format has no way to state that a check is expected to fail on a
recording. Every other probe, proof and check passes. `SPEC.md` § *The finding*
has what was measured.

`manifest`, `proofs`, `reader`, `recordings` and `harness` are not among
`main`'s required contexts.

Do not make `proofs` pass by pointing `plex:claimed` at something else or by
trimming it. That is the one change this repository cannot accept.

## Before you open a PR

`just ci` turns this clone's git hooks on as its first step, and
`.githooks/commit-msg` then refuses a commit that CI would refuse — a
non-conventional subject, a missing sign-off, a missing `Spec:` citation, or a
trailer crediting an assistant. All four rules are in
[50-governance/contributing.md](https://github.com/lemonfiber/spec/blob/main/50-governance/contributing.md).
