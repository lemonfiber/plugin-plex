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
schema**. Its shape is accepted. Its two claims are not, and that is the finding
this repository exists for:

- a probe's request is `{method, path}` with no `headers`, so it cannot send the
  `Accept: application/json` that Plex needs to answer JSON at all;
- `media.serve`'s `catalogue` probe permits only JSON body assertions, so the
  XML escape hatch is closed by the capability as well as by the schema;
- and the expectation vocabulary is flat, so even reading JSON nothing could
  reach `MediaContainer.size`, one level down.

Every fixture in both published plugins is flat at the top level and neither
service needs content negotiation, which is why none of this has bitten before.
A catalogue of plugins chosen for being easy to write strains nothing.

There are also no recordings here. A fixture names the image digest it was taken
against, nobody has run this image, and inventing one would put the single kind
of wrong this apparatus exists to catch inside the apparatus. So `just manifest`
is red twice over, and both are the right reason.

`schema_version = 1` also permits exactly one service, and this plugin wants
two — argued and costed in SPEC.md.

Six open questions are at the end of SPEC.md. The first asks how a probe should
ask for JSON and how an expectation should reach into a nested body; it is not
specific to Plex and is worth answering whether or not this plugin is ever
built.
