#!/usr/bin/env python3
"""Generate harness/oracle/fixtures/enclave-step-tier3.json — the U4.4 capstone
corpus (single-authored; both the fixture builder, the jest oracle, and the Rust
harness read this file). Regenerate: python3 gen-enclave-step-spec.py
"""
import json
from datetime import datetime, timezone

BASE_MS = 1767326645000  # 2026-01-02T03:04:05.000Z
STEP_MS = 600_000        # 10 min between call anchors
SEED_TS = "2026-01-01T00:00:00.000Z"

def iso(ms):
    return datetime.fromtimestamp(ms / 1000, tz=timezone.utc).strftime("%Y-%m-%dT%H:%M:%S.") + f"{ms % 1000:03d}Z"

def a(i):
    return BASE_MS + i * STEP_MS

USER1 = "e18e05bc-63e8-4539-8a85-719b7a508850"
USER2 = "f2222222-2222-4222-8222-222222222222"
PROFILE = "a1cb6066-2fcc-4006-a7db-ddb2fc6e2221"
ADA = "969a21df-73ad-465f-8f6c-2bc52d90ddc6"
BYRON = "a990117b-2f44-466f-a853-6b4d4c2db52d"

def run_id(n):
    return f"00000000-0000-4000-8000-0000000run{n:02d}"[:22] + f"{n:014d}"

def rid(n):
    # uuid-shaped pinned run ids
    return f"aaaaaaa{n % 10}-0000-4000-8000-{n:012d}"

def pid(chat_n, which):
    return f"bbbbbbb{which}-0000-4000-8000-{chat_n:012d}"

def mid(chat_n, msg_n):
    return f"ccccccc0-0000-4000-8000-{chat_n:06d}{msg_n:06d}"

def chat_id(n):
    return f"dddddddd-0000-4000-8000-{n:012d}"

def participants(n, status="active"):
    return [
        {"id": pid(chat_n=n, which=1), "character": ADA, "status": status},
        {"id": pid(chat_n=n, which=2), "character": BYRON, "status": status},
    ]

def opener(n):
    # One seeded assistant message from pA so the selection picks pB.
    return [{
        "id": mid(n, 1), "role": "ASSISTANT", "content": f"Opening remark {n}.",
        "participantId": pid(n, 1),
    }]

chats = []
calls = []
streams = {}

def add_turn_call(name, chat, run, stream_chunks, job_id=None, job_created_at=None):
    i = len(calls)
    label = name if stream_chunks is not None else None
    calls.append({
        "name": name, "kind": "turn", "chatId": chat, "userId": USER1,
        "runId": run, "jobId": job_id or f"eeeeeeee-0000-4000-8000-{i:012d}",
        "jobCreatedAt": job_created_at or iso(a(i) - 5000),
        "streamLabel": label, "anchorMs": a(i),
    })
    if stream_chunks is not None:
        streams[label] = stream_chunks

def turn_stream(text, usage=None):
    done: dict = {"done": True}
    if usage:
        done["usage"] = {"promptTokens": usage[0], "completionTokens": usage[1], "totalTokens": usage[2]}
    return [[{"content": text}, done]]

# --- Case 0: daily gate broken-zero + normal re-enqueue -----------------------
n = 1
c = {"id": chat_id(n), "userId": USER1, "title": "Daily broken zero",
     "participants": participants(n), "messages": opener(n),
     "columns": {"chatType": "autonomous", "runState": "running",
                  "currentRunId": rid(n), "runStartedAt": SEED_TS}}
chats.append(c)
add_turn_call("daily_broken_zero_reenqueue", chat_id(n), rid(n),
              turn_stream("Quite so; the ledgers agree.", (40, 8, 48)))

# --- Case 1: budget_turns_end (post-turn end; organic stream usage) ----------
n = 2
c = {"id": chat_id(n), "userId": USER1, "title": "Turns end",
     "participants": participants(n), "messages": opener(n),
     "columns": {"chatType": "autonomous", "runState": "running",
                  "currentRunId": rid(n),
                  "runStartedAt": iso(a(1) - 49_600),
                  "budgetMaxTurns": 2, "runTurnsConsumed": 1,
                  "runMilestonesAnnounced": 3}}
chats.append(c)
add_turn_call("budget_turns_end", chat_id(n), rid(n),
              turn_stream("A final word, then.", (30, 6, 36)))

