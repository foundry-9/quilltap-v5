#!/usr/bin/env python3
"""P4.143 item 2 (the restore orchestrator's serde arm, P4.130's un-planted
follow-up): derive `restore-archive-chat-serde-arm.zip` from
`restore-archive.zip`.

The archive's two chats are kept byte for byte; ONE clone of c…0001 is added —
message-less and dependent-less (no message, memory, file or link names it),
so skipping it strands nothing — carrying `scenarioText: 5`:

  c…0006  (clone, no messages) scenarioText 5   → its Concierge columns are
          c…0001's own (valid or absent), so v5's `concierge_columns_zod_error`
          passes and the restore reaches v5's TYPED decode, which refuses the
          number with serde's sentence (``invalid type: integer `5`, expected a
          string``); v4's `stripScenarioSeededSummary` reads `scenarioText`
          only through `typeof … !== 'string'`, so the number reaches
          `repos.chats.create` → `validate` against `ChatMetadataBaseSchema`,
          which throws the ZodError (`invalid_type` at `["scenarioText"]`).
          Both sides' per-chat catch skips the chat with `Failed to restore
          chat "The Serde Room": <tail>` — the tails differ, and
          `system_restore_state`'s `classify_restore_serde_arm` pins that
          RECORDED divergence both ways.

The manifest's `counts.chats` moves 2 → 3 (the clone carries no messages, so
`counts.messages` stays 2). Entry order, names, `date_time` and every other
byte kept (the `derive-restore-archive-concierge-bogus.py` precedent). Writes a
NEW file; never reads and writes one path.

Usage (from the v5 repo root):
  python3 harness/oracle/fixtures/derive-restore-archive-chat-serde-arm.py
"""
import copy
import json
import pathlib
import zipfile

ROOT = pathlib.Path(__file__).resolve().parents[3]
ARCHIVES = ROOT / "crates/quilltap-web/tests/fixtures/restore-archives"
SRC = ARCHIVES / "restore-archive.zip"
DST = ARCHIVES / "restore-archive-chat-serde-arm.zip"
assert SRC != DST

C1 = "c1000000-0000-4000-8000-000000000001"
C2 = "c1000000-0000-4000-8000-000000000002"
C6 = "c1000000-0000-4000-8000-000000000006"

# v4 `ChatMetadataBaseSchema`'s three Concierge enums (the ones
# `concierge_columns_zod_error` checks before v5's typed decode).
CONCIERGE = {
    "conciergeMode": {"moderated", "unmoderated", "locked"},
    "conciergeModeSetBy": {"operator", "concierge"},
    "conciergeModeReason": {"manual", "refusals", "classifier", "migration"},
}


def rewrite_chats(chats):
    by_id = {c["id"]: c for c in chats}
    assert set(by_id) == {C1, C2}, sorted(by_id)
    c6 = copy.deepcopy(by_id[C1])
    c6.update(id=C6, title="The Serde Room", scenarioText=5, messages=[],
              messageCount=0)
    # The clone must PASS the Concierge check so v5 reaches the typed decode.
    for column, values in CONCIERGE.items():
        v = c6.get(column)
        assert v is None or v in values, (column, v)
    return chats + [c6]


def main():
    with zipfile.ZipFile(SRC) as zin, zipfile.ZipFile(DST, "w", zipfile.ZIP_DEFLATED) as zout:
        for info in zin.infolist():
            data = zin.read(info.filename)
            if info.filename.endswith("/data/chats.json"):
                data = (json.dumps(rewrite_chats(json.loads(data)), indent=2) + "\n").encode()
            elif info.filename.endswith("/manifest.json"):
                m = json.loads(data)
                assert m["counts"]["chats"] == 2
                m["counts"]["chats"] = 3
                data = (json.dumps(m, indent=2) + "\n").encode()
            out = zipfile.ZipInfo(info.filename, date_time=info.date_time)
            out.external_attr = info.external_attr
            out.compress_type = zipfile.ZIP_DEFLATED
            zout.writestr(out, data)
    print(f"wrote {DST.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
