#!/usr/bin/env python3
"""Record every response in fixtures/ off the image plugin.toml pins.

**This is CI harness, not plugin content.** Nothing under `.github/` is
installed, and lemonfiber never runs any of it (`F3-R6`).

`F10-R4` asks for proofs that run against recorded responses, and a recording
is only evidence if it came out of the software it names. This is how every
file in `fixtures/` is made: it starts the image `plugin.toml` pins, never
claimed, and asks it each question from one of three places.

    host      the machine Docker runs on, through a published port, which
              Plex sees arrive from its network's gateway
    lan       another device on the server's own private network
              (10.232.0.0/24)
    outside   a device outside the private address ranges (100.100.0.0/24),
              on a network the server is attached to directly

Plex logs the address of every caller, and each recording is checked against
that log before it is written: a recording that says it was asked from the
gateway was, as far as Plex could tell. The two peers are containers of the
pinned image itself with `curl` as the entrypoint, so the recording needs no
image the manifest does not already name.

Nothing here claims the server. Claiming needs a plex.tv account, and none is
held for this repository.

    python3 .github/record.py                  write fixtures/
    python3 .github/record.py --check          record again and compare

`--check` fails where a fresh recording differs from the committed one in
anything but the values Plex makes up for each instance (`VARIES`).

Needs Docker and curl. Exit 0 = recorded (or matched), 1 = it did not.
"""

from __future__ import annotations

import argparse
import ipaddress
import json
import pathlib
import subprocess
import sys
import tempfile
import time
import tomllib
import uuid

ROOT = pathlib.Path(__file__).resolve().parents[1]
MANIFEST = "plugin.toml"
PORT = 32400
ACCEPT = "application/json"
STARTUP_S = 300
# How many answers in a row `/identity` must carry no `startState` for Plex to
# count as started, two seconds apart.
SETTLED = 10
# The most of a body that is not a document a recording keeps, the same as
# `prove.py` keeps of a live answer.
KEPT = 200

NETWORKS = {"lan": "10.232.0.0/24", "outside": "100.100.0.0/24"}

WHERE = {
    "host": "the machine Docker runs on, through a published port, which Plex logged as its "
            "network's gateway",
    "lan": "another device on the server's own private network, 10.232.0.0/24",
    "outside": "a device outside the private address ranges, on 100.100.0.0/24, a network the "
               "server is attached to directly",
}

UNCLAIMED = (
    "A fresh instance of this image, never claimed: no plex.tv account and no token. This is "
    "what an operator's Plex answers between install and the first-run flow, to a caller "
    "presenting nothing."
)

# The library the recordings are taken with. One, because a catalogue probe or a
# libraries check against an empty catalogue shows the shape and not the serving.
LIBRARY = (
    "/library/sections?name=Movies&type=movie&agent=tv.plex.agents.movie"
    "&scanner=Plex%20Movie&language=en-US&location=/data/movies"
)

# file, where from, path, why it is recorded
RECORDINGS = [
    ("identity-anonymous.json", "lan", "/identity",
     ("Public server information, answered to every caller. `claimed` is false, which is the "
     "answer `plex:claimed` exists to catch.")),
    ("library-sections-anonymous.json", "lan", "/library/sections",
     ("The catalogue refuses a device on the household network, with an HTML page rather than "
     "a document.")),
    ("library-sections-anonymous-outside.json", "outside", "/library/sections",
     ("The catalogue refuses a device outside the private ranges exactly as it refuses one "
     "inside them.")),
    ("accounts-anonymous.json", "lan", "/accounts",
     ("The account list refuses a device on the household network, with an HTML page rather "
     "than a document.")),
    ("accounts-anonymous-outside.json", "outside", "/accounts",
     ("The account list refuses a device outside the private ranges exactly as it refuses one "
     "inside them.")),
    ("library-sections-host.json", "host", "/library/sections",
     ("The catalogue, with one library pointed at /data/movies. Unclaimed, Plex admits a caller "
     "it sees arrive from its gateway, which is how Docker delivers a request the machine it "
     "runs on makes through a published port. That is the whole of the standing an unclaimed "
     "server's operator has.")),
    ("accounts-host.json", "host", "/accounts",
     ("The account list, admitted from the gateway as the catalogue is.")),
    ("prefs-not-published.json", "host", "/:/prefs",
     ("Server settings, admitted from the gateway. PublishServerOnPlexOnlineKey is one entry of "
     "MediaContainer.Setting, found by its `id`.")),
]

LOG = "/config/Library/Application Support/Plex Media Server/Logs/Plex Media Server.log"


