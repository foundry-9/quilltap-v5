#!/usr/bin/env python3
"""P4.D293 (Tier 1 item 10, L-11): derive `restore-archive-scenario-source.zip`
from `restore-archive.zip` — v4 `545e299bb`'s `chats.scenarioSource` /
`scenarioNotes` through both restore modes.

Mutations (each key placed after `title`, where a fresh chat row has it):

- chat `c1…01`: a VALID source (an object) naming the archived project store
  `5c17e916-…` + `scenarioNotes`.
- chat `c1…02`: a VALID source as a JSON STRING (a raw row's shape) naming the
  group store `60a8194d-…`.
- NEW chat `c1…03` ("The Bad Source"): an INVALID source (a bad uuid) —
  `replace` refuses the chat through `ChatMetadataSchema`, `new-account` nulls
  it (no mint) and restores the chat.

The source archive is md5-checked unchanged.

Usage (from the v5 repo root):
  python3 harness/oracle/fixtures/derive-restore-archive-scenario-source.py
"""
import copy
import json

from restore_archive_derive import derive, load

C = lambda n: f"c1000000-0000-4000-8000-00000000000{n}"
PROJECT_STORE = "5c17e916-5f79-4cca-a134-ec09c05924e9"
GROUP_STORE = "60a8194d-f8ea-4540-af77-5e13e7ed0e9b"
SHA = "cd" * 32


def with_after_title(row, pairs):
    out = {}
    for k, v in row.items():
        out[k] = v
        if k == "title":
            out.update(pairs)
    assert all(k in out for k, _ in pairs.items())
    return out


def plan(zin, root):
    stores = {p["id"] for p in load(zin, root, "doc-mount-points")}
    assert {PROJECT_STORE, GROUP_STORE} <= stores
    chats = load(zin, root, "chats")
    by_id = {c["id"]: i for i, c in enumerate(chats)}
    chats[by_id[C(1)]] = with_after_title(chats[by_id[C(1)]], {
        "scenarioSource": {"mountPointId": PROJECT_STORE, "path": "Scenarios/The Voyage.md", "bodySha256": SHA},
        "scenarioNotes": "The harbour at dusk.",
    })
    chats[by_id[C(2)]] = with_after_title(chats[by_id[C(2)]], {
        "scenarioSource": json.dumps(
            {"mountPointId": GROUP_STORE, "path": "Scenarios/The Crew.md", "bodySha256": SHA},
            separators=(",", ":"),
        ),
    })
    three = copy.deepcopy(chats[by_id[C(2)]])
    three["id"] = C(3)
    three["title"] = "The Bad Source"
    three["participants"] = []
    three["scenarioSource"] = {"mountPointId": "not-a-uuid", "path": "Scenarios/x.md", "bodySha256": SHA}
    three["scenarioNotes"] = "Notes beside a bad source."
    chats.append(three)
    return {"chats": chats}


if __name__ == "__main__":
    derive("restore-archive-scenario-source.zip", plan)
