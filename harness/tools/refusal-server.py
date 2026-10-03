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
  midframe       200 SSE whose FIRST frame is `data: {"error": {..., "code":
                 "content_filter"}}` (added 2026-09-30) -> P4.122's mid-stream
                 error frame before content: the SDK throws a CODED APIError,
                 so the Concierge reroutes on `provider-code`
  midframe-late  200 SSE: one content chunk, THEN the same coded error frame
                 -> P4.122's twin: no failover, the partial kept
  midframe-uncoded  200 SSE whose first frame is an error with a message and
                 NO code -> whatever v4 does (measured, not asserted)
  malformed      200 SSE whose FIRST frame is `data: {"choices": [` (not JSON;
                 added 2026-09-30, the smalls-round walk) -> P4.128's SDK
                 frame semantics: the stream FAILS with the SDK's SyntaxError
                 bytes instead of skipping the frame
  unauthorized   401 {"error": {"message": "Incorrect API key provided", ...}}
                 before any stream (added 2026-09-30) -> P4.128's pre-stream
                 `OpenAICompatible API error in streamMessage` catch line
  json           answers a small JSON object (added 2026-10-02) -> lets the
                 multi-step generators (the AI import's twelve steps, the
                 wizard) parse each answer and reach their later steps, so
                 every step's request is captured
  toolcall       with `tools` and no `tool` message yet: streams ONE native
                 tool call (`list_mail` if offered, else the first tool, args
                 {}); once a tool result is in the history, answers in text
                 -> P4.121's per-leg CHAT_MESSAGE rows, deterministically
  empty-choices  200 {"choices": []} on a non-streaming request (added
                 2026-10-02, P4.141); a stream gets an ordinary answer -> the
                 2xx shape guard: v4's `Cannot read properties of undefined
                 (reading 'message')` throw, the `OpenAICompatible API error in
                 sendMessage` catch line, and the cheap path's stand-in chain
                 engaging (trigger `provider-error`) instead of a blank result
  hang           accepts the request and NEVER answers (P4.141) -> with a short
                 budget, `Request timed out.` on the catch line and the
                 failover trigger `network` (a timeout); the streaming arm's
                 headers deadline likewise
  stall-body     200 headers with `Content-Length: 100`, then silence
                 (P4.141) -> the non-streaming body-read deadline: a `Timeout`
                 transport error, never an empty answer

A POST to .../responses (the OPENAI provider's Responses API) is answered in
that shape, non-streaming only -- enough for an OPENAI-provider describer
pointed here with a dummy key (the OPENAI_COMPATIBLE plugin cannot forward
images, so bug 116's arm needs the OPENAI provider).

Every request body is appended to $QT_REFUSE_CAPTURE (default
/tmp/refusal-server-requests.ndjson) so the wire bytes can be read back --
WITH the request's `Authorization` header (added 2026-10-01, P4.133), so a
capture line says which key a profile actually sent.

THE KEY GATE (P4.133, dogfood #133): with $QT_REFUSE_KEY set, every POST whose
`Authorization` is not exactly `Bearer $QT_REFUSE_KEY` answers the
`unauthorized` 401 BEFORE its mode runs (any model name). Bind one profile to
that key and another of the same provider to any other key, and only the bound
profile is answered -- the live proof that a turn sends its OWN profile's key
rather than the provider's first stored one. Unset (the default), no gate.

  /v1/models -> every mode's model name (so profile setup works)
"""
import json, os, sys, time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

PORT = int(sys.argv[1]) if len(sys.argv) > 1 else 8898
DEFAULT_MODE = os.environ.get("QT_REFUSE_MODE", "code")
CAPTURE = os.environ.get("QT_REFUSE_CAPTURE", "/tmp/refusal-server-requests.ndjson")
REQUIRED_KEY = os.environ.get("QT_REFUSE_KEY")
JSON_TEXT = json.dumps({"name": "Dogfood Lore", "title": "", "firstMessage": "Good evening.",
                        "pronouns": {"subject": "they", "object": "them", "possessive": "their"}})
MODES = {"refuse-code": "code", "refuse-finish": "finish", "tokenlimit": "tokenlimit",
         "notools": "notools", "blind": "blind", "echo": "echo",
         "midframe": "midframe", "midframe-late": "midframe-late",
         "midframe-uncoded": "midframe-uncoded", "toolcall": "toolcall",
         "malformed": "malformed", "unauthorized": "unauthorized", "json": "json",
         "empty-choices": "empty-choices", "hang": "hang", "stall-body": "stall-body"}

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
UNAUTH_BODY = {"error": {"message": "Incorrect API key provided: dogfood.",
                         "type": "invalid_request_error", "param": None,
                         "code": "invalid_api_key"}}
NOTOOLS_BODY = {"error": {"message": "This model does not support tools",
                          "type": "invalid_request_error", "param": "tools",
                          "code": None}}
BLIND_TEXT = ("**Overview.** The image shows a sunlit Victorian conservatory. "
              "A woman in a green dress stands beside a brass telescope, one hand "
              "on the eyepiece; behind her, ferns climb the ironwork and the glass "
              "panes throw long diagonal shadows across a tiled floor.")


# The mid-stream error frame's inner error (the openai SDK throws
# `APIError(undefined, data.error)` on it: status undefined, `code` read).
FRAME_ERROR = {"message": "The response was filtered due to the prompt triggering "
               "Azure OpenAI's content management policy.", "type": None,
               "param": "prompt", "code": "content_filter"}
FRAME_ERROR_UNCODED = {"message": "The upstream model host went away mid-answer."}


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

    def _sse_open(self):
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Cache-Control", "no-cache")
        self.send_header("Connection", "close")
        self.end_headers()

    def _sse(self, obj):
        self.wfile.write(b"data: " + json.dumps(obj).encode() + b"\n\n")
        self.wfile.flush()

    def _frame(self, req, error, before=None):
        """200, optionally one content chunk, then an error frame, then close."""
        model = req.get("model", "echo")
        self._sse_open()
        if before:
            self._sse({"id": "posed-1", "object": "chat.completion.chunk", "model": model,
                       "choices": [{"index": 0, "delta": {"role": "assistant",
                                                          "content": before},
                                    "finish_reason": None}]})
        self._sse({"error": error})
        self.close_connection = True

    def _toolcall(self, req):
        names = [t.get("function", {}).get("name") for t in req.get("tools") or []]
        name = "list_mail" if "list_mail" in names else (names[0] if names else "list_mail")
        model = req.get("model", "echo")
        self._sse_open()
        self._sse({"id": "posed-1", "object": "chat.completion.chunk", "model": model,
                   "choices": [{"index": 0, "delta": {"role": "assistant", "tool_calls": [
                       {"index": 0, "id": "call_posed_1", "type": "function",
                        "function": {"name": name, "arguments": "{}"}}]},
                       "finish_reason": None}]})
        self._sse({"id": "posed-1", "object": "chat.completion.chunk", "model": model,
                   "choices": [{"index": 0, "delta": {}, "finish_reason": "tool_calls"}]})
        self._sse({"id": "posed-1", "object": "chat.completion.chunk", "model": model,
                   "choices": [], "usage": {"prompt_tokens": 500, "completion_tokens": 5,
                                            "total_tokens": 505}})
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
        auth = self.headers.get("Authorization")
        key_ok = REQUIRED_KEY is None or auth == "Bearer " + REQUIRED_KEY
        with open(CAPTURE, "a") as f:
            f.write(json.dumps({"at": time.time(), "path": self.path, "mode": mode,
                                "authorization": auth, "keyGate": key_ok,
                                "body": req}) + "\n")
        sys.stderr.write("[refuse] POST %s model=%s mode=%s stream=%s messages=%d tools=%s auth=%s%s\n"
                         % (self.path, req.get("model"), mode, req.get("stream"),
                            len(msgs), "tools" in req, auth,
                            "" if key_ok else " (KEY GATE: 401)"))
        if not key_ok:
            self._json(401, UNAUTH_BODY)
        elif mode == "finish":
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
        elif mode == "midframe":
            self._frame(req, FRAME_ERROR)
        elif mode == "midframe-late":
            self._frame(req, FRAME_ERROR, before="The kettle had only just begun to ")
        elif mode == "midframe-uncoded":
            self._frame(req, FRAME_ERROR_UNCODED)
        elif mode == "malformed":
            self._sse_open()
            self.wfile.write(b'data: {"choices": [\n\n')
            self.wfile.write(b"data: [DONE]\n\n")
            self.wfile.flush()
            self.close_connection = True
        elif mode == "unauthorized":
            self._json(401, UNAUTH_BODY)
        elif mode == "toolcall":
            if "tools" in req and not any(m.get("role") == "tool" for m in msgs):
                self._toolcall(req)
            else:
                self._answer(req, "The posed tool ran; here is its answer, in plain text.")
        elif mode == "empty-choices":
            if req.get("stream"):
                self._answer(req, "ok")
            else:
                self._json(200, {"id": "posed-1", "object": "chat.completion",
                                 "model": req.get("model", "empty-choices"), "choices": []})
        elif mode == "hang":
            # Hold the socket open, silently, until the client gives up.
            while True:
                time.sleep(3600)
        elif mode == "stall-body":
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", "100")
            self.end_headers()
            self.wfile.flush()
            while True:
                time.sleep(3600)
        elif mode == "blind":
            self._answer(req, BLIND_TEXT, prompt_tokens=40)
        elif mode == "json":
            self._answer(req, JSON_TEXT)
        else:
            self._answer(req, "ok")


if __name__ == "__main__":
    sys.stderr.write("[refuse] listening on 127.0.0.1:%d default mode=%s capture=%s key gate=%s\n"
                     % (PORT, DEFAULT_MODE, CAPTURE,
                        "Bearer " + REQUIRED_KEY if REQUIRED_KEY else "off"))
    ThreadingHTTPServer(("127.0.0.1", PORT), H).serve_forever()
