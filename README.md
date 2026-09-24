# Plex for lemonfiber

**This plugin cannot be installed.** lemonfiber refuses its manifest, for two
reasons:

- **It needs something lemonfiber does not offer.** It asks for `recipe.run`,
  the ability to run a first-run flow against a service, and lemonfiber does not
  offer it.
- **It does not conform to the plugin format.** One of the questions it asks Plex sends a
  request header, which the format has no field for, and it names recorded
  responses that are not in this repository.

It is not in the [lemonfiber plugin catalogue](https://github.com/lemonfiber/lemonfiber-plugins),
and this repository has no instructions for installing it.

## What `plugin.toml` describes

The manifest here describes Plex Media Server as a service in a lemonfiber stack:

- **The service:** Plex Media Server, image `docker.io/plexinc/pms-docker` at tag
  `1.43.4.10903-e5521bd8c`, on port 32400, for the whole household, at
  `plex.<your domain>` and in the dashboard's Watch group. It mounts your data
  root and serves movies, TV and music.
- **A first-run flow:** claim the server with a claim code you fetch from
  plex.tv, read the server's token, create the libraries, and give the token to
  the stack's request service.
- **A secret:** the Plex token, held for the request service, which signs the
  household in through Plex.
- **A change to the stack:** the dashboard's Watch entry that points at Jellyfin.
- **Four checks for `lemonfiber doctor`:**

| Check | What it notices |
| --- | --- |
| Plex has an owner, so nobody else can become one | Nobody has claimed the server, so the first person on your network to reach it becomes its owner |
| The catalogue still refuses a caller presenting nothing | A setting inside Plex has made the whole library readable by anyone on your network |
| Plex is not publishing itself to the internet on its own | Plex has opened its own way in from the internet through plex.tv's relay |
| Every Plex library points where the stack files that media type | A Plex library is scanning a folder the stack does not file that media type into, so it stays empty |

Plex is proprietary software. Its licence is Plex's, and this repository does
not contain it: it names an image.

## Getting help

- **A question:** ask on [Discord](https://discord.nightworks.io).
- **Something about this plugin:**
  [open an issue on this repository](https://github.com/lemonfiber/plugin-plex/issues).

## Licence

The plugin data in this repository is under the Hippocratic License 3.0
(HL3-CORE); see [LICENSE](LICENSE).

## Working on this plugin

Where the manifest stands against lemonfiber's format, and what is needed for it
to be accepted: [docs/development.md](docs/development.md). The full design is in
[SPEC.md](SPEC.md).