class Instance:
    """One unclaimed Plex, the two devices that ask it things, and the host."""

    def __init__(self, image: str) -> None:
        run = uuid.uuid4().hex[:8]
        self.image = image
        self.server = f"plex-record-{run}"
        self.networks = {where: f"plex-record-{where}-{run}" for where in NETWORKS}
        self.clients = {where: f"plex-record-{where}-client-{run}" for where in NETWORKS}
        self.port = ""

    def __enter__(self) -> Instance:
        try:
            for where, subnet in NETWORKS.items():
                docker("network", "create", "--subnet", subnet, "--gateway", gateway(subnet),
                       self.networks[where])
            docker("run", "-d", "--name", self.server, "--network", self.networks["lan"],
                   "-p", f"127.0.0.1::{PORT}", self.image)
            docker("network", "connect", self.networks["outside"], self.server)
            for where in NETWORKS:
                docker("run", "-d", "--name", self.clients[where], "--network", self.networks[where],
                       "--entrypoint", "sleep", self.image, "infinity")
            self.port = docker("port", self.server, f"{PORT}/tcp").splitlines()[0].rsplit(":", 1)[1]
        except BaseException:
            self.__exit__()
            raise
        return self

    def __exit__(self, *_: object) -> None:
        for container in [self.server, *self.clients.values()]:
            docker("rm", "-f", container, check=False)
        for network in self.networks.values():
            docker("network", "rm", network, check=False)

    def expected_caller(self, where: str) -> str:
        """The address Plex should log for a request from here."""
        if where == "host":
            return gateway(NETWORKS["lan"])
        return docker("inspect", self.clients[where], "--format",
                      f"{{{{(index .NetworkSettings.Networks \"{self.networks[where]}\").IPAddress}}}}").strip()

    def ask(self, where: str, method: str, path: str) -> dict:
        """One request, as the recording format holds the answer."""
        if where == "host":
            command = ["curl", f"http://127.0.0.1:{self.port}{path}"]
        else:
            command = ["docker", "exec", self.clients[where], "curl", f"http://{self.server}:{PORT}{path}"]
        command[-1:-1] = ["-s", "-i", "-X", method, "-H", f"Accept: {ACCEPT}"]
        raw = subprocess.run(command, capture_output=True, check=True).stdout.decode("utf-8", "replace")
        head, _, body = raw.partition("\r\n\r\n")
        lines = head.split("\r\n")
        headers: dict[str, str] = {}
        for line in lines[1:]:
            name, _, value = line.partition(":")
            headers.setdefault(name.strip().lower(), value.strip())
        try:
            parsed = json.loads(body) if body else None
        except ValueError:
            parsed = None
        answer = {
            "status": int(lines[0].split()[1]),
            "headers": {"content-type": headers.get("content-type", "")},
            "json": parsed,
        }
        if parsed is None:
            answer["body_starts_with"] = body[:KEPT]
        return answer

    def logged_caller(self, method: str, path: str) -> str:
        """The address Plex logged for the latest request of this method and path."""
        log = docker("exec", self.server, "cat", LOG)
        marker = f"] {method} {path.split('?')[0]}"
        for line in reversed(log.splitlines()):
            if "Request: [" in line and marker in line:
                return line.split("Request: [", 1)[1].split("]", 1)[0].rsplit(":", 1)[0]
        return ""

    def started(self) -> None:
        """Wait until Plex has finished starting, not merely begun answering.

        `/identity` carries a `startState` while Plex is starting, and it comes
        back for a while after a library is made, so finished means absent on
        `SETTLED` answers in a row.
        """
        deadline = time.monotonic() + STARTUP_S
        quiet = 0
        while time.monotonic() < deadline:
            try:
                answer = self.ask("host", "GET", "/identity")
            except (subprocess.CalledProcessError, ValueError, IndexError):
                answer = {}
            body = answer.get("json") or {}
            if answer.get("status") == 200 and "startState" not in body.get("MediaContainer", {}):
                quiet += 1
                if quiet >= SETTLED:
                    return
            else:
                quiet = 0
            time.sleep(2)
        raise RuntimeError(f"Plex had not finished starting after {STARTUP_S}s")

    def furnished(self) -> None:
        """One library, and wait until Plex has finished its first scan of it."""
        docker("exec", self.server, "mkdir", "-p", "/data/movies")
        made = self.ask("host", "POST", LIBRARY)
        if made["status"] not in (200, 201):
            raise RuntimeError(f"creating the library answered {made['status']}")
        deadline = time.monotonic() + STARTUP_S
        while time.monotonic() < deadline:
            body = self.ask("host", "GET", "/library/sections").get("json") or {}
            sections = body.get("MediaContainer", {}).get("Directory", [])
            if sections and not any(section.get("refreshing") for section in sections):
                return
            time.sleep(2)
        raise RuntimeError(f"the library had not finished its first scan after {STARTUP_S}s")