# --- Case 2: budget_tokens_end (seeded tagged rows + stream usage) -----------
n = 3
c = {"id": chat_id(n), "userId": USER1, "title": "Tokens end",
     "participants": participants(n), "messages": opener(n),
     "columns": {"chatType": "autonomous", "runState": "running",
                  "currentRunId": rid(n),
                  "runStartedAt": iso(a(2) - 120_400),
                  "budgetMaxTokens": 100, "runMilestonesAnnounced": 3}}
chats.append(c)
add_turn_call("budget_tokens_end", chat_id(n), rid(n),
              turn_stream("Spending the last of the allowance.", (10, 5, 15)))

# --- Case 3: budget_tokens_cachehits (budgetExcludeCacheHits = 0) ------------
n = 4
c = {"id": chat_id(n), "userId": USER1, "title": "Cache-hit tokens end",
     "participants": participants(n), "messages": opener(n),
     "columns": {"chatType": "autonomous", "runState": "running",
                  "currentRunId": rid(n),
                  "runStartedAt": iso(a(3) - 30_400),
                  "budgetMaxTokens": 100, "budgetExcludeCacheHits": 0,
                  "runMilestonesAnnounced": 3}}
chats.append(c)
add_turn_call("budget_tokens_cachehits", chat_id(n), rid(n),
              turn_stream("Counting every token now."))

# --- Case 4: budget_time_pre_end (pre-turn END + cron recompute) -------------
n = 5
c = {"id": chat_id(n), "userId": USER1, "title": "Wall clock pre end",
     "participants": participants(n), "messages": opener(n),
     "columns": {"chatType": "autonomous", "runState": "running",
                  "currentRunId": rid(n),
                  "runStartedAt": iso(a(4) - 100_000),
                  "budgetMaxWallClockMs": 60_000,
                  "scheduleCron": "0 * * * *",
                  "runMilestonesAnnounced": 2}}
chats.append(c)
add_turn_call("budget_time_pre_end", chat_id(n), rid(n), None)

# --- Case 5+6: grace grant (pre-turn) then grace completion ------------------
n = 6
c = {"id": chat_id(n), "userId": USER1, "title": "Grace pre",
     "participants": participants(n), "messages": opener(n),
     "columns": {"chatType": "autonomous", "runState": "running",
                  "currentRunId": rid(n),
                  "runStartedAt": iso(a(6) - 60_400),
                  "budgetMaxTurns": 1, "runTurnsConsumed": 1}}
chats.append(c)
add_turn_call("grace_pre_grant", chat_id(n), rid(n), None)
add_turn_call("grace_completion", chat_id(n), rid(n),
              turn_stream("One last word, and the scene closes.", (20, 4, 24)))

# --- Case 7: grace_post_vault (post-turn grant) -------------------------------
n = 7
c = {"id": chat_id(n), "userId": USER1, "title": "Grace post vault",
     "participants": participants(n), "messages": opener(n),
     "columns": {"chatType": "autonomous", "runState": "running",
                  "currentRunId": rid(n), "runStartedAt": SEED_TS,
                  "budgetMaxTurns": 1}}
chats.append(c)
add_turn_call("grace_post_vault", chat_id(n), rid(n),
              turn_stream("Vaulting straight past the warning band.", (15, 3, 18)))

# --- Case 8: milestone_halfway -------------------------------------------------
n = 8
c = {"id": chat_id(n), "userId": USER1, "title": "Halfway",
     "participants": participants(n), "messages": opener(n),
     "columns": {"chatType": "autonomous", "runState": "running",
                  "currentRunId": rid(n), "runStartedAt": SEED_TS,
                  "budgetMaxTurns": 4, "runTurnsConsumed": 1}}
chats.append(c)
add_turn_call("milestone_halfway", chat_id(n), rid(n),
              turn_stream("Midway through our exchanges."))

# --- Case 9: milestone_nearend_vault (0.95, mask 0 → both bits) ---------------
n = 9
c = {"id": chat_id(n), "userId": USER1, "title": "Near end vault",
     "participants": participants(n), "messages": opener(n),
     "columns": {"chatType": "autonomous", "runState": "running",
                  "currentRunId": rid(n), "runStartedAt": SEED_TS,
                  "budgetMaxTurns": 20, "runTurnsConsumed": 18}}
chats.append(c)
add_turn_call("milestone_nearend_vault", chat_id(n), rid(n),
              turn_stream("The pocket-watch ticks on."))

