#!/usr/bin/env python3
"""A posed OpenAI-compatible provider (the dogfood failure instrument).

Written for the 2026-09-29 dogfood walk. Some v5 paths only run when a
provider fails in one specific way, and no real provider fails that way on
demand: the refusal classifier reads only a provider ERROR (never prose), the
request-limit recovery needs a token-limit rejection, the tool-unsupported
retry needs a "does not support tools" rejection, and bug 116's describer
verdict needs a gateway that bills for no image yet answers confidently. This
endpoint poses each one, so the real v5 path runs on a live instance while
every other desk stays real.

Recipe: run it, create an OPENAI_COMPATIBLE connection profile with Base URL
http://127.0.0.1:8898/v1 and pick the MODEL NAME for the failure you want (one
server serves every mode at once; `QT_REFUSE_MODE` sets the mode for any other
model name, default `code`):

  refuse-code    400 {"error": {"code": "content_filter", ...}} (Azure's shape)
                 -> the Concierge's `provider-code` evidence
  refuse-finish  200 SSE with no content and finish_reason "content_filter"
                 -> the `finish-reason` evidence
  tokenlimit     400 "This model's maximum context length is 8192 tokens.
                 However, your messages resulted in 12000 tokens..." on the
                 turn; ANSWERS a bare system+user request (the recovery's own
                 call) -> P4.99's recovery INFO line + the recovery message
  notools        400 "This model does not support tools" whenever the request
                 carries `tools`; answers when it does not -> P4.97's retry
  blind          answers confidently while reporting prompt_tokens: 40 (below
                 the describer's instruction-token ceiling) -> bug 116's
                 positive arm (the invented description is discarded)
  echo           answers "ok" -> a plain working desk

A POST to .../responses (the OPENAI provider's Responses API) is answered in
that shape, non-streaming only -- enough for an OPENAI-provider describer
pointed here with a dummy key (the OPENAI_COMPATIBLE plugin cannot forward
images, so bug 116's arm needs the OPENAI provider).

Every request body is appended to $QT_REFUSE_CAPTURE (default
/tmp/refusal-server-requests.ndjson) so the wire bytes can be read back.

  /v1/models -> every mode's model name (so profile setup works)
"""
import json, os, sys, time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

PORT = int(sys.argv[1]) if len(sys.argv) > 1 else 8898
DEFAULT_MODE = os.environ.get("QT_REFUSE_MODE", "code")
CAPTURE = os.environ.get("QT_REFUSE_CAPTURE", "/tmp/refusal-server-requests.ndjson")
MODES = {"refuse-code": "code", "refuse-finish": "finish", "tokenlimit": "tokenlimit",
         "notools": "notools", "blind": "blind", "echo": "echo"}

# Azure OpenAI's content-filter rejection, as the openai SDK surfaces it.
CODE_BODY = {"error": {
    "message": "The response was filtered due to the prompt triggering Azure "
    "OpenAI's content management policy. Please modify your prompt and retry.",
    "type": None, "param": "prompt", "code": "content_filter", "status": 400}}
# OpenAI's context-length rejection (the message carries both numbers).
TOKEN_BODY = {"error": {
    "message": "This model's maximum context length is 8192 tokens. However, your "
    "messages resulted in 12000 tokens. Please reduce the length of the messages.",
    "type": "invalid_request_error", "param": "messages",
    "code": "context_length_exceeded"}}
NOTOOLS_BODY = {"error": {"message": "This model does not support tools",
                          "type": "invalid_request_error", "param": "tools",
                          "code": None}}
BLIND_TEXT = ("**Overview.** The image shows a sunlit Victorian conservatory. "
              "A woman in a green dress stands beside a brass telescope, one hand "
              "on the eyepiece; behind her, ferns climb the ironwork and the glass "
              "panes throw long diagonal shadows across a tiled floor.")


