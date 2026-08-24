#!/usr/bin/env python3
"""A byte-faithful TCP tap that DUMPS the message array structurally.

Unlike harness/tools/wire-tap.py (which collapses `messages` to a count), this
one walks every message and prints its content shape, eliding only the base64
payload of any data: URL — which is exactly what a vision-wire check needs.

  python3 vision-tap.py [LISTEN] [UPHOST:UPPORT]   # default 11435 -> localhost:11434
"""
import json, socket, sys, threading
from datetime import datetime

LISTEN = int(sys.argv[1]) if len(sys.argv) > 1 else 11435
UP_HOST, _, _p = (sys.argv[2] if len(sys.argv) > 2 else "localhost:11434").partition(":")
UP_PORT = int(_p or 80)
lock = threading.Lock()

def elide(v):
    if isinstance(v, str):
        if v.startswith("data:"):
            head, _, tail = v.partition(",")
            return f"<{head},  {len(tail)} b64 chars, starts {tail[:24]!r}>"
        return v if len(v) <= 300 else v[:300] + f"…<+{len(v)-300} chars>"
    if isinstance(v, list): return [elide(x) for x in v]
    if isinstance(v, dict): return {k: elide(x) for k, x in v.items()}
    return v

def show(head, body):
    line = head.split(b"\r\n", 1)[0].decode("latin-1", "replace")
    with lock:
        print(f"\n=== {datetime.now():%H:%M:%S}  {line}")
        try: parsed = json.loads(body)
        except Exception:
            print(body.decode("utf-8", "replace")[:2000]); sys.stdout.flush(); return
        msgs = parsed.pop("messages", None)
        print("TOP-LEVEL KEYS:", list(parsed.keys()))
        print(json.dumps(elide(parsed), indent=2, ensure_ascii=False)[:4000])
        if isinstance(msgs, list):
            print(f"MESSAGES ({len(msgs)}):")
            for i, m in enumerate(msgs):
                c = m.get("content")
                kind = type(c).__name__
                if isinstance(c, list):
                    parts = [p.get("type") for p in c if isinstance(p, dict)]
                    print(f"  [{i}] role={m.get('role')} content=LIST parts={parts}")
                    print("      " + json.dumps(elide(c), ensure_ascii=False)[:1200])
                else:
                    s = c if isinstance(c, str) else json.dumps(c)
                    print(f"  [{i}] role={m.get('role')} content={kind} len={len(s or '')} :: {(s or '')[:110]!r}")
        sys.stdout.flush()

def read_head(sock):
    buf = b""
    while b"\r\n\r\n" not in buf:
        ch = sock.recv(65536)
        if not ch: return buf
        buf += ch
    return buf

def pump(src, dst):
    try:
        while True:
            d = src.recv(65536)
            if not d: break
            dst.sendall(d)
    except OSError: pass
    finally:
        try: dst.shutdown(socket.SHUT_WR)
        except OSError: pass

def handle(client):
    try:
        buf = read_head(client)
        if not buf: return
        head, _, rest = buf.partition(b"\r\n\r\n")
        length = 0
        for l in head.split(b"\r\n")[1:]:
            n, _, v = l.partition(b":")
            if n.strip().lower() == b"content-length": length = int(v.strip() or 0)
        body = rest
        while len(body) < length:
            ch = client.recv(65536)
            if not ch: break
            body += ch
        show(head, body[:length] if length else body)
        up = socket.create_connection((UP_HOST, UP_PORT))
        up.sendall(head + b"\r\n\r\n" + body)
        t = threading.Thread(target=pump, args=(up, client), daemon=True); t.start()
        pump(client, up); t.join(timeout=60)
        up.close()
    except Exception as e:
        print("tap error:", e); sys.stdout.flush()
    finally:
        try: client.close()
        except OSError: pass

srv = socket.socket(); srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
srv.bind(("127.0.0.1", LISTEN)); srv.listen(64)
print(f"vision-tap listening on 127.0.0.1:{LISTEN} -> {UP_HOST}:{UP_PORT}"); sys.stdout.flush()
while True:
    c, _ = srv.accept()
    threading.Thread(target=handle, args=(c,), daemon=True).start()
