#!/usr/bin/env python3
"""P4.147 items 9 + 10(b): derive `restore-archive-informs.zip` from
`restore-archive.zip`.

No committed archive carries `data/chat-informs.json` (every one predates
Inform), so neither restore can be seen reading the standing flag
(`permanent`) off an archived row. This derivation adds that file — seven rows
on chat c…0001's one seat, `e1000000-…0001`:

  a9…01  permanent true          a standing inform        → restored, standing
  a9…02  permanent false         a one-shot               → restored, one-shot
  a9…03  (no permanent key)      a pre-standing archive   → restored, one-shot
  a9…04  permanent null          MALFORMED                → v4's Zod refuses it
  a9…05  permanent "true"        MALFORMED (a string)     → v4's Zod refuses it
  a9…06  permanent 1             MALFORMED (an integer)   → v4's Zod refuses it
  a9…07  permanent false, CONSUMED by the assistant message d1…02

`ChatInformSchema.permanent` is `z.boolean().default(false)`: absent → false,
and `null` / `"true"` / `1` fail the parse, so v4 skips those three rows with
`Failed to restore inform: <ZodError message>`.

It ALSO plants ONE malformed message (item 10(b), the message replay's serde
arm): chat c…0002 ("Quiet Interlude", otherwise message-less) gains a USER
message whose `content` is the NUMBER 5. v4's `addMessage` refuses it with its
ZodError; v5's typed decode refuses it with serde's sentence. Both skip it
with `Failed to restore message in chat "Quiet Interlude": …`.

The manifest gains `counts.chatInforms: 7` and `counts.messages` moves 2 → 3.
Every other entry and byte is kept; `data/chat-informs.json` is appended at
the end. `restore-archive.zip` is md5-checked unchanged before and after.

Usage (from the v5 repo root):
  python3 harness/oracle/fixtures/derive-restore-archive-informs.py
"""
import hashlib
import json
import pathlib
import zipfile

ROOT = pathlib.Path(__file__).resolve().parents[3]
ARCHIVES = ROOT / "crates/quilltap-web/tests/fixtures/restore-archives"
SRC = ARCHIVES / "restore-archive.zip"
DST = ARCHIVES / "restore-archive-informs.zip"
SRC_MD5 = "6b076003bacf8db22c1bf8440fac09d6"

C1 = "c1000000-0000-4000-8000-000000000001"
C2 = "c1000000-0000-4000-8000-000000000002"
SEAT1 = "e1000000-0000-4000-8000-000000000001"
SEAT2 = "e1000000-0000-4000-8000-000000000002"
ASSISTANT_MSG = "d1000000-0000-4000-8000-000000000002"
BATCH_A = "b9000000-0000-4000-8000-000000000001"
BATCH_B = "b9000000-0000-4000-8000-000000000002"
TS = "2026-03-05T00:00:00.000Z"
CONSUMED_TS = "2026-03-06T00:00:00.000Z"

ABSENT = object()
ROWS = [
    # (suffix, batch, permanent, consumed)
    ("01", BATCH_A, True, False),
    ("02", BATCH_A, False, False),
    ("03", BATCH_A, ABSENT, False),
    ("04", BATCH_B, None, False),
    ("05", BATCH_B, "true", False),
    ("06", BATCH_B, 1, False),
    ("07", BATCH_B, False, True),
]


def informs():
    out = []
    for suffix, batch, permanent, consumed in ROWS:
        row = {
            "id": f"a9000000-0000-4000-8000-0000000000{suffix}",
            "chatId": C1,
            "batchId": batch,
            "participantId": SEAT1,
            "contentMarkdown": f"Inform {suffix}: the barometer is falling.",
            "recordMessageId": None,
        }
        if permanent is not ABSENT:
            row["permanent"] = permanent
        row["createdAt"] = TS
        row["updatedAt"] = TS
        if consumed:
            row["consumedAt"] = CONSUMED_TS
            row["consumedByMessageId"] = ASSISTANT_MSG
        else:
            row["consumedAt"] = None
            row["consumedByMessageId"] = None
        out.append(row)
    return out


def rewrite_chats(chats):
    by_id = {c["id"]: c for c in chats}
    assert set(by_id) == {C1, C2}, sorted(by_id)
    assert by_id[C1]["participants"][0]["id"] == SEAT1
    assert by_id[C2]["participants"][0]["id"] == SEAT2
    assert by_id[C2]["messages"] == []
    assert ASSISTANT_MSG in {m["id"] for m in by_id[C1]["messages"]}
    by_id[C2]["messages"] = [
        {
            "type": "message",
            "id": "d1000000-0000-4000-8000-000000000009",
            "role": "USER",
            "content": 5,
            "attachments": [],
            "createdAt": "2026-03-04T00:00:00.000Z",
            "participantId": SEAT2,
        }
    ]
    return chats


def main():
    before = hashlib.md5(SRC.read_bytes()).hexdigest()
    assert before == SRC_MD5, f"restore-archive.zip moved: {before}"
    with zipfile.ZipFile(SRC) as zin, zipfile.ZipFile(DST, "w", zipfile.ZIP_DEFLATED) as zout:
        infos = zin.infolist()
        root = infos[0].filename.rstrip("/")
        assert f"{root}/data/chat-informs.json" not in zin.namelist()
        last = infos[-1]
        for info in infos:
            data = zin.read(info.filename)
            if info.filename.endswith("/data/chats.json"):
                data = (json.dumps(rewrite_chats(json.loads(data)), indent=2) + "\n").encode()
            elif info.filename.endswith("/manifest.json"):
                m = json.loads(data)
                assert m["counts"]["messages"] == 2 and "chatInforms" not in m["counts"]
                m["counts"]["messages"] = 3
                m["counts"]["chatInforms"] = len(ROWS)
                data = (json.dumps(m, indent=2) + "\n").encode()
            out = zipfile.ZipInfo(info.filename, date_time=info.date_time)
            out.external_attr = info.external_attr
            out.compress_type = zipfile.ZIP_DEFLATED
            zout.writestr(out, data)
        out = zipfile.ZipInfo(f"{root}/data/chat-informs.json", date_time=last.date_time)
        out.external_attr = last.external_attr
        out.compress_type = zipfile.ZIP_DEFLATED
        zout.writestr(out, (json.dumps(informs(), indent=2) + "\n").encode())
    after = hashlib.md5(SRC.read_bytes()).hexdigest()
    assert after == SRC_MD5, f"restore-archive.zip moved during the derive: {after}"
    print(f"wrote {DST.relative_to(ROOT)} (restore-archive.zip md5 {after} unchanged)")


if __name__ == "__main__":
    main()