# --- Case 10: stale_run --------------------------------------------------------
n = 10
c = {"id": chat_id(n), "userId": USER1, "title": "Stale run",
     "participants": participants(n), "messages": opener(n),
     "columns": {"chatType": "autonomous", "runState": "running",
                  "currentRunId": rid(90), "runStartedAt": SEED_TS}}
chats.append(c)
add_turn_call("stale_run", chat_id(n), rid(n), None)

# --- Case 11: sibling tie-break yield -----------------------------------------
n = 11
c = {"id": chat_id(n), "userId": USER1, "title": "Sibling yield",
     "participants": participants(n), "messages": opener(n),
     "columns": {"chatType": "autonomous", "runState": "running",
                  "currentRunId": rid(n), "runStartedAt": SEED_TS}}
chats.append(c)
i = len(calls)
job_created = iso(a(i) - 5000)
add_turn_call("sibling_tie_yield", chat_id(n), rid(n), None,
              job_id=f"ffffffff-0000-4000-8000-{i:012d}",
              job_created_at=job_created)
# The seeded elder PROCESSING sibling: SAME createdAt, lexically-smaller id.
sibling_job = {
    "id": f"00000000-0000-4000-8000-0000000000ab",
    "userId": USER1, "type": "AUTONOMOUS_ROOM_TURN", "status": "PROCESSING",
    "payload": {"chatId": chat_id(n), "runId": rid(n)},
    "createdAt": job_created,
}

# --- Case 12: paused → not active ---------------------------------------------
n = 12
c = {"id": chat_id(n), "userId": USER1, "title": "Paused room",
     "participants": participants(n), "messages": opener(n),
     "columns": {"chatType": "autonomous", "runState": "paused",
                  "currentRunId": rid(n), "runStartedAt": SEED_TS}}
chats.append(c)
add_turn_call("paused_not_active", chat_id(n), rid(n), None)

# --- Case 13: idle→running fallback (banner + LOCAL counter pinning) ----------
n = 13
c = {"id": chat_id(n), "userId": USER1, "title": "Idle fallback",
     "participants": participants(n), "messages": opener(n),
     "columns": {"chatType": "autonomous", "runState": "idle",
                  "currentRunId": rid(n),
                  "runStartedAt": SEED_TS,
                  "runTurnsConsumed": 7, "runTokensConsumed": 7000,
                  "runMilestonesAnnounced": 3,
                  "budgetMaxTokens": 1_000_000}}
chats.append(c)
add_turn_call("idle_fallback", chat_id(n), rid(n),
              turn_stream("Rising to the occasion.", (25, 5, 30)))

# --- Case 14: no eligible speaker → error --------------------------------------
n = 14
c = {"id": chat_id(n), "userId": USER1, "title": "No speaker",
     "participants": participants(n, status="removed"), "messages": [],
     "columns": {"chatType": "autonomous", "runState": "running",
                  "currentRunId": rid(n), "runStartedAt": SEED_TS}}
chats.append(c)
add_turn_call("no_speaker_error", chat_id(n), rid(n), None)

# --- Case 15: mid-turn stream error (the shell swallow) ------------------------
n = 15
c = {"id": chat_id(n), "userId": USER1, "title": "Stream error",
     "participants": participants(n), "messages": opener(n),
     "columns": {"chatType": "autonomous", "runState": "running",
                  "currentRunId": rid(n), "runStartedAt": SEED_TS}}
chats.append(c)
add_turn_call("stream_error_swallow", chat_id(n), rid(n),
              [[{"error": "model exploded"}]])

# --- Case 16: the summary fold fires -------------------------------------------
n = 16
fold_msgs = []
for k in range(11):
    fold_msgs.append({
        "id": mid(n, k + 1), "role": "ASSISTANT",
        "content": f"Fold seed line {k + 1}.",
        "participantId": pid(n, 1 if k % 2 == 0 else 2),
    })
c = {"id": chat_id(n), "userId": USER1, "title": "Fold room",
     "participants": participants(n), "messages": fold_msgs,
     "columns": {"chatType": "autonomous", "runState": "running",
                  "currentRunId": rid(n), "runStartedAt": SEED_TS}}
chats.append(c)
add_turn_call("fold_fires", chat_id(n), rid(n),
              turn_stream("And that makes a dozen.", (35, 7, 42)))

