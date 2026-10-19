#!/usr/bin/env python3
"""P4.D317: derive the three outfit-history restore archives from
`restore-archive.zip` (v4 `7953da3a0` carriers + `e8e17b2d9` bug 252).

No committed archive carries `data/wardrobe-history.json` (every one predates
it) nor, reachably, the four character-keyed chat columns, so no restore can be
seen running step 22n-ter or bug 252's re-key. Rebuilding the archive family
would add the file to EVERY archive (the P4.D264 rule), so these are
DERIVATIONS — every other entry and byte is kept; the history file is appended
LAST (as v4 writes it) and `counts.wardrobeHistory` is appended LAST.

The source carries Lorian `a1…01` (chat `c1…01`, messages `d1…01` / `d1…02`),
Riya `a1…02` (chat `c1…02`), the file `f0000001-…0001` and the coat item
`ac000000-0000-4000-8000-000000000001`. Hashes are v4's `hashEquippedSlots`
(sha256 of the compact five-slot JSON, 16 hex) unless a row says STALE.

1. `restore-archive-outfit-history.zip` — the happy path, both modes:
     h01  Lorian / c1, entry 0, `seeded`, null actor                 (cursor off)
     h02  Lorian / c1, entry 1, `operator`, actor Riya, the coat in `top`,
          a FOUR-key pre-hair `slots` and a STALE hash — `upsertRows`
          recomputes it                                              (THE cursor)
     h03  Lorian / c1, entry 2, `pascal`, actor Lorian,
          `actorToolName: dress_for_dinner`, the coat in `accessories`
     h04  Lorian / c1, entry 3, `character`, NO `actorToolName` key  (restores null)
     h05  Riya / c2, entry 0, `chat-start`, null actor               (cursor)
     h06  a PHANTOM character `a1…99` (not in `data/characters.json`) / c1 —
          new-account MINTS its id (R-9 (a)) while the chat pass DROPS the
          phantom's outfit entry
   and on `c1…01` in `data/chats.json`: `equippedOutfit` (Lorian: a four-slot
   bag with the coat in `top`; the phantom), `characterAvatars` (Lorian →
   `f0000001-…0001`), `courierCheckpoints` (Lorian → `d1…02`), and
   `pendingOutfitNotifications` (Lorian → a value kept as given).

2. `restore-archive-outfit-history-skips.zip` — the repository's per-row
   WARN-and-skip, `replace` mode (written rows 4 of 8):
     k01  Lorian / c1, entry 0, current                               WRITTEN
     k02  Lorian / c1, entry 1, current too — the partial UNIQUE cursor index
     k03  Lorian / c1, entry 0 again under another id — the UNIQUE index
     k04  a chat the archive does not carry (`c1…99`) — the FK
     k05  `entryIndex: true` — a value better-sqlite3 cannot bind
     k06  Riya / c2, entry 0, `source: "bogus"` — WRITTEN as given (reads
          back `seeded`)
     k07  Riya / c2, entry 1, `isCurrent: "false"` — truthy, so 1: the ONLY
          cursor of its timeline                                      WRITTEN
     k08  Lorian / c2, entry 0, no `actorToolName` key                WRITTEN

3. `restore-archive-outfit-history-bad-slot.zip` — a NON-ARRAY slot value
   (b02's `top` is a string): `replace` salvages it (`upsertRows` normalizes
   the bag — `top` reads `[]`, the hash recomputed), while `new-account`'s
   remap `.map`s it and THROWS, so the whole restore rejects before a write
   (R-9 (b), v4-faithful — a filing candidate).
     b01  Lorian / c1, entry 0, current
     b02  Riya / c2, entry 0, current, `slots.top: "ac000000-…0001"`
   (A `null` row cannot ride an archive at all: v4's streaming
   `readJsonArrayFile` refuses any top-level element that is not an object or
   an array, so the one input that throws out of `upsertRows`' own catch —
   making 22n-ter's outer catch fire — never reaches it.)

`restore-archive.zip` is md5-checked unchanged before and after.

Usage (from the v5 repo root):
  python3 harness/oracle/fixtures/derive-restore-archive-outfit-history.py
"""
import copy
import hashlib
import json
import pathlib
import zipfile

