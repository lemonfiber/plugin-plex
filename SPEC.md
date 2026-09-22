# Plex, as a lemonfiber plugin

**Status:** Draft — specification only. Nothing here is built.

The plugin the extensibility features were written for. `F3` names it as "the hard
case this must survive"; `F4` uses it twice to explain why capabilities exist at
all; `F5` opens on it; `F8` calls substituting it "the case the whole design
exists for"; and the `[[service]]` example in the
[manifest contract](https://github.com/lemonfiber/spec/blob/main/20-architecture/contracts/plugin-manifest.md#service)
is already a Plex block, health path and all. `F2` keeps Plex out of the bundled
catalogue for not being open source, which is the reason it has to arrive this way
rather than an obstacle to it.

**Why this document exists.** The two published plugins — `plugin-komga` and
`plugin-uptime-kuma` — each install a container that nothing in the stack talks
to. Between them they exercise eight of the manifest's eleven declarable blocks
and none of the three with consequences in them. This plugin is specified to close
that, and to be the worked example every one of `F4`, `F8`, `F9` and `F10` is
measured against.

---

## What is dark today, and what this lights

| Surface | `plugin-komga` | `plugin-uptime-kuma` | Here |
|---|---|---|---|
| `[plugin]`, `[[service]]`, `[[proof]]`, `[requires]` | ✔ | ✔ | ✔ |
| `[[contribution]]` at `doctor.check` / `doctor.remedy` | ✔ | ✔ | ✔ |
| `[[claim]]` + `[[claim.probe]]` | ✔ | — | two capabilities, **both blocked** |
| A **core** capability that something asks for | inert | inert | `media.serve`, `identity.source` |
| `[[recipe]]` / `.step` / `.pair` | ✘ | ✘ | ✔ one capture, three pairs |
| `[[secret]]` | ✘ | ✘ | ✔ |
| `[[override]]` | ✘ | ✘ | ✔ |
| A call to a host outside the stack | ✘ | ✘ | ✔ `plex.tv` |
| Standing in for a bundled service (`F9`) | ✘ | ✘ | ✔ |
| More than one `[[service]]` | ✘ | ✘ | **refused by the schema — see below** |

### What writing it found

The draft manifest beside this file was written first and run against the
published schema and the contract second, which is the order that produces
findings rather than agreement. The first one is the reason this document is
worth having:

1. **The contract cannot express a claim that Plex satisfies.** Not "has not
   yet" — cannot. Three separate rules meet, and the section below sets it out.
2. **`schema_version = 1` permits exactly one service.** Not merely unexercised,
   refused: `plugin-manifest.md:74`, and the reader agrees. The two-service
   shape this plugin wants is a contract change, costed below.
3. **The recordings have to come from somewhere.** They cannot be written at a
   desk, and `F10`'s promise depends on somebody having made them once.

A fourth was found and is already gone: the interim validator derived a
namespaced capability's prefix from `service.id` rather than `plugin.id`, so a
plugin called `plex` declaring `plex:direct-play` on a service called
`plex-server` was refused. It never reached the real reader, whose
`namespaced_with(name, plugin)` takes the plugin id as an argument, and the
interim copy was rewritten out of existence by `plugin-template#7` while this
was being written. What survives is a test-coverage note: every fixture in
`claiming.rs` names its service after its plugin, so nothing would catch a
regression that passed the wrong one.

---

## The finding: Plex cannot make a claim

Raised as [lemonfiber/spec#458](https://github.com/lemonfiber/spec/issues/458),
and **measured** against `plexinc/pms-docker@sha256:e0ab2739…` (`1.43.4.10903`)
rather than argued from documentation. The recordings in `fixtures/` are that
run.

`media.serve` and `identity.source` are the reason to build this plugin. Neither
can currently be demonstrated by Plex, and the three rules that meet to prevent
it are each individually reasonable.

**One — a probe cannot carry a header.** `PluginRequest` in the published schema
is `{method, path}` with `additionalProperties: false`. There is nowhere to put
one.

**Two — Plex answers XML unless asked for JSON.** Measured, same path, same
second:

```
GET /identity                          GET /identity  (Accept: application/json)
Content-Type: text/xml;charset=utf-8   Content-Type: application/json
<MediaContainer size="0" …/>           {"MediaContainer":{"size":0, …}}
```

`Accept: application/json` is how you ask. Rule one says a probe cannot.

**Three — the capability requires a JSON assertion anyway.** `media.serve`'s
`catalogue` probe permits its body to be constrained by `json`, `json_has_keys`,
`json_types`, `json_at_least` or `json_array_min` and nothing else.
`content_type` and `body_starts_with` are not in that set, so the XML escape
hatch is closed by the capability rather than by the schema.

So the catalogue probe must assert JSON about a response that cannot be JSON.

**And a fourth, underneath, that would still bite if the first three were
fixed.** The expectation vocabulary is flat. `Expected` is documented as "three
kinds and no nesting", and the runner looks a key up with `key not in body` — a
top-level membership test, not a path. Plex nests every response one level down
under `MediaContainer`, so `json_has_keys = ["MediaContainer"]` is the only
assertion available and it says nothing at all. Writing `"MediaContainer.size"`
passes the schema and then looks for a key with a dot in its name, which is
worse than failing.

A path syntax would not be enough for the check this plugin most wants either.
*Is Plex publishing itself to the internet* lives at

```
MediaContainer.Setting[] → the entry whose id == "PublishServerOnPlexOnlineKey" → .value
```

— an array, filtered by a field, then a key. All four contributed doctor checks
drafted below are unexpressible, and for this one a predicate is needed rather
than a path.

### And a fourth, found by running it

An unclaimed Plex answers **200 to everything, anonymously**:

| Call | The probe requires | A fresh Plex answers |
|---|---|---|
| `GET /library/sections` | `401`/`403` — `media.serve` `guarded` | `200` and the catalogue |
| `GET /accounts` | `401`/`403` — `identity.source` `guarded` | `200` and `Account:[{"name":"Administrator",…}]` |

That is the default state between install and the first-run flow, and it is why
`plex:claimed` below is not the same check as Komga's. On Komga an unclaimed
server is an ownership risk: whoever asks first becomes administrator. Here it
is that **and** a disclosure — the account list is already readable by anything
on the household network.

It does not invalidate the claims: `F4` runs a probe against the recording, and
the recording would be of a claimed server. What it does mean is that the window
`F8`'s recipe closes is a window worth closing quickly, and that `F6` should
probably not put a service on the `lan` tier before its recipe has run.

### What the runner actually says

`prove.py` against the four recordings, verbatim but for the trailing detail:

```
  ok   proof  identity-before-claim         HTTP 200, and the body it declares
  FAIL proof  catalogue-refuses-anonymous   status 200, and it declares 401
  FAIL proof  accounts-refuses-anonymous    status 200, and it declares 401
  FAIL probe  media.serve/guarded           status 200, and it declares 401
  ???? probe  media.serve/catalogue         names …-operator.json, not in this source
  ok   probe  identity.source/identifies    HTTP 200, and the body it declares
  FAIL probe  identity.source/guarded       status 200, and it declares 401
  ???? check  plex:claimed                  names …-claimed.json, not in this source
  FAIL check  plex:no-anonymous-lan         status 200, and it declares 401
  FAIL check  plex:not-published            MediaContainer is {…151 settings…}
```

Two pass. Read what they are: `identity-before-claim` and
`identity.source/identifies` are the same call, and the only assertion the flat
vocabulary let either of them make is `json_has_keys = ["MediaContainer"]`.
**They pass by asserting that Plex replied.** The vocabulary asks a probe to
show *which server this is*; what got written is *there is an envelope*. A
green probe that establishes nothing is a worse outcome than a red one, and it
is the outcome the flat vocabulary forces.

The five `FAIL`s are honest and expected: unclaimed, Plex does not refuse
anybody. The two `????` need a claimed server. And `plex:not-published`'s
refusal printed all 151 settings into the message, which is a small separate
point about a refusal nobody can read.

### Why this has never bitten

Every recorded fixture in both published plugins is flat at the top level —
Komga's catalogue is `{content, totalElements, …}`, Uptime Kuma's entry page is
`{type, entryPage}` — and neither service needs content negotiation. The two
plugins that exist are exactly the two that would not notice.

That is the argument for a worked example in one sentence. A rule is only tested
by the case that strains it, and a catalogue of plugins chosen for being easy to
write is a catalogue that strains nothing.

### What would fix it

Three candidates, and they are not equivalent:

- **`headers` on `PluginRequest`.** Smallest, and it is a real widening: a probe
  that can send headers can send credentials, and `credential: "none"` on the
  `guarded` probes means what it means only because nothing can be sent today.
  Whatever shape this takes has to keep an anonymous probe anonymous.
- **A path syntax in the expectation keys.** `conforming.rs` already resolves
  JSON Pointers for schema references, so the machinery exists. This is the one
  with the widest reach: nesting under a root object is what most APIs do.
- **An `accept` field, narrower than headers.** Says the one thing content
  negotiation needs and grants none of the rest. Least general, least risk.

This is a decision, and it belongs to whoever owns `F4` rather than to this
plugin. What this plugin can say is that the gap is real, that it is reachable
from the first serious manifest anybody wrote, and that it blocks the case the
whole extensibility arc was designed around.

---

## Functional

### What it is for

Someone who already has Plex — a library they have curated for years, clients on
a television they are not going to replace, people they share with — wants the
rest of what lemonfiber does. Acquisition, filing, subtitles, requests, the
doctor, the front door. Today the answer is that lemonfiber files media into a
library and then serves it through Jellyfin, and Jellyfin is also what the request
service signs people in through. Wanting Plex instead means wanting both of those
to be Plex.

**`without_it`:** *the stack serves media through Jellyfin, and everyone signs in
through Jellyfin.*

### What the operator does

1. Chooses Plex from the plugin catalogue. The catalogue says what it will
   replace, because installing it is a substitution and not an addition.
2. Fetches a claim code from `plex.tv/claim` and pastes it in. This is the one
   value lemonfiber cannot obtain on their behalf, it is theirs, and it expires
   four minutes after it is issued — so the interface asks for it at the moment it
   is needed rather than in a form filled out earlier.
3. Waits. The recipe claims the server, reads the token back, creates a library
   for each media type the stack files, and hands the token to the request
   service so that signing in still works.
4. Finds Plex on the front door under **Watch**, where Jellyfin was.

### What it does not do

**It does not migrate anyone's accounts.** A household signed in through Jellyfin
does not become a household signed in through Plex. Plex accounts are plex.tv
accounts; they are created at plex.tv, by the person, and shared into the server
by the operator. The substitution re-points *what asks the question*; it cannot
re-point *who has already answered it*.

This is the sharpest edge in the whole plugin and it is a functional fact, not a
technical limitation to be worked around later. The catalogue entry and the
install flow both have to say it plainly, before the install and not after.

**It does not take the library with it.** Plex reads the same filed media from the
same data root. Nothing is copied, nothing moves, and uninstalling leaves the
library exactly as it was — which is what `takes_data` plus a read-only view of
the media root already gives, and is the reason substitution is reversible at all.

**It does not bring Plex's own remote access.** Plex will happily publish the
server to the internet through plex.tv's relay, and this plugin declares nothing
that turns that on. lemonfiber's reach-from-away is its own feature with its own
design, and a plugin that quietly opened a second door to the household's library
would be undetected reach of exactly the kind `ARCH-R91` refuses to tolerate. The
doctor contributes a check for it, below, because the setting can also be turned
on by hand inside Plex.

---

## Technical

### `[plugin]`

```toml
[plugin]
id          = "plex"
name        = "Plex"
version     = "1.0.0"
description = "Serves the library to the Plex clients already on the television, and signs the household in"
without_it  = "The stack serves media through Jellyfin, and everyone signs in through Jellyfin"
upstream    = "https://github.com/plexinc/pms-docker"
license     = "Proprietary"
forms       = ["tv", "movies", "music", "full"]
```

`license = "Proprietary"` is the first one, and it is the honest value. **Open
question 1**, below.

### `[[service]]` — one, and the argument for a second

```toml
[[service]]
id          = "plex"
name        = "Plex"
image       = "docker.io/plexinc/pms-docker"
digest      = "sha256:e0ab2739…"
tag         = "1.43.4.10903-e5521bd8c"
port        = 32400
bind        = "lan"
health      = { kind = "http", path = "/identity", timeout_s = 90 }
criticality = "important"
takes_data  = true
media_types = ["movies", "tv", "music"]
config_path = "/config"
provides    = ["media.serve", "identity.source", "plex:direct-play"]

# Proposed, and refused by schema_version 1 — see below.
# [[service]]
# id          = "plex-stats"
# name        = "Tautulli"
# image       = "ghcr.io/tautulli/tautulli"
# port        = 8181
# bind        = "loopback"
# criticality = "enhancing"
# provides    = ["plex:watch-history"]
```

**The second block is commented out in the draft manifest, because the schema
refuses it.** `schema_version = 1` permits exactly one service, deliberately —
`plugin-manifest.md:74`, enforced at `validate.py:1004`. So this is a proposal
against the contract rather than a declaration, and it is the clearest thing the
exercise turned up: the showcase cannot show the shape it most wants to.

What a second service would demonstrate, and nothing else can:

- **Two `bind` tiers in one manifest.** Plex is `lan` because the point of it is
  the television. Tautulli would be `loopback` because it is an operator surface
  with no business being reachable from the sofa. A plugin that can only declare
  one tier gives the author no way to be narrow about half of itself, which
  pushes toward the wider tier for the whole.
- **Two criticalities.** Losing Plex means nobody can watch; losing Tautulli
  means a graph is missing. `critical` is not available to a plugin at all
  (`plugin-manifest.md` § *A plugin may not declare itself `critical`*), so
  `important` is this one's correct ceiling.
- **A pair analysis with two in-stack destinations.** With Tautulli present, the
  captured token reaches `plex`, `seerr` **and** `plex-stats` — two of the three
  outside the service the plugin installed. That is the case `[[recipe.pair]]`
  exists for, at full strength.

**What it would cost.** Service ids are already required unique across the stack
and every installed plugin, so nothing about identity changes. What does change
is that `[wiring]`'s single `hostname` and single `dashboard_group` stop being
answerable for a plugin, and `F4-R8`'s collision rule has to say whether two
services of one plugin claiming one capability is a collision. Those are the two
real questions, and they are why "exactly one, in this version" is a defensible
place to have stopped rather than an oversight.

`plex:direct-play` is the namespaced example `F4` itself uses, kept.

#### A side note this turned up

Naming a service something other than its plugin also refused `plex:direct-play`
from a plugin called `plex`, because the interim validator derived the namespace
prefix from `service.id` while the contract gives it to the plugin (`F4-R4`,
`F4-R16`). It never reached the real reader — `namespaced_with(name, plugin)`
takes the plugin id as an argument — and the interim copy carrying it was
rewritten out of existence by `plugin-template#7` before this was filed.

Worth one line in the Rust crate all the same: every manifest in `claiming.rs`'s
tests names its service after its plugin, so nothing there would catch a
regression that passed the wrong id.

### `[[claim]]` — the two core capabilities, and how Plex satisfies each probe

The published vocabulary fixes four probes across the two capabilities. Plex
answers all four, and one of them is the reason this plugin is interesting rather
than merely large.

#### `media.serve`

| Probe | Credential | Required | The call |
|---|---|---|---|
| `guarded` | none | `401` or `403`, no body | `GET /library/sections`, `Accept: application/json` |
| `catalogue` | operator | `200`, JSON with keys | `GET /library/sections`, `X-Plex-Token: …`, `Accept: application/json` |

**The header in that table is the blocker.** Plex answers XML unless asked for
JSON, and the `catalogue` probe permits only JSON body assertions — but
`PluginRequest` has no `headers` field, so the ask cannot be written. The table
above describes the calls Plex needs; the contract cannot currently carry two of
them. See *The finding* above. What follows is what the probes would demonstrate
once it can, and is the reason closing that gap is worth the work.

**The `guarded` probe has real teeth here.** Plex has a setting — *List of IP
addresses and networks that are allowed without auth* — which, when set to the
household subnet, makes `GET /library/sections` answer `200` with the full
catalogue to anyone on the network. The service stays green on its health probe
the entire time. That is, word for word, the failure the vocabulary describes
`media.serve`'s `guarded` probe as existing to catch:

> A media server bound to the household network that stops refusing this keeps
> answering its health probe, keeps appearing green, and has published somebody's
> library. Nothing else would notice.

So this is not a probe that passes because nothing was ever going to make it fail.
There is a single checkbox inside Plex that breaks it, an operator can reach that
checkbox at any time after install, and the doctor check below is what notices.

#### `identity.source`

| Probe | Credential | Required | The call |
|---|---|---|---|
| `identifies` | none | `200`, JSON with keys | `GET /identity` |
| `guarded` | none | `401` or `403`, no body | `GET /accounts` |

`GET /identity` is the one call here that needs no header at all — Plex answers
it without a credential — so `identifies` is the single probe of the four this
plugin could bind today, except that its assertion would have to reach
`MediaContainer.machineIdentifier`, one level down, which the flat expectation
vocabulary cannot do either. It returns `machineIdentifier` and `version` to an
anonymous caller and nothing else. The vocabulary describes what it wants as *"which server this
is — the part with nothing of the household's in it"*, and Plex has an endpoint
that is precisely and only that. `GET /accounts` without a token is `401`.

`identity.source` is declared by exactly one bundled service — `jellyfin` — which
makes it the single most valuable place in the stack for substitution to be
demonstrated and the single most fragile place for it to be left unproven.

### `[wiring]`

```toml
[wiring]
hostname        = "plex"
dashboard_group = "Watch"
```

### `[[recipe]]` — one capture, three pairs

This is the block neither published plugin has, and the reason `F8` is separated
from `F3`: a recipe runs with lemonfiber's own authority.

```toml
[[recipe]]
id    = "claim-and-furnish"
title = "Claim the server, then point it at the library the stack already fills"
why   = """
A Plex server that has not been claimed belongs to whoever reaches it first, and
an unclaimed server on the household network is the same defect an unclaimed
Komga is. Claiming it is also the only way to obtain the token everything else
here needs, so the two are one flow rather than two.
"""

[[recipe.step]]
id      = "take-the-claim-code"
call    = { method = "GET", to = "plex", path = "/identity" }
expect  = { status = 200 }
capture = [{ name = "claim", from = "operator.claim_code", origin = "operator" }]

[[recipe.step]]
id     = "claim"
call   = { method = "POST", to = "plex", path = "/:/claim?token={{claim}}" }
expect = { status = 200 }

[[recipe.step]]
id      = "read-the-token"
call    = { method = "GET", to = "plex", path = "/:/prefs" }
expect  = { status = 200 }
capture = [{ name = "plex_token", from = "xml.PlexOnlineToken", origin = "stack-service" }]

[[recipe.step]]
id     = "make-the-libraries"
call   = { method  = "POST", to = "plex", path = "/library/sections",
           headers = { "X-Plex-Token" = "{{plex_token}}" } }
expect = { status = 200 }

[[recipe.step]]
id     = "tell-the-request-service"
call   = { method  = "POST", to = "seerr", path = "/api/v1/settings/plex",
           headers = { "X-Api-Key" = "{{plex_token}}" } }
expect = { status = 200 }

[[recipe.pair]]
value = "plex_token"
to    = "plex"

[[recipe.pair]]
value = "plex_token"
to    = "seerr"

[[recipe.pair]]
value = "claim"
to    = "plex"
```

**Why this shape is worth having as the first real recipe.** The pair analysis in
`validate.py` computes every flow a recipe *could* produce by reading it, and
fails on any flow without a declared pair behind it. Every recipe written so far
would make that analysis a check over an empty set. Here one captured value
reaches two destinations, one of them a service this plugin did not install —
which is the case the analysis exists for, and the case where a missing pair
would be an operator's credential arriving somewhere they never agreed to. With
Tautulli it would be three destinations and two such services, which is the
argument for the contract change put at its narrowest.

The claim code is captured with `origin = "operator"`. It is the only one of the
four permitted origins that means *the person typed this*, and the four-minute
expiry is why it is asked for here rather than held.

**The call outside the stack.** The claim exchange Plex performs on
`POST /:/claim` is server-to-plex.tv, so the recipe itself does not name an
external host. If the design later needs the token validated directly — `GET
https://plex.tv/api/v2/user` — that step names `plex.tv`, which `validate.py`
accepts as a DNS name and would refuse as an address. **Open question 3.**

### `[[secret]]`

```toml
[[secret]]
id  = "plex-token"
of  = "plex"
why = "The request service signs the household in through Plex"
```

`F3-R17` fails validation on a secret captured but not declared. Until now that
rule has had nothing to refuse.

### `[[override]]`

```toml
[[override]]
id  = "homepage.services.jellyfin"
why = "The front door's Watch group points at Jellyfin, and after this install it is not what serves the library"
```

`F3-R18` fails validation on a bundled thing changed but not declared, and this is
the first bundled thing any plugin has had cause to change. Note what it is *not*:
it does not remove Jellyfin, stop it, or touch its data. It changes which service
the front door's entry points at. Jellyfin keeps running unless the operator says
otherwise, which is what makes the substitution reversible.

### `[[contribution]]` — four checks, four remedies

Every check carries at least one remedy (`F3-R34`). Every id is namespaced
(`F4-R16`) and none collides with an occupied bundled identity (`F4-R17`).

| `id` | `category` | What it catches |
|---|---|---|
| `plex:claimed` | `services` | An unclaimed server belongs to whoever reaches it first |
| `plex:no-anonymous-lan` | `credentials` | The *allowed without auth* list is set, so the catalogue is public on the household network |
| `plex:not-published` | `network` | Plex's own remote access is on, which is a second door lemonfiber did not open |
| `plex:libraries-match` | `storage` | A Plex library points somewhere other than where the stack files that media type |

`plex:no-anonymous-lan` is the one to read twice. It is a contributed check whose
subject is the same condition a *claim probe* tests — the claim proves it at
install, and the check proves it every day after, because the thing that breaks it
is a human with a browser rather than an upstream release. A capability verified
once and never again is a capability verified at the least interesting moment.

`plex:libraries-match` is the check that catches the quietest failure in the whole
substitution: Plex scanning the wrong directory files nothing, breaks nothing,
throws nothing, and simply shows an empty library for a media type the stack is
still happily acquiring into.

### `[requires]`

```toml
[requires]
capabilities = [
    "service.add",
    "service.health.http",
    "doctor.contribute",
    "recipe.run",
]
```

`recipe.run` is what makes this plugin refuse to install on a build that cannot
run its recipe, **by naming that capability**. A build that parsed the recipe and
skipped it would install a Plex that was never claimed, never furnished, and
never handed its token to anything — a plugin whose declared behaviour is wider
than its actual one, which is the tolerated unknown `ARCH-R91` exists to refuse.

`validate.py` already enforces the pairing: a manifest with a `[[recipe]]` and no
`recipe.run` in `[requires]` fails today.

---

## Which release unlocks which part

The manifest validator already knows `recipe`, `secret` and `override`, so the
whole of this plugin can be **written and checked** before any of it runs — and
was, which is where the findings above came from. What each release adds is the
ability to *honour* a part of it.

| Part | Needs | Release |
|---|---|---|
| `[plugin]`, `[[service]]`, `[wiring]`, `[requires]` | `F3` | shape accepted ✔ |
| `[[recipe]]`, `[[secret]]`, `[[override]]` **declared** | the format already carries them | shape accepted ✔ |
| `[[claim]]` for `media.serve` and `identity.source` | **a request that can carry a header, and an expectation that can reach one level down** | blocked |
| `[[proof]]` and `[[contribution]]` against Plex's own responses | the same two | blocked |
| Substitution: seerr re-points with nothing edited | `F9` converts the wiring to ask | **0.17.0** |
| `[[recipe]]`, `[[secret]]`, `[[override]]` **honoured** | `F8` | **0.18.0** |
| A second service | a contract change | unscheduled |

"Shape accepted ✔" means the published schema accepts those blocks as written.
Nothing here has been run against a Plex, and the two blocked rows are blocked
by the contract rather than by this plugin: see *The finding* above. An earlier
draft of this table claimed the claims were checked, on the word of the interim
validator that `plugin-template#7` replaced — it permitted `headers` on a
request, which the published schema does not. Re-checking against the generated
schema is what turned the finding up, and is a small argument for `F10-R2`
having been worth closing.

### The recordings are a finding of their own

A fixture carries `recorded_from`, naming the image digest it was taken against.
That is what makes `F10`'s promise — *you should not have to own a Plex server to
write a plugin for Plex* — true for the second author and every author after.

It is not true for the first. Somebody has to run this image once, against the
four probes and the four contributed checks, and record what comes back. The
fixtures in this repository are therefore **absent rather than approximated**: a
recording with a plausible body and an invented `recorded_from` would be the one
kind of wrong this whole apparatus is built to catch, and it would be sitting
inside the apparatus.

So `just manifest` fails here today: once per missing recording, and again for
each request the contract cannot carry. Both are the correct state for it to be
in.

That ordering is the argument for specifying the whole thing now rather than the
runnable part of it. A conformance example written one release at a time gets
shaped by what each release happened to make easy. Written whole, it is a standing
test of whether `F9` and `F8` landed as specified — and the day `plugin-plex`'s
`just ci` goes green without its manifest changing is the day both did.

---

## Open questions

These are the six this cannot be written past. Each is a decision, not a
research task. The first is new, and it outranks the rest.

0. **How does a probe ask for JSON, and how does an expectation reach into a
   nested body?** The finding above. It blocks both of this plugin's claims, it
   is not specific to Plex, and it is the one question here that is worth
   answering whether or not this plugin is ever built.

1. **Does `F2`'s open-source constraint bind a plugin?** `f2-service-catalogue.md`
   excludes Plex from the *bundled* catalogue for not being open source. If that
   constraint extends to plugins, this document is about Emby and the rest of it
   is unchanged. If it does not — and the existence of a plugin system is a fair
   argument that it does not — then `[plugin] license = "Proprietary"` is the
   first of its kind and the catalogue needs to show it as such.

2. **What does the install flow say about accounts?** The substitution re-points
   which service is asked; it cannot re-point who has already answered. Someone
   signed in through Jellyfin has to make a plex.tv account and be shared in. This
   needs words before the install, and whoever writes them owns the most
   consequential sentence in the plugin.

3. **Is the token read back from `/:/prefs`, or from plex.tv?** `GET /:/prefs`
   returns `PlexOnlineToken` as XML, which means a `capture.from` of
   `xml.PlexOnlineToken` and therefore an XML capture path in a format whose every
   other example is JSON. The alternative is a step naming `plex.tv` — which
   exercises the external-destination rule properly, and costs a dependency on an
   external service inside a first-run flow.

4. **Does the second service earn a contract change?** The argument is above and
   the two open questions it raises — what `[wiring]`'s single `hostname` means
   for a plugin with two services, and whether two services of one plugin
   claiming one capability trips `F4-R8` — are the work. Answering *no* is
   respectable: it costs this plugin Tautulli and costs the pair analysis its
   third destination, and everything else in this document stands.

5. **Who records the first fixtures?** Someone with a Plex server, once. Until
   then this repository is a specification with a manifest beside it, and
   `just ci` is red for the right reason.

---

## Also raised, and out of scope here

Both came out of choosing this plugin over the alternatives, and both are **base
stack** work rather than plugin work:

- **A second `download.usenet`.** `indexer.search` has two bundled declarers and
  `library.curate` has four, but `download.usenet`, `download.torrent` and
  `identity.source` have one each. A capability with one implementation is a
  contract nobody has had to satisfy twice, which is where the interface is
  subtly wrong in the way that costs a plugin author a day.
- **Photos.** There is no photo capability, because `F9-R3` correctly refuses to
  publish one nothing bundled implements. Putting a photo server in the bundled
  stack is what makes a photo capability publishable, and until then every photo
  plugin's claims are namespaced and inert by construction.

---

## Related

- [`plugin-manifest.md`](https://github.com/lemonfiber/spec/blob/main/20-architecture/contracts/plugin-manifest.md) — the format, whose `[[service]]` example is this plugin
- [`capability-vocabulary.md`](https://github.com/lemonfiber/spec/blob/main/20-architecture/contracts/capability-vocabulary.md) — the four probes this is held to
- [`extension-points.md`](https://github.com/lemonfiber/spec/blob/main/20-architecture/contracts/extension-points.md) — `doctor.check` and `doctor.remedy`
- `F4` capabilities · `F8` recipes · `F9` bundled capabilities · `F10` authoring
