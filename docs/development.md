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

Every CI run asks `lemonfiber plugin claims` of the release `targets.toml` names,
and it refuses nothing about the manifest. It would not install it: the plugin
asks for `service.add`, `service.health.http` and `recipe.run`, and that release
offers a plugin `doctor.contribute` alone.

- **Every recording is made by running the image.** `.github/record.py` starts
  the image `plugin.toml` pins, never claimed, and writes every file in
  `fixtures/`, each asked from where its `note` says. `just record` runs it;
  `just recordings` and the `recordings` CI job run it again and compare with
  what is committed. Both need Docker.
- **`plex:claimed` fails against its recording.** The server is never claimed,
  because claiming needs a plex.tv account and none is held for this
  repository, so the recording holds `claimed: false`, which is the answer the
  check exists to catch. `just proofs` reports it failed, and every other
  proof, probe and check passes.
- **`media.serve`'s `catalogue` probe names no credential.** Its request is
  `accept` alone; the vocabulary says it is asked with the operator's
  credential, and how the runner presents a Plex token is open question 0 in
  SPEC.md.

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

`just proofs` is red, for the reason above. `just` lists the recipes `ci` is
made of, and the CI jobs it does not run are named in the `justfile` beside the
recipe. Rules for working in this repository are in [AGENTS.md](../AGENTS.md).

`just ci` turns this clone's git hooks on as its first step, and
`.githooks/commit-msg` then refuses a commit CI would refuse: a non-conventional
subject, a missing sign-off, a missing `Spec:` citation, or a trailer crediting
an assistant. The rules are in
[50-governance/contributing.md](https://github.com/lemonfiber/spec/blob/main/50-governance/contributing.md).
