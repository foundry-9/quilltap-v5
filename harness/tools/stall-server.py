#!/usr/bin/env python3
"""A deliberately stalling OpenAI-compatible endpoint (the stream-watchdog instrument).

Written for the 2026-09-15 dogfood walk, which used it to prove v4 bug 141 /
P4.D189-D190 on real data: the greeting abandoned at elapsed_ms 90002 against a
90,000 ms budget, the Salon at 240001 against 240,000, and the stalled error
classified as `network` so the understudy answered instead of the turn wedging.
No test can pose this — every canned stream on both sides yields and CLOSES.

Recipe: run it, point an OPENAI_COMPATIBLE connection profile at
http://127.0.0.1:8899/v1 with a working `fallbackProfileId`, seat it, and send.
Add ?chunks=2 to the model path for the IDLE arm (120 s after 2 chunks).

Answers 200 with SSE headers and then holds the socket open, sending either
nothing at all (the first-chunk arm) or N content chunks and then silence
(the idle arm).  Never closes.  Used to prove the v5 stream watchdog fires.

  /v1/chat/completions          -> 200 + headers, then silence forever
  /v1/chat/completions?chunks=2 -> 200 + headers, 2 content chunks, then silence
  /v1/models                    -> a normal model list (so profile setup works)
"""
import json, os, socket, sys, threading, time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlparse, parse_qs

PORT = int(sys.argv[1]) if len(sys.argv) > 1 else 8899
# Default chunk count for the IDLE arm. A connection profile's baseUrl cannot
# carry a query string through the SDK's URL join, so the env var is the only
# way to reach the idle budget from a real seat: QT_STALL_CHUNKS=2.
DEFAULT_CHUNKS = int(os.environ.get("QT_STALL_CHUNKS", "0"))
MODEL = "stall-model"
HELD = []          # keep references so sockets are never GC'd/closed

class H(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, fmt, *a):
        sys.stderr.write("[stall] %s %s\n" % (self.address_string(), fmt % a))
        sys.stderr.flush()

    def do_GET(self):
        p = urlparse(self.path).path
        if p.rstrip("/").endswith("/models"):
            body = json.dumps({"object": "list", "data": [
                {"id": MODEL, "object": "model", "owned_by": "stall"}]}).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body); self.wfile.flush()
            return
        self.send_error(404)

    def do_POST(self):
        q = parse_qs(urlparse(self.path).query)
        n = int(q.get("chunks", [str(DEFAULT_CHUNKS)])[0])
        ln = int(self.headers.get("Content-Length") or 0)
        raw = self.rfile.read(ln) if ln else b""
        try:
            stream = json.loads(raw or b"{}").get("stream", False)
        except Exception:
            stream = False
        sys.stderr.write("[stall] POST %s stream=%s chunks=%d bytes=%d\n"
                         % (self.path, stream, n, ln)); sys.stderr.flush()

        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream" if stream else "application/json")
        self.send_header("Cache-Control", "no-cache")
        self.send_header("Connection", "keep-alive")
        self.end_headers()
        self.wfile.flush()

        for i in range(n):
            frame = {"id": "chatcmpl-stall", "object": "chat.completion.chunk",
                     "created": int(time.time()), "model": MODEL,
                     "choices": [{"index": 0, "delta": {"content": "tick "},
                                  "finish_reason": None}]}
            try:
                self.wfile.write(b"data: " + json.dumps(frame).encode() + b"\n\n")
                self.wfile.flush()
            except Exception as e:
                sys.stderr.write("[stall] write failed: %r\n" % (e,)); return
            time.sleep(0.4)

        sys.stderr.write("[stall] now holding the socket open, sending nothing\n")
        sys.stderr.flush()
        HELD.append(self.connection)
        # Hold forever.  Do not close, do not send.
        while True:
            time.sleep(3600)

if __name__ == "__main__":
    srv = ThreadingHTTPServer(("127.0.0.1", PORT), H)
    srv.daemon_threads = True
    sys.stderr.write("[stall] listening on 127.0.0.1:%d\n" % PORT); sys.stderr.flush()
    srv.serve_forever()
