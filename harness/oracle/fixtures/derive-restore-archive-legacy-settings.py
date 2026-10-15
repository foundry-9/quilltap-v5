#!/usr/bin/env python3
"""P4.D293 (Tier 1 item 11, L-4, L-13, R-AE): derive three archives from
`restore-archive.zip` — v4 `e0ac7493a`'s legacy settings translation at 22o
and `4ae0c4ca6`'s `textShapeVersion` at 22l.

`restore-archive-legacy-settings.zip`:
- the chat-settings row's `autoHousekeepingSettings` becomes a JSON STRING:
  `enabled: true`, `perCharacterCap: 50` (→ 100), overrides keyed by the
  ARCHIVE's character ids — Lorian `"abc"` (dropped), Riya `250000`
  (→ 100000) — plus the retired merge knobs (dropped). 22o derives
  `memoryMaintenance` and APPENDS it LAST; a new-account restore keeps the
  override keys the ARCHIVE's ids (R-AE).
- an instance row `memoryExtractionConcurrency` (DEAD — dropped).
- tf-idf: the source's row (no `textShapeVersion` → 0), NEW `b9…02` with `1`,
  NEW `b9…03` with `null` (REFUSED, v4's ZodError) — each on its own profile id
  (`a8…02` / `a8…03`, two non-default clones of the source's embedding
  profile — a migrated instance's `profileId` is UNIQUE and references
  `embedding_profiles`, measured on the vintage fixture).

`restore-archive-legacy-settings-own.zip`: the same, plus the archive's OWN
`memoryMaintenance` instance row — kept, the legacy value ignored.

`restore-archive-legacy-settings-null.zip`: `autoHousekeepingSettings: null`
(the key present — it qualifies, then derives nothing) and no dead row.

The source archive is md5-checked unchanged.

Usage (from the v5 repo root):
  python3 harness/oracle/fixtures/derive-restore-archive-legacy-settings.py
"""
import copy
import json

from restore_archive_derive import derive, load

LORIAN = "a1000000-0000-4000-8000-000000000001"
RIYA = "a1000000-0000-4000-8000-000000000002"


def plan_for(variant):
    def plan(zin, root):
        settings = load(zin, root, "chat-settings")
        assert len(settings) == 1 and "autoHousekeepingSettings" in settings[0]
        if variant == "null":
            settings[0]["autoHousekeepingSettings"] = None
        else:
            settings[0]["autoHousekeepingSettings"] = json.dumps({
                "enabled": True,
                "perCharacterCap": 50,
                "perCharacterCapOverrides": {LORIAN: "abc", RIYA: 250000},
                "autoMergeSimilarThreshold": 0.9,
                "mergeSimilar": False,
            }, separators=(",", ":"))
        instance = load(zin, root, "instance-settings")
        if variant != "null":
            instance.append({"key": "memoryExtractionConcurrency", "value": "4"})
        if variant == "own":
            instance.append({"key": "memoryMaintenance", "value": json.dumps(
                {"enabled": False, "perCharacterCap": 3000, "perCharacterCapOverrides": {}},
                separators=(",", ":"))})
        out = {"chat-settings": settings, "instance-settings": instance}
        if variant == "main":
            vocabs = load(zin, root, "tfidf-vocabularies")
            assert len(vocabs) == 1 and "textShapeVersion" not in vocabs[0]
            for n, shape in [(2, 1), (3, None)]:
                v = copy.deepcopy(vocabs[0])
                v["id"] = f"b9000000-0000-4000-8000-00000000000{n}"
                # One vocabulary per profile — a migrated instance's
                # `profileId` is UNIQUE (measured on the vintage fixture).
                v["profileId"] = f"a8000000-0000-4000-8000-00000000000{n}"
                keys = list(v.items())
                v = {}
                for k, val in keys:
                    v[k] = val
                    if k == "fittedAt":
                        v["textShapeVersion"] = shape
                vocabs.append(v)
            out["tfidf-vocabularies"] = vocabs
            # ...and each profile exists (a migrated instance's `profileId`
            # also references `embedding_profiles`): two clones of the
            # source's, not default, under their own names.
            profiles = load(zin, root, "embedding-profiles")
            assert [p["id"] for p in profiles] == ["a8000000-0000-4000-8000-000000000001"]
            for n in (2, 3):
                p = copy.deepcopy(profiles[0])
                p["id"] = f"a8000000-0000-4000-8000-00000000000{n}"
                p["name"] = f"Local Embeddings {n}"
                p["isDefault"] = False
                profiles.append(p)
            out["embedding-profiles"] = profiles
        return out
    return plan


if __name__ == "__main__":
    derive("restore-archive-legacy-settings.zip", plan_for("main"))
    derive("restore-archive-legacy-settings-own.zip", plan_for("own"))
    derive("restore-archive-legacy-settings-null.zip", plan_for("null"))
