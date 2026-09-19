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
| `[[claim]]` + `[[claim.probe]]` | ✔ | — | ✔ two capabilities |
| A **core** capability that something asks for | inert | inert | `media.serve`, `identity.source` |
| `[[recipe]]` / `.step` / `.pair` | ✘ | ✘ | ✔ one capture, three pairs |
| `[[secret]]` | ✘ | ✘ | ✔ |
| `[[override]]` | ✘ | ✘ | ✔ |
| A call to a host outside the stack | ✘ | ✘ | ✔ `plex.tv` |
| Standing in for a bundled service (`F9`) | ✘ | ✘ | ✔ |
| More than one `[[service]]` | ✘ | ✘ | **refused by the schema — see below** |

### What writing it found

The draft manifest beside this file was written first and run through the
contract validator second, which is the order that produces findings rather than
agreement. Four came out, and they are the substance of this document:

1. **`schema_version = 1` permits exactly one service.** Not merely unexercised —
   refused. `plugin-manifest.md:74` says `[[service]]  # exactly one, in this
   version` and `validate.py` enforces it. The two-service shape this plugin
   wants is a contract change, costed below, not something the manifest can
   express today.
2. **`provides` read the namespace off the wrong id**, and this plugin is what
   exposed it. Found, fixed, proven — below.
3. **Every other rule passes.** With recordings in place the draft manifest
   conforms on every rule the stand-in can check: the recipe's flow analysis, the
   secret, the override, all four contributions, both claims against the
   published vocabulary, and `recipe.run` in `[requires]`.
4. **The recordings have to come from somewhere.** They are the only thing
   outstanding, they cannot be written at a desk, and `F10`'s promise depends on
   somebody having made them once — below.

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
digest      = "sha256:…"
tag         = "1.41.9.9961"
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

#### The defect this found

Writing a two-service manifest meant naming a service something other than the
plugin, and that refused `plex:direct-play` — `F4`'s own example of a well-formed
namespaced capability — from a plugin called `plex`:

```
[[service]].provides: 'plex:direct-play' is neither a core name … nor namespaced
with this plugin's id (plex-server:…)
```

The message says *this plugin's id* and prints the **service's**. `validate.py`
derived the prefix from `service.id` in `validate_provides` and from `plugin.id`
in the contribution half of the same file, and the contract gives the namespace
to the plugin in both places (`F4-R4`, `F4-R16`). Both published plugins name
their one service after themselves, so the two readings agreed and the
disagreement cost nothing.

Fixed in the shared harness, with a self-test case on each side of the edge: a
plugin-namespaced capability on a differently-named service must be **accepted**,
and another plugin's namespace must be **refused**. The acceptance case is the
load-bearing one — the refusal alone passes just as well against the broken
code. Verified red before, green after, and propagated to `plugin-template`,
`plugin-komga` and `plugin-uptime-kuma`, whose self-tests all still pass.

### `[[claim]]` — the two core capabilities, and how Plex satisfies each probe

The published vocabulary fixes four probes across the two capabilities. Plex
answers all four, and one of them is the reason this plugin is interesting rather
than merely large.

#### `media.serve`

| Probe | Credential | Required | The call |
|---|---|---|---|
| `guarded` | none | `401` or `403`, no body | `GET /library/sections`, `Accept: application/json` |
| `catalogue` | operator | `200`, JSON with keys | `GET /library/sections`, `X-Plex-Token: …`, `Accept: application/json` |

`Accept: application/json` is not decoration. Plex answers XML by default and the
`catalogue` probe requires `json` in its body assertions, so a manifest that
omitted the header would fail a claim the service actually satisfies. It is the
first authoring detail in this plugin that cannot be guessed from the contract
prose, which makes it exactly the sort of thing `F10`'s worked example exists to
carry.

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

`GET /identity` returns `machineIdentifier` and `version` to an anonymous caller
and nothing else. The vocabulary describes what it wants as *"which server this
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
| Manifest, service, proofs, doctor contributions | `F3`, `F4` | **0.16.0** — checked ✔ |
| `[[claim]]` for `media.serve` and `identity.source` | the published vocabulary | **0.16.0** — checked ✔ |
| `[[recipe]]`, `[[secret]]`, `[[override]]` **declared** | the format, which already carries them | now — checked ✔ |
| Substitution: seerr re-points with nothing edited | `F9` converts the wiring to ask | **0.17.0** |
| `[[recipe]]`, `[[secret]]`, `[[override]]` **honoured** | `F8` | **0.18.0** |
| A second service | a contract change | unscheduled |

"Checked ✔" means exactly what it says and no more: run against the contract
stand-in with recordings in place, `plugin.toml` *conforms on every rule that
stand-in can check*. It has never been run against a Plex.

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

So `just manifest` fails here today, eleven times, once per missing recording,
and that is the correct state for it to be in.

That ordering is the argument for specifying the whole thing now rather than the
runnable part of it. A conformance example written one release at a time gets
shaped by what each release happened to make easy. Written whole, it is a standing
test of whether `F9` and `F8` landed as specified — and the day `plugin-plex`'s
`just ci` goes green without its manifest changing is the day both did.

---

## Open questions

These are the five this cannot be written past. Each is a decision, not a
research task.

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
