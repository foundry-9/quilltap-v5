#!/usr/bin/env python3
"""An OpenAI-compatible stub that answers with a canned assistant message.

Used to drive the P4.D110 title-verdict parser's NEAR-MISS key path on a real
instance: the model "emits" `Suggested_Title` instead of `suggestedTitle`.
Handles both stream:true (SSE) and stream:false (plain JSON).
"""
import json, socket, sys, threading
from datetime import datetime

PORT = int(sys.argv[1]) if len(sys.argv) > 1 else 11437
CANNED = sys.argv[2] if len(sys.argv) > 2 else json.dumps(
    {"needsNewTitle": True, "reason": "the stub says so", "Suggested_Title": "A Misspelled Key Still Titles"}
)
lock = threading.Lock()

def reply(c, streaming):
    if streaming:
        payload = (
            'data: {"choices":[{"index":0,"delta":{"content":%s},"finish_reason":null}]}\n\n'
            'data: {"choices":[{"index":0,"delta":{},"finish_reason":"stop"}],'
            '"usage":{"prompt_tokens":10,"completion_tokens":20,"total_tokens":30}}\n\n'
            'data: [DONE]\n\n'
        ) % json.dumps(CANNED)
        ct = "text/event-stream"
    else:
        payload = json.dumps({
            "id": "stub", "object": "chat.completion",
            "choices": [{"index": 0, "message": {"role": "assistant", "content": CANNED}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 10, "completion_tokens": 20, "total_tokens": 30},
        })
        ct = "application/json"
    b = payload.encode()
    c.sendall(f"HTTP/1.1 200 OK\r\nContent-Type: {ct}\r\nContent-Length: {len(b)}\r\nConnection: close\r\n\r\n".encode() + b)

def handle(c):
    try:
        buf = b""
        while b"\r\n\r\n" not in buf:
            ch = c.recv(65536)
            if not ch: return
            buf += ch
        head, _, rest = buf.partition(b"\r\n\r\n")
        length = 0
        for l in head.split(b"\r\n")[1:]:
            n, _, v = l.partition(b":")
            if n.strip().lower() == b"content-length": length = int(v.strip() or 0)
        body = rest
        while len(body) < length:
            ch = c.recv(65536)
            if not ch: break
            body += ch
        streaming = b'"stream":true' in body.replace(b" ", b"")
        with lock:
            print(f"title-stub {datetime.now():%H:%M:%S.%f} stream={streaming} len={len(body)}", flush=True)
        reply(c, streaming)
    except Exception as e:
        print("stub error:", e, flush=True)
    finally:
        try: c.close()
        except OSError: pass

s = socket.socket(); s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
s.bind(("127.0.0.1", PORT)); s.listen(32)
print(f"title-stub on 127.0.0.1:{PORT}", flush=True)
print("CANNED:", CANNED, flush=True)
while True:
    c, _ = s.accept(); threading.Thread(target=handle, args=(c,), daemon=True).start()