# --- Tick chats (user 2) --------------------------------------------------------
T1, T2, T3, T4 = chat_id(21), chat_id(22), chat_id(23), chat_id(24)
tick_anchor = a(len(calls))
chats.append({"id": T1, "userId": USER2, "title": "Tick seed",
              "participants": participants(21), "messages": [],
              "columns": {"chatType": "autonomous", "runState": "idle",
                           "scheduleCron": "*/15 * * * *"}})
chats.append({"id": T2, "userId": USER2, "title": "Tick stale",
              "participants": participants(22), "messages": [],
              "columns": {"chatType": "autonomous", "runState": "idle",
                           "scheduleCron": "0 3 * * *",
                           "scheduleNextRunAt": iso(tick_anchor - 24 * 3600 * 1000)}})
chats.append({"id": T3, "userId": USER2, "title": "Tick fresh",
              "participants": participants(23), "messages": [],
              "columns": {"chatType": "autonomous", "runState": "idle",
                           "scheduleCron": "*/15 * * * *",
                           "scheduleNextRunAt": iso(tick_anchor - 60_000)}})
chats.append({"id": T4, "userId": USER2, "title": "Tick wedged",
              "participants": participants(24), "messages": [],
              "columns": {"chatType": "autonomous", "runState": "running",
                           "currentRunId": rid(24)}})
calls.append({"name": "tick_first", "kind": "tick", "userId": USER2,
              "anchorMs": tick_anchor})
calls.append({"name": "tick_second", "kind": "tick", "userId": USER2,
              "anchorMs": a(len(calls))})

# --- Seeded llm_logs rows --------------------------------------------------------
llm_log_rows = [
    # Tagged spend for budget_tokens_end (run 3): 100 total.
    {"id": "11111111-0000-4000-8000-000000000301", "userId": USER1,
     "chatId": chat_id(3), "autonomousRunId": rid(3),
     "usage": {"promptTokens": 80, "completionTokens": 20, "totalTokens": 100},
     "createdAt": iso(a(2) - 60_000)},
    # Tagged spend for budget_tokens_cachehits (run 4): 50 + 60 cache reads.
    {"id": "11111111-0000-4000-8000-000000000401", "userId": USER1,
     "chatId": chat_id(4), "autonomousRunId": rid(4),
     "usage": {"promptTokens": 40, "completionTokens": 10, "totalTokens": 50},
     "cacheUsage": {"cacheReadInputTokens": 60},
     "createdAt": iso(a(3) - 60_000)},
    # UNTAGGED user spend since local midnight — far over dailyTokenBudget 500;
    # the BROKEN-BUT-EXACT since-read sums it to 0, so nothing ever pauses.
    {"id": "11111111-0000-4000-8000-000000000001", "userId": USER1,
     "usage": {"promptTokens": 5000, "completionTokens": 5000, "totalTokens": 10000},
     "createdAt": "2026-01-02T02:00:00.000Z"},
]

spec = {
    "testPepperBase64": "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=",
    "seedTimestamp": SEED_TS,
    "baseMs": BASE_MS,
    "stepMs": STEP_MS,
    "user1": USER1,
    "user2": USER2,
    "connectionProfile": {"id": PROFILE, "name": "Primary",
                           "provider": "ANTHROPIC", "modelName": "claude-sonnet"},
    "chatSettings": {
        "id": "475d8e74-4bdc-4d46-a9c1-62c382b76116",
        "dailyTokenBudget": 500,
    },
    "characters": [
        {"id": ADA, "name": "Ada", "description": "A methodical analyst.",
         "talkativeness": 0.5, "connectionProfileId": PROFILE},
        {"id": BYRON, "name": "Byron", "description": "A romantic conversationalist.",
         "talkativeness": 0.5, "connectionProfileId": PROFILE},
    ],
    "chats": chats,
    "siblingJob": sibling_job,
    "llmLogRows": llm_log_rows,
    "calls": calls,
    "streams": streams,
    "cheapCompletion": {
        "response": "{\"keywords\": [\"salon\"], \"paraphrase\": \"the salon conversation\"}",
        "usage": {"promptTokens": 12, "completionTokens": 6, "totalTokens": 18},
    },
    "foldCompletion": {
        "response": "The company traded a dozen observations in the salon.",
        "usage": {"promptTokens": 60, "completionTokens": 12, "totalTokens": 72},
    },
}

import os
out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "enclave-step-tier3.json")
with open(out, "w") as f:
    json.dump(spec, f, indent=1)
    f.write("\n")
print(f"wrote {out}: {len(chats)} chats, {len(calls)} calls, {len(streams)} streams")