ROOT = pathlib.Path(__file__).resolve().parents[3]
ARCHIVES = ROOT / "crates/quilltap-web/tests/fixtures/restore-archives"
SRC = ARCHIVES / "restore-archive.zip"
SRC_MD5 = "6b076003bacf8db22c1bf8440fac09d6"

COAT = "ac000000-0000-4000-8000-000000000001"
LORIAN = "a1000000-0000-4000-8000-000000000001"
RIYA = "a1000000-0000-4000-8000-000000000002"
PHANTOM = "a1000000-0000-4000-8000-000000000099"
C1 = "c1000000-0000-4000-8000-000000000001"
C2 = "c1000000-0000-4000-8000-000000000002"
C_MISSING = "c1000000-0000-4000-8000-000000000099"
M2 = "d1000000-0000-4000-8000-000000000002"
PORTRAIT = "f0000001-0000-4000-8000-000000000001"
SLOTS = ["top", "bottom", "footwear", "accessories", "hair"]


def five(**slots):
    return {s: list(slots.get(s, [])) for s in SLOTS}


def v4_hash(slots):
    """v4 `hashEquippedSlots`: `bySlot((slot) => slots?.[slot] ?? [])`, compact
    JSON, sha256, 16 hex."""
    normalized = {s: (slots.get(s) if slots.get(s) is not None else []) for s in SLOTS}
    text = json.dumps(normalized, separators=(",", ":"), ensure_ascii=False)
    return hashlib.sha256(text.encode()).hexdigest()[:16]


def row(suffix, chat, character, index, slots, source, actor, current, at, tool=None, *, hash_=None,
        no_tool_key=False):
    r = {
        "id": f"7e100000-0000-4000-8000-0000000000{suffix}",
        "chatId": chat,
        "characterId": character,
        "entryIndex": index,
        "slots": slots,
        "slotsHash": hash_ if hash_ is not None else v4_hash(slots),
        "source": source,
        "actorCharacterId": actor,
        "actorToolName": tool,
        "isCurrent": current,
        "createdAt": at,
    }
    if no_tool_key:
        del r["actorToolName"]
    return r


HISTORY = [
    row("01", C1, LORIAN, 0, five(), "seeded", None, False, "2026-03-01T00:00:00.000Z"),
    row("02", C1, LORIAN, 1, {"top": [COAT], "bottom": [], "footwear": [], "accessories": []},
        "operator", RIYA, True, "2026-03-02T00:00:00.000Z", hash_="stalehash0000000"),
    row("03", C1, LORIAN, 2, five(accessories=[COAT]), "pascal", LORIAN, False,
        "2026-03-03T00:00:00.000Z", "dress_for_dinner"),
    row("04", C1, LORIAN, 3, five(), "character", LORIAN, False, "2026-03-04T00:00:00.000Z",
        no_tool_key=True),
    row("05", C2, RIYA, 0, five(), "chat-start", None, True, "2026-03-05T00:00:00.000Z"),
    row("06", C1, PHANTOM, 0, five(top=[COAT]), "character", PHANTOM, True,
        "2026-03-06T00:00:00.000Z"),
]

KEYED_COLUMNS = {
    "equippedOutfit": {
        LORIAN: {"top": [COAT], "bottom": [], "footwear": [], "accessories": []},
        PHANTOM: five(),
    },
    "characterAvatars": {
        LORIAN: {"imageId": PORTRAIT, "generatedAt": "2026-03-02T00:00:00.000Z", "afterMessageCount": 2},
    },
    "courierCheckpoints": {
        LORIAN: {"lastResolvedMessageId": M2, "resolvedAt": "2026-03-03T00:00:00.000Z"},
    },
    "pendingOutfitNotifications": {
        LORIAN: {"text": "Lorian changed into the coat.", "at": "2026-03-02T00:00:00.000Z"},
    },
}

