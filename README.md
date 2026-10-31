# Plex for lemonfiber

**This plugin is not ready to install.** Its `plugin.toml` does not run the
adapter this repository builds yet, and its first-run flow asks Plex for a claim
Plex answers only to the machine it runs on. It is not in the
[lemonfiber plugin catalogue](https://github.com/lemonfiber/lemonfiber-plugins).

## What it is for

[Plex Media Server](https://github.com/plexinc/pms-docker) in a lemonfiber stack:
it serves your movies, TV and music to the Plex apps already on your television,
and signs the household in through Plex.

- **The service:** Plex Media Server, image `docker.io/plexinc/pms-docker` at tag
  `1.43.4.10903-e5521bd8c`, on port 32400, for the whole household, at
  `plex.<your domain>` and in the dashboard's Watch group.
- **The adapter:** `plex-adapter`, built here and published as
  `ghcr.io/lemonfiber/plex-adapter`. It claims Plex with the code plex.tv shows
  you, reads what Plex holds, looks over its libraries again and makes Plex serve
  its internal TLS port with the certificate lemonfiber makes for it. It tells a
  guard in front of Plex which paths serve an item's bytes, and answers the
  guard whether a member may have them by asking Plex with that member's own
  token.
- **Two checks for `lemonfiber doctor`:**

| Check | What it notices |
| --- | --- |
| Plex has an owner, so nobody else can become one | Nobody has claimed the server, so the first person on your network to reach it becomes its owner |
| Plex is not publishing itself to the internet on its own | Plex has opened its own way in from the internet through plex.tv's relay |

Plex is proprietary software. Its licence is Plex's, and this repository does
not contain it: it names an image.

## Getting help

- **A question:** ask on [Discord](https://discord.nightworks.io).
- **Something about this plugin:**
  [open an issue on this repository](https://github.com/lemonfiber/plugin-plex/issues).

## Licence

The plugin data and the adapter in this repository are under the Hippocratic
License 3.0 (HL3-CORE); see [LICENSE](LICENSE).

## Working on this plugin

How to work on the manifest and the adapter, and the checks CI runs:
[AGENTS.md](AGENTS.md).
