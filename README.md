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

`plugin.toml` is written and has been run against lemonfiber's **published
schema**. Its shape is accepted. The finding this repository exists for
([spec#458](https://github.com/lemonfiber/spec/issues/458)) was three rules
meeting; **two are answered** and one is not:

- ~~a probe's request is `{method, path}` with no `headers`~~ — a request now
  names the one representation it asks for, as `accept`, which is what Plex
  needs: it answers XML at every path, including the one its own health probe
  uses, unless asked otherwise;
- ~~the expectation vocabulary is flat~~ — a key is now a *place*, written as a
  JSON Pointer with a step that picks one entry of an array by a field it holds.
  The setting this plugin most wants to check is one of 151, found by its `id`,
  and it is now readable;
- **`media.serve`'s `catalogue` probe still cannot be written.** It has to
  present a credential, and there is no field for one. That is a decision rather
  than an omission: `accept` is one media type and not a header map, so *asked
  as nobody* stays a property of the form — any service may name its credential
  header whatever it likes, so no list of refused names could ever be closed.

Every fixture in both published plugins is flat at the top level and neither
service needs content negotiation, which is why none of this had bitten before.
A catalogue of plugins chosen for being easy to write strains nothing — which is
the argument for this repository in one sentence, and the two answered rules
above are it paying out.

There are also no recordings here. A fixture names the image digest it was taken
against, nobody has run this image, and inventing one would put the single kind
of wrong this apparatus exists to catch inside the apparatus. So `just manifest`
is red twice over, and both are the right reason.

`schema_version = 1` permitted exactly one service; **it now permits more than
one** (`ARCH-R126`), and wiring, proofs and contributed checks name which service
they are about (`ARCH-R127`, `ARCH-R128`). The second service this plugin wants
is argued and costed in SPEC.md and is now a decision rather than a blocker.

Six open questions are at the end of SPEC.md. **The first is answered**: it asked
how a probe should ask for JSON and how an expectation should reach into a nested
body, it was never specific to Plex, and it was worth answering whether or not
this plugin is ever built. Its successor is narrower and still open — how a probe
presents a credential without a `guarded` probe quietly gaining the ability to.
