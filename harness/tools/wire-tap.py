#!/usr/bin/env python3
"""A byte-faithful TCP tap: forwards to an upstream and prints request bodies.

  python3 wire-tap.py                     # listen 11435 -> localhost:11434 (ollama)
  python3 wire-tap.py 8081 127.0.0.1:1234 # listen 8081  -> llama-server

Point a connection profile's Base URL at the LISTEN port. Every request body is
pretty-printed; the bytes themselves are relayed untouched, so streaming and
chunked responses behave exactly as they would without the tap.
"""

import json
import socket
import sys
import threading
from datetime import datetime

LISTEN = int(sys.argv[1]) if len(sys.argv) > 1 else 11435
UP_HOST, _, _port = (sys.argv[2] if len(sys.argv) > 2 else "localhost:11434").partition(":")
UP_PORT = int(_port or 80)

_print_lock = threading.Lock()


def show(head: bytes, body: bytes) -> None:
    line = head.split(b"\r\n", 1)[0].decode("latin-1", "replace")
    with _print_lock:
        print(f"\n\033[1;36m=== {datetime.now():%H:%M:%S}  {line}\033[0m")
        if not body:
            print("(no body)")
            return
        try:
            parsed = json.loads(body)
        except Exception:
            print(body.decode("utf-8", "replace")[:4000])
            return
        # Messages are long and rarely the thing under test: summarize them and
        # show every other key in full, which is where the parameters live.
        msgs = parsed.pop("messages", None)
        if isinstance(msgs, list):
            parsed["messages"] = f"<{len(msgs)} messages, {sum(len(json.dumps(m)) for m in msgs)} bytes>"
        print(json.dumps(parsed, indent=2, ensure_ascii=False)[:8000])
        sys.stdout.flush()


def read_head(sock: socket.socket) -> bytes:
    buf = b""
    while b"\r\n\r\n" not in buf:
        chunk = sock.recv(65536)
        if not chunk:
            return buf
        buf += chunk
    return buf


def pump(src: socket.socket, dst: socket.socket) -> None:
    try:
        while True:
            data = src.recv(65536)
            if not data:
                break
            dst.sendall(data)
    except OSError:
        pass
    finally:
        try:
            dst.shutdown(socket.SHUT_WR)
        except OSError:
            pass


def handle(client: socket.socket) -> None:
    try:
        buf = read_head(client)
        if not buf:
            return
        head, _, rest = buf.partition(b"\r\n\r\n")
        length = 0
        for line in head.split(b"\r\n")[1:]:
            name, _, value = line.partition(b":")
            if name.strip().lower() == b"content-length":
                length = int(value.strip() or 0)
        body = rest
        while len(body) < length:
            chunk = client.recv(65536)
            if not chunk:
                break
            body += chunk
        show(head, body[:length] if length else body)

        upstream = socket.create_connection((UP_HOST, UP_PORT))
        upstream.sendall(head + b"\r\n\r\n" + body)
        t = threading.Thread(target=pump, args=(client, upstream), daemon=True)
        t.start()
        pump(upstream, client)
    except Exception as exc:  # a tap must never take the app down with it
        with _print_lock:
            print(f"[tap] {exc!r}", file=sys.stderr)
    finally:
        client.close()


def main() -> None:
    server = socket.socket()
    server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    server.bind(("127.0.0.1", LISTEN))
    server.listen(64)
    print(f"tap: http://localhost:{LISTEN}  ->  {UP_HOST}:{UP_PORT}   (Ctrl-C to stop)")
    while True:
        client, _ = server.accept()
        threading.Thread(target=handle, args=(client,), daemon=True).start()


if __name__ == "__main__":
    main()
