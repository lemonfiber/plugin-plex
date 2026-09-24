# Working on plugin-plex

For the people who maintain this plugin. What it is for the people who run a
stack is in the [README](../README.md); the full design is in
[SPEC.md](../SPEC.md).

## Why this repository exists

Plex is the plugin lemonfiber's extensibility features were written for. `F3`
calls it "the hard case this must survive", and `F8` calls substituting it "the
case the whole design exists for". `plugin-komga` and `plugin-uptime-kuma` each
install a container nothing in the stack talks to. This repository is the worked
example of the rest: a manifest that claims core capabilities something asks
for, stands in for a bundled service, holds a credential, changes a bundled
setting, and runs a first-run flow.

## Where the manifest stands

`plugin.toml` is written and is run against lemonfiber's **published schema** on
every CI run. It is refused, for these reasons:

- **`media.serve`'s `catalogue` probe cannot be written.** It has to present a
  credential, and a probe's request has no field for one. The manifest carries a
  `headers` table on that probe, which the schema refuses. A request may name the
  one representation it asks for, as `accept` (`ARCH-R123`); `accept` is one
  media type and not a header map, so *asked as nobody* stays a property of the
  form, since any service may name its credential header whatever it likes and no
  list of refused names could be closed. How a probe presents a credential
  without a `guarded` probe gaining the ability to is open question 0 in
  SPEC.md.
- **Recordings are missing.** `fixtures/` holds four recordings off the pinned
  image. The manifest also names `fixtures/library-sections-operator.json` and
  `fixtures/identity-claimed.json`, which are not here: they need a claimed
  server, and a fixture names the image digest it was taken against, so neither
  can be written without running one.
- **It asks for `recipe.run`**, which lemonfiber does not offer, so an install is
  refused naming that capability.

Two rules this plugin needed are in the contract:

- a probe's request may name the one representation it asks for, as `accept`,
  which Plex needs because it answers XML at every path, including the one its
  own health probe uses, unless asked otherwise (`ARCH-R123`);
- an expectation's key is a *place*, written as a JSON Pointer with a step that
  picks one entry of an array by a field it holds (`ARCH-R125`). The setting this
  plugin most wants to check is one of 151, found by its `id`.

A manifest may declare more than one service (`ARCH-R126`), and wiring, proofs
and contributed checks name which service they are about (`ARCH-R127`,
`ARCH-R128`). The second service this plugin wants is argued and costed in
SPEC.md.

The open questions are at the end of SPEC.md.

## Checks

```sh
just ci        # every gate CI runs over this repository, in CI's order
```

`just manifest` is red, for the reasons above. `just` lists the recipes `ci` is
made of, and the CI jobs it does not run are named in the `justfile` beside the
recipe. Rules for working in this repository are in [AGENTS.md](../AGENTS.md).

`just ci` turns this clone's git hooks on as its first step, and
`.githooks/commit-msg` then refuses a commit CI would refuse: a non-conventional
subject, a missing sign-off, a missing `Spec:` citation, or a trailer crediting
an assistant. The rules are in
[50-governance/contributing.md](https://github.com/lemonfiber/spec/blob/main/50-governance/contributing.md).
