# AGENTS.md — plugin-template

Guidance for any AI agent working in this repo.

> **Common rules for every lemonfiber repo are canonical in the spec:**
> [50-governance/ai-contributors.md](https://github.com/lemonfiber/spec/blob/main/50-governance/ai-contributors.md).
> Read them. This file is the `plugin-template`-specific header only.

## What this repo is

The plugin an author copies, and the **canonical home of the interim CI
harness**. `plugin-komga` and `plugin-uptime-kuma` carry byte-identical copies of
`.github/interim/` and each has a job that fails when its copy drifts from this
one. Change the harness here, then copy it to both; changing it there fails.

The plugin itself describes Kavita, and it describes something real on purpose:
`F10-R7` asks for a template that validates and proves *unmodified*, and a
template whose proofs are invented teaches an author to invent proofs.

## The rules you cannot break

- **Nothing here executes.** A plugin is declarative data (`F3-R1`), and
  contributed code is never run, under any opt-in (`F3-R6`). The Python under
  `.github/interim/` is CI harness, is not part of what an operator installs,
  and is deleted when lemonfiber's own verbs replace it.
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
  `validate.py` must never carry a list of tables, fields, kinds, closed sets or
  capability names — `F10-R2` forbids a second, hand-maintained description of
  the format, and the one that used to live here had drifted from the parser in
  seven places by the time it was removed. What a manifest may contain is the
  generated schema's to say; rules that need it, or the vocabulary, or the
  points, are skipped and **named as skipped**, and `published_gate.py` fetches
  all three and decides them.
- **A rule here must be one lemonfiber holds a manifest to.** Being weaker than
  the binary is what a stand-in is. Refusing something lemonfiber accepts is the
  defect, because an author then changes a manifest for no reason and the change
  is invisible to everybody else. Check the reader before adding a rule.

## Checks

```
just ci
```

Every gate CI runs over the contents of this repository, in CI's order. The jobs
it leaves out are named in the `justfile` beside the recipe, with what covers
each. `just` lists the recipes it is made of.

`proofs.json` is generated and committed; CI fails when the committed one is not
what the run would write. `prove.py` writes it only when given
`--report proofs.json`, which is why the recipe passes the flag.

## Before you open a PR

`just ci` turns this clone's git hooks on as its first step, and
`.githooks/commit-msg` then refuses a commit that CI would refuse — a
non-conventional subject, a missing sign-off, a missing `Spec:` citation, or a
trailer crediting an assistant. All four rules are in
[50-governance/contributing.md](https://github.com/lemonfiber/spec/blob/main/50-governance/contributing.md).
