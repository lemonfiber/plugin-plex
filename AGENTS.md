# AGENTS.md — plugin-plex

> **Start at the roadmap and board on [lemonfiber.app](https://lemonfiber.app),
> rendered from the report of where every unreleased version stands. Then the
> rules** every repository shares:
> [working in the repositories](https://github.com/lemonfiber/spec/blob/main/50-governance/working-in-the-repositories.md)
> and [the rules for agents](https://github.com/lemonfiber/spec/blob/main/50-governance/ai-contributors.md).
> This file holds only what is true of this repository.

## What this repo is

Plex Media Server as a first-party lemonfiber plugin: `plugin.toml` and the
adapter that speaks the `owner.claim@1` and `media.serve@1` contracts in front of
Plex. The adapter is the Rust crate at the root, built on `lemonfiber-adapter-kit`
from the core at the commit `Cargo.toml` pins, into the image the `Dockerfile`
builds. `release.yml` publishes that image from every commit on `main` and from
the train's tag, through spec's reusable `adapter-release`. `plugin.toml` runs Plex
alone and does not run the adapter yet; its recordings are under `fixtures/`.

## The rules you cannot break

- **The plugin id is the service's id** (`ARCH-R266`).
- **No Python here.** The reader is spec's `first-party-plugin` workflow's.
- **No `unwrap`, `expect`, `panic` or indexing** outside tests. The lints in
  `Cargo.toml` are the core's.
- **The adapter never keeps Plex's token.** Plex writes the token of the account
  that claimed it into its `Preferences.xml`, and the adapter reads it from the
  configuration directory it shares with Plex for every call, through the kit's
  `Shared`, which follows no link (`ARCH-R339`, `ARCH-R364`). It never logs it.
- **A claim changes nothing Plex already holds** (`ARCH-R328`). Only a Plex whose
  `/identity` says it is unclaimed is claimed. The code the operator entered is
  exchanged at plex.tv under the identifier Plex presents there, the token that
  answers is written into `Preferences.xml`, and `claim` answers that it is in
  force once Plex restarts, because Plex reads its preferences only at start. A
  Plex claimed by an account whose token it does not keep is answered as theirs.
- **Nothing is asked for a member by name.** A member's own token on Plex is
  handed out by plex.tv, so every `media.serve` operation naming a member answers
  `unsupported`, as does everything else Plex cannot do (`ARCH-R334`).
- **A guard asks the adapter about a part or an item.** The declaration names
  each byte path's space: a part file's id is in `part`, and an item's own
  answer, pictures, transcodes and resized pictures are in `metadata`, the last
  two by the item their `path` or `url` names, matched whole. Bytes under
  stream, download, sync and live TV ids are refused. The guard's question is
  answered only by asking Plex with the member's own token, and grants only
  where Plex shows that member the item asked about, or an item holding the
  part asked about (`ARCH-R366`, `ARCH-R368`).
- **The internal TLS port is Plex's own port.** `serve_tls` sets
  `customCertificatePath` to the PKCS#12 the core wrote and an empty
  `customCertificateKey`, and answers that it is in force once Plex restarts
  until a handshake presents the certificate (`ARCH-R345`, `ARCH-R346`).
- **The core's crates move with the train.** The `rev` in `Cargo.toml` is the
  core commit `targets.toml` names, the same for every `lemonfiber-*` crate.
- **Moving Plex's digest** means recording every response again against the new
  image in the same change, moving `RELEASE` and `DIGEST` in `src/adapter.rs`
  with it, and committing the `proofs.json` the checks write.

## Checks

```
just ci
```

The gates that need nothing but this tree. The plugin verdict and the
recordings run in CI; `just recordings` takes the recordings again locally, with
Docker.
