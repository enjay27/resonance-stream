"""A stand-in for llama-server: the two endpoints the app uses, with the replies llama.cpp's server gives, and nothing behind them -- no model, no GPU.

`GET /health` answers 200 `{"status":"ok"}` once "loaded" and 503 `{"error":{"code":503,"message":"Loading model",...}}` before that, as llama-server does
while it loads a model. `POST /completion` (the native endpoint, not the OpenAI one) answers `{"content": ..., "stop": true, "tokens_predicted": ...,
"timings": ...}` for a request with a `prompt`; anything else is the 404 llama-server gives. Every request body is kept (`requests`), so a test can check
what the app actually sent. Behaviour, changeable while it runs:

    health_mode      ok | loading (503 for `loading_seconds` after the FIRST /health request -- the app's first poll is when a real
                     server would have begun loading -- then ok) | down (503 always)
    completion_mode  ok | error (500) | empty (a reply with blank content) | slot (503 "no slot available") | close (the connection closes with no reply)
    translations     {japanese text: korean text}; any other line comes back as "[KO] " + the line
"""
from __future__ import annotations

import http.server
import json
import threading
import time

SOURCE_MARK = "Please translate the following Japanese text into Korean:\n"
TURN_END = "<end_of_turn>"


def source_of(prompt: str) -> str:
    """The chat line a prompt asks to translate: what follows the instruction, up to the end of the user turn."""
    start = prompt.find(SOURCE_MARK)
    if start < 0:
        return ""
    text = prompt[start + len(SOURCE_MARK):]
    return text.split(TURN_END, 1)[0]


def error_body(code: int, message: str, kind: str) -> bytes:
    return json.dumps({"error": {"code": code, "message": message, "type": kind}}).encode()


class _Handler(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *args) -> None:
        pass

    def _reply(self, status: int, body: bytes) -> None:
        self.send_response(status)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Server", "llama.cpp")
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self) -> None:  # noqa: N802
        owner: LlamaStub = self.server.owner  # type: ignore[attr-defined]
        owner.hits.append(f"GET {self.path}")
        if self.path == "/health":
            owner.first_health()
            if owner.is_ready():
                return self._reply(200, json.dumps({"status": "ok"}).encode())
            owner.health_503 += 1
            return self._reply(503, error_body(503, "Loading model", "unavailable_error"))
        self._reply(404, error_body(404, "File Not Found", "not_found_error"))

    def do_POST(self) -> None:  # noqa: N802
        owner: LlamaStub = self.server.owner  # type: ignore[attr-defined]
        owner.hits.append(f"POST {self.path}")
        length = int(self.headers.get("Content-Length", "0"))
        raw = self.rfile.read(length)
        if self.path != "/completion":
            return self._reply(404, error_body(404, "File Not Found", "not_found_error"))
        try:
            body = json.loads(raw)
        except ValueError:
            return self._reply(400, error_body(400, "Invalid JSON", "invalid_request_error"))
        owner.requests.append(body)
        mode = owner.completion_mode
        if mode == "close":
            self.close_connection = True
            return
        if mode == "error":
            return self._reply(500, error_body(500, "Internal Server Error", "server_error"))
        if mode == "slot":
            return self._reply(503, error_body(503, "no slot available", "unavailable_error"))
        prompt = str(body.get("prompt", ""))
        source = source_of(prompt)
        content = "  " if mode == "empty" else " " + owner.translations.get(source, "[KO] " + source) + " "
        self._reply(200, json.dumps({
            "index": 0, "content": content, "tokens": [], "id_slot": 0, "stop": True, "model": "stand-in",
            "tokens_predicted": max(1, len(content) // 2), "tokens_evaluated": max(1, len(prompt) // 4),
            "generation_settings": {"n_predict": body.get("n_predict"), "temperature": body.get("temperature")},
            "prompt": prompt, "has_new_line": False, "truncated": False, "stop_type": "eos", "stopping_word": "",
            "tokens_cached": 0, "timings": {"prompt_n": 1, "predicted_n": 1, "predicted_per_second": 100.0},
        }, ensure_ascii=False).encode("utf-8"))


class LlamaStub:
    """`with LlamaStub() as stub:` ... `stub.url` for `--llama-url`."""

    def __init__(self, loading_seconds: float = 0.0) -> None:
        self.health_mode = "loading" if loading_seconds > 0 else "ok"
        self.loading_seconds = loading_seconds
        self.completion_mode = "ok"
        self.translations: dict[str, str] = {}
        self.requests: list[dict] = []
        self.hits: list[str] = []
        self.health_503 = 0  # how many /health requests were answered 503
        self._started: float | None = None
        self._httpd: http.server.ThreadingHTTPServer | None = None
        self._thread: threading.Thread | None = None

    @property
    def url(self) -> str:
        assert self._httpd is not None, "stub not started"
        return f"http://127.0.0.1:{self._httpd.server_address[1]}"

    def first_health(self) -> None:
        if self._started is None:
            self._started = time.monotonic()

    def is_ready(self) -> bool:
        if self.health_mode == "down":
            return False
        if self.health_mode == "loading":
            return self._started is not None and time.monotonic() - self._started >= self.loading_seconds
        return True

    def start(self) -> "LlamaStub":
        httpd = http.server.ThreadingHTTPServer(("127.0.0.1", 0), _Handler)
        httpd.daemon_threads = True
        httpd.owner = self  # type: ignore[attr-defined]
        self._httpd = httpd
        self._thread = threading.Thread(target=httpd.serve_forever, daemon=True)
        self._thread.start()
        return self

    def stop(self) -> None:
        if self._httpd is not None:
            self._httpd.shutdown()
            self._httpd.server_close()
        if self._thread is not None:
            self._thread.join(timeout=5)

    def __enter__(self) -> "LlamaStub":
        return self.start()

    def __exit__(self, *exc) -> None:
        self.stop()