SKIPS = [
    row("11", C1, LORIAN, 0, five(top=[COAT]), "character", LORIAN, True, "2026-03-01T00:00:00.000Z"),
    row("12", C1, LORIAN, 1, five(), "character", LORIAN, True, "2026-03-02T00:00:00.000Z"),
    row("13", C1, LORIAN, 0, five(), "character", LORIAN, False, "2026-03-03T00:00:00.000Z"),
    row("14", C_MISSING, LORIAN, 0, five(), "character", LORIAN, True, "2026-03-04T00:00:00.000Z"),
    row("15", C2, RIYA, True, five(), "character", RIYA, False, "2026-03-05T00:00:00.000Z"),
    row("16", C2, RIYA, 0, five(), "bogus", None, False, "2026-03-06T00:00:00.000Z"),
    row("17", C2, RIYA, 1, five(), "character", RIYA, "false", "2026-03-07T00:00:00.000Z"),
    row("18", C2, LORIAN, 0, five(), "character", None, True, "2026-03-08T00:00:00.000Z",
        no_tool_key=True),
]

BAD_SLOT = [
    row("21", C1, LORIAN, 0, five(top=[COAT]), "character", LORIAN, True, "2026-03-01T00:00:00.000Z"),
    row("22", C2, RIYA, 0, {"top": COAT, "bottom": [], "footwear": [], "accessories": [], "hair": []},
        "character", RIYA, True, "2026-03-02T00:00:00.000Z", hash_="0000000000000000"),
]


def dump(value):
    return (json.dumps(value, indent=2) + "\n").encode()


def edit_for(history, keyed):
    def edit(name, data):
        if name.endswith("/manifest.json"):
            m = json.loads(data)
            assert "wardrobeHistory" not in m["counts"], m["counts"]
            m["counts"]["wardrobeHistory"] = len(history)
            return dump(m)
        if keyed and name.endswith("/data/chats.json"):
            chats = json.loads(data)
            hits = [c for c in chats if c["id"] == C1]
            assert len(hits) == 1
            for column, value in KEYED_COLUMNS.items():
                assert hits[0].get(column) is None, column
                hits[0][column] = copy.deepcopy(value)
            return dump(chats)
        return None
    return edit


def derive(dst, history, keyed):
    edit = edit_for(history, keyed)
    with zipfile.ZipFile(SRC) as zin, zipfile.ZipFile(dst, "w", zipfile.ZIP_DEFLATED) as zout:
        infos = zin.infolist()
        root = infos[0].filename.rstrip("/").split("/")[0]
        last = infos[-1]
        for info in infos:
            data = zin.read(info.filename)
            new = edit(info.filename, data)
            out = zipfile.ZipInfo(info.filename, date_time=info.date_time)
            out.external_attr = info.external_attr
            out.compress_type = zipfile.ZIP_DEFLATED
            zout.writestr(out, data if new is None else new)
        path = f"{root}/data/wardrobe-history.json"
        assert path not in zin.namelist()
        out = zipfile.ZipInfo(path, date_time=last.date_time)
        out.external_attr = last.external_attr
        out.compress_type = zipfile.ZIP_DEFLATED
        zout.writestr(out, dump(history))
    print(f"wrote {dst.relative_to(ROOT)} md5 {hashlib.md5(dst.read_bytes()).hexdigest()}")


def main():
    before = hashlib.md5(SRC.read_bytes()).hexdigest()
    assert before == SRC_MD5, f"restore-archive.zip moved: {before}"
    derive(ARCHIVES / "restore-archive-outfit-history.zip", HISTORY, True)
    derive(ARCHIVES / "restore-archive-outfit-history-skips.zip", SKIPS, False)
    derive(ARCHIVES / "restore-archive-outfit-history-bad-slot.zip", BAD_SLOT, False)
    after = hashlib.md5(SRC.read_bytes()).hexdigest()
    assert after == SRC_MD5, f"restore-archive.zip moved during the derive: {after}"
    print(f"restore-archive.zip md5 {after} unchanged")


if __name__ == "__main__":
    main()