def mode_for(model):
    return MODES.get(model or "", DEFAULT_MODE)


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

    def _answer(self, req, text, prompt_tokens=None, finish="stop"):
        model = req.get("model", "echo")
        if self.path.rstrip("/").endswith("/responses"):
            # The OpenAI Responses API (the OPENAI provider's path) --
            # non-streaming only, which is what the describer sends.
            inp = prompt_tokens if prompt_tokens is not None else 500
            out = max(1, len(text) // 4)
            self._json(200, {"id": "resp_posed", "object": "response", "status": "completed",
                             "model": model, "output": [{
                                 "type": "message", "id": "msg_posed", "role": "assistant",
                                 "status": "completed",
                                 "content": [{"type": "output_text", "text": text,
                                              "annotations": []}]}],
                             "usage": {"input_tokens": inp, "output_tokens": out,
                                       "total_tokens": inp + out}})
            return
        usage = {"prompt_tokens": prompt_tokens if prompt_tokens is not None else 500,
                 "completion_tokens": max(1, len(text) // 4)}
        usage["total_tokens"] = usage["prompt_tokens"] + usage["completion_tokens"]
        if not req.get("stream"):
            self._json(200, {"id": "posed-1", "object": "chat.completion",
                             "created": int(time.time()), "model": model,
                             "choices": [{"index": 0, "finish_reason": finish,
                                          "message": {"role": "assistant", "content": text}}],
                             "usage": usage})
            return
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Cache-Control", "no-cache")
        self.send_header("Connection", "close")
        self.end_headers()
        chunks = []
        if text:
            chunks.append({"delta": {"role": "assistant", "content": text}, "finish_reason": None})
        chunks.append({"delta": {}, "finish_reason": finish})
        for c in chunks:
            obj = {"id": "posed-1", "object": "chat.completion.chunk", "model": model,
                   "choices": [dict(index=0, **c)]}
            self.wfile.write(b"data: " + json.dumps(obj).encode() + b"\n\n")
        tail = {"id": "posed-1", "object": "chat.completion.chunk", "model": model,
                "choices": [], "usage": usage}
        self.wfile.write(b"data: " + json.dumps(tail).encode() + b"\n\n")
        self.wfile.write(b"data: [DONE]\n\n")
        self.wfile.flush()
        self.close_connection = True

    def do_GET(self):
        if self.path.rstrip("/").endswith("/models"):
            self._json(200, {"object": "list", "data": [
                {"id": m, "object": "model", "owned_by": "dogfood"} for m in MODES]})
        else:
            self._json(404, {"error": {"message": "not found"}})

    def do_POST(self):
        length = int(self.headers.get("Content-Length") or 0)
        raw = self.rfile.read(length) if length else b""
        try:
            req = json.loads(raw or b"{}")
        except ValueError:
            req = {}
        mode = mode_for(req.get("model"))
        msgs = req.get("messages") or []
        with open(CAPTURE, "a") as f:
            f.write(json.dumps({"at": time.time(), "path": self.path, "mode": mode,
                                "body": req}) + "\n")
        sys.stderr.write("[refuse] POST %s model=%s mode=%s stream=%s messages=%d tools=%s\n"
                         % (self.path, req.get("model"), mode, req.get("stream"),
                            len(msgs), "tools" in req))
        if mode == "finish":
            self._answer(req, "", finish="content_filter")
        elif mode == "code":
            self._json(400, CODE_BODY)
        elif mode == "tokenlimit":
            # The recovery's own call is exactly [system, user] with no tools.
            if len(msgs) == 2 and "tools" not in req:
                self._answer(req, "Terribly sorry -- that letter ran long; do try a shorter one.")
            else:
                self._json(400, TOKEN_BODY)
        elif mode == "notools":
            if "tools" in req:
                self._json(400, NOTOOLS_BODY)
            else:
                self._answer(req, "Answered without any tools, as requested.")
        elif mode == "blind":
            self._answer(req, BLIND_TEXT, prompt_tokens=40)
        else:
            self._answer(req, "ok")


if __name__ == "__main__":
    sys.stderr.write("[refuse] listening on 127.0.0.1:%d default mode=%s capture=%s\n"
                     % (PORT, DEFAULT_MODE, CAPTURE))
    ThreadingHTTPServer(("127.0.0.1", PORT), H).serve_forever()
