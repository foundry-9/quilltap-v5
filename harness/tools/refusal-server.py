#!/usr/bin/env python3
"""A deliberately refusing OpenAI-compatible endpoint (the Concierge instrument).

Written for the 2026-09-29 dogfood walk. A real provider refusal cannot be
summoned on demand: the refusal classifier reads only the provider's ERROR
(a moderation code, a moderation stop reason, or policy wording in the error
text) -- never the model's prose -- and today's models mostly just answer.
This endpoint answers every chat completion the way Azure OpenAI's content
filter does, so the whole Concierge path (classify -> ledger -> reroute to the
uncensored desk -> auto-switch after N) runs on a live instance against a
posed primary while the desk it reroutes to stays real.

Recipe: run it, create an OPENAI_COMPATIBLE connection profile with Base URL
http://127.0.0.1:8898/v1 (any model name), seat it in a MODERATED chat, send.

  QT_REFUSE_MODE=code    (default) 400 {"error": {"code": "content_filter", ...}}
                         -> evidence `provider-code`
  QT_REFUSE_MODE=finish  200 SSE stream with no content and
                         finish_reason "content_filter" -> evidence `finish-reason`

  /v1/models -> a normal model list (so profile setup works)
"""
import json, os, sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

PORT = int(sys.argv[1]) if len(sys.argv) > 1 else 8898
MODE = os.environ.get("QT_REFUSE_MODE", "code")
MODEL = "refusal-model"

# Azure OpenAI's content-filter rejection, as the openai SDK surfaces it.
CODE_BODY = {
    "error": {
        "message": "The response was filtered due to the prompt triggering Azure "
        "OpenAI's content management policy. Please modify your prompt and retry.",
        "type": None,
        "param": "prompt",
        "code": "content_filter",
        "status": 400,
    }
}


class H(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, fmt, *a):
        sys.stderr.write("[refuse] %s %s\n" % (self.address_string(), fmt % a))
        sys.stderr.flush()

    def _json(self, status, obj):
        body = json.dumps(obj).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        if self.path.rstrip("/").endswith("/models"):
            self._json(200, {"object": "list", "data": [
                {"id": MODEL, "object": "model", "owned_by": "dogfood"}]})
        else:
            self._json(404, {"error": {"message": "not found"}})

    def do_POST(self):
        length = int(self.headers.get("Content-Length") or 0)
        raw = self.rfile.read(length) if length else b""
        try:
            req = json.loads(raw or b"{}")
        except ValueError:
            req = {}
        sys.stderr.write("[refuse] POST %s stream=%s mode=%s\n"
                         % (self.path, req.get("stream"), MODE))
        if MODE == "finish":
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Cache-Control", "no-cache")
            self.send_header("Connection", "close")
            self.end_headers()
            chunk = {"id": "refuse-1", "object": "chat.completion.chunk",
                     "model": req.get("model", MODEL),
                     "choices": [{"index": 0, "delta": {},
                                  "finish_reason": "content_filter"}]}
            self.wfile.write(b"data: " + json.dumps(chunk).encode() + b"\n\n")
            self.wfile.write(b"data: [DONE]\n\n")
            self.wfile.flush()
            self.close_connection = True
        else:
            self._json(400, CODE_BODY)


if __name__ == "__main__":
    sys.stderr.write("[refuse] listening on 127.0.0.1:%d mode=%s\n" % (PORT, MODE))
    ThreadingHTTPServer(("127.0.0.1", PORT), H).serve_forever()
