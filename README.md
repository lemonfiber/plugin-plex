# plugin-plex

**A specification, not yet a plugin.** Read [SPEC.md](SPEC.md).

Plex is the plugin lemonfiber's extensibility features were written for — `F3`
calls it "the hard case this must survive", `F8` calls substituting it "the case
the whole design exists for" — and the two published plugins, `plugin-komga` and
`plugin-uptime-kuma`, each install a container nothing in the stack talks to.
This repository exists to be the worked example instead: a manifest that claims
core capabilities something asks for, stands in for a bundled service, holds a
credential, changes a bundled setting, and runs a first-run flow.

## What state it is in

`plugin.toml` is written and has been run against the contract. With recordings
in place it conforms on every rule the stand-in can check — the recipe's flow
analysis, the secret, the override, all four contributed doctor checks, and both
claims against lemonfiber's published capability vocabulary.

There are no recordings. A fixture names the image digest it was taken against,
and nobody has run this image yet, so `just manifest` fails once per missing one
and that is the correct state for it to be in. See SPEC.md § *The recordings are
a finding of their own*.

Two findings came out of writing it. One is fixed
(`lemonfiber/plugin-template#8` — a namespaced capability was checked against
the service's id rather than the plugin's). One is open: `schema_version = 1`
permits exactly one service, and this plugin wants two.

Five open questions are listed at the end of SPEC.md. The first — whether a
plugin may wrap a proprietary service at all — decides whether this document is
about Plex or about Emby.
