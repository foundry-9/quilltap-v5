#!/usr/bin/env python3
"""An OpenAI-compatible stub that answers every /chat/completions with an EMPTY
stream whose final chunk carries finish_reason: "content_filter".

Used to drive the bug-93 moderation-refusal sentence end to end on a real
instance WITHOUT sending anything a real provider would have to refuse.
"""
import socket, threading, sys

PORT = int(sys.argv[1]) if len(sys.argv) > 1 else 11436
REASON = sys.argv[2] if len(sys.argv) > 2 else "content_filter"

BODY = (
    'data: {"id":"stub","object":"chat.completion.chunk","choices":[{"index":0,'
    '"delta":{},"finish_reason":"%s"}],"usage":{"prompt_tokens":10,'
    '"completion_tokens":0,"total_tokens":10}}\n\n'
    'data: [DONE]\n\n'
) % REASON

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
        print(f"stub: served at {__import__('datetime').datetime.now():%H:%M:%S.%f}", flush=True)
        payload = BODY.encode()
        c.sendall(
            b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n"
            + f"Content-Length: {len(payload)}\r\n".encode()
            + b"Connection: close\r\n\r\n" + payload
        )
    except Exception as e:
        print("stub error:", e, flush=True)
    finally:
        try: c.close()
        except OSError: pass

s = socket.socket(); s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
s.bind(("127.0.0.1", PORT)); s.listen(32)
print(f"refusal-stub on 127.0.0.1:{PORT} finish_reason={REASON}", flush=True)
while True:
    c, _ = s.accept(); threading.Thread(target=handle, args=(c,), daemon=True).start()