def gateway(subnet: str) -> str:
    """The first address of a subnet, which is the gateway every network here is given."""
    return str(next(ipaddress.ip_network(subnet).hosts()))


def pinned() -> str:
    service = tomllib.loads((ROOT / MANIFEST).read_text(encoding="utf-8"))["service"][0]
    return f"{service['image']}@{service['digest']}"


def docker(*args: str, check: bool = True) -> str:
    done = subprocess.run(["docker", *args], capture_output=True, text=True, check=False)
    if check and done.returncode != 0:
        raise RuntimeError(f"docker {' '.join(args)}: {done.stderr.strip()}")
    return done.stdout


# Values Plex makes up for each instance or each second, by where they are in a
# recording. `--check` compares everything else.
VARIES = {
    "identity-anonymous.json": ["/MediaContainer/machineIdentifier"],
    "library-sections-host.json": [
        "/MediaContainer/Directory/0/uuid",
        "/MediaContainer/Directory/0/createdAt",
        "/MediaContainer/Directory/0/updatedAt",
        "/MediaContainer/Directory/0/scannedAt",
    ],
    "prefs-not-published.json": ["/MediaContainer/Setting/[id=MachineIdentifier]/value"],
}


def record(into: pathlib.Path) -> None:
    image = pinned()
    with Instance(image) as plex:
        plex.started()
        plex.furnished()
        plex.started()
        for name, where, path, why in RECORDINGS:
            answer = plex.ask(where, "GET", path)
            logged, expected = plex.logged_caller("GET", path), plex.expected_caller(where)
            if logged != expected:
                raise RuntimeError(
                    f"{name}: asked from {where}, which should reach Plex as {expected}, and "
                    f"Plex logged {logged or 'no such request'}"
                )
            recording = {
                "recorded_from": image,
                "note": f"{UNCLAIMED} Asked from {WHERE[where]}. {why}",
                "request": {"method": "GET", "path": path, "accept": ACCEPT},
                "response": answer,
            }
            (into / name).write_text(json.dumps(recording, indent=2) + "\n", encoding="utf-8")
            print(f"  {answer['status']}  {where:7}  {logged:12}  {path:18}  {name}")


def masked(document: dict, name: str) -> dict:
    """A recording with the values that vary per instance replaced by a marker.

    A step is a member name, a list index, or `[field=value]` picking the entry
    of a list whose field holds that value, as an expectation's key picks one.
    """
    for pointer in VARIES.get(name, []):
        at = document["response"]["json"]
        *way, last = pointer.strip("/").split("/")
        for step in way:
            if step.startswith("["):
                field, value = step[1:-1].split("=", 1)
                at = next(entry for entry in at if str(entry.get(field)) == value)
            else:
                at = at[int(step)] if isinstance(at, list) else at[step]
        at[last] = "<varies>"
    return document


def apart(ours: object, theirs: object, at: str = "") -> list[str]:
    """Every place two recordings hold different things, as a pointer."""
    if isinstance(ours, dict) and isinstance(theirs, dict):
        return [place for key in ours.keys() | theirs.keys()
                for place in apart(ours.get(key), theirs.get(key), f"{at}/{key}")]
    if isinstance(ours, list) and isinstance(theirs, list) and len(ours) == len(theirs):
        return [place for index, (one, other) in enumerate(zip(ours, theirs, strict=True))
                for place in apart(one, other, f"{at}/{index}")]
    return [] if ours == theirs else [at or "/"]


def check() -> int:
    with tempfile.TemporaryDirectory() as scratch:
        fresh = pathlib.Path(scratch)
        record(fresh)
        differ = []
        for name, *_ in RECORDINGS:
            committed = ROOT / "fixtures" / name
            if not committed.is_file():
                differ.append(f"fixtures/{name} is not committed")
                continue
            ours = masked(json.loads(committed.read_text(encoding="utf-8")), name)
            theirs = masked(json.loads((fresh / name).read_text(encoding="utf-8")), name)
            if ours != theirs:
                places = ", ".join(sorted(apart(ours, theirs))[:10])
                differ.append(f"fixtures/{name} is not what the pinned image answers now, at {places}")
    for line in differ:
        print(f"::error::{line}. Run `python3 .github/record.py` and commit fixtures/.")
    return 1 if differ else 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="record again and compare")
    args = parser.parse_args()
    if args.check:
        return check()
    record(ROOT / "fixtures")
    return 0


if __name__ == "__main__":
    sys.exit(main())
