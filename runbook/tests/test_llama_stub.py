"""The stand-in llama-server answers the way llama.cpp's server does, for the two endpoints the app uses."""
import json
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from runbook.llama_stub import LlamaStub, source_of  # noqa: E402

PROMPT = ("<bos><start_of_turn>user\nYou are a professional translator.\n"
          "Please translate the following Japanese text into Korean:\nこんにちは<end_of_turn>\n<start_of_turn>model\n")


def get(url):
    try:
        with urllib.request.urlopen(url, timeout=5) as r:
            return r.status, json.loads(r.read())
    except urllib.error.HTTPError as e:
        return e.code, json.loads(e.read())


def post(url, body):
    request = urllib.request.Request(url, data=json.dumps(body).encode(), headers={"Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(request, timeout=5) as r:
            return r.status, json.loads(r.read())
    except urllib.error.HTTPError as e:
        return e.code, json.loads(e.read())


def test_the_source_line_is_what_follows_the_instruction():
    assert source_of(PROMPT) == "こんにちは"
    assert source_of("no instruction here") == ""


def test_health_is_503_while_loading_then_200():
    with LlamaStub(loading_seconds=0.6) as stub:
        time.sleep(0.8)  # loading begins at the first /health request, not when the stub starts
        status, body = get(stub.url + "/health")
        assert status == 503 and body["error"]["message"] == "Loading model" and body["error"]["code"] == 503
        assert stub.health_503 == 1
        time.sleep(0.8)
        assert get(stub.url + "/health") == (200, {"status": "ok"})
        stub.health_mode = "down"
        assert get(stub.url + "/health")[0] == 503


def test_completion_answers_with_content_and_keeps_the_request():
    with LlamaStub() as stub:
        stub.translations["こんにちは"] = "안녕하세요"
        status, body = post(stub.url + "/completion", {"prompt": PROMPT, "n_predict": 64, "stream": False})
        assert status == 200 and body["content"].strip() == "안녕하세요" and body["stop"] is True
        assert body["tokens_predicted"] >= 1 and "timings" in body
        assert stub.requests[0]["prompt"] == PROMPT and stub.requests[0]["n_predict"] == 64
        status, body = post(stub.url + "/completion", {"prompt": PROMPT.replace("こんにちは", "さようなら")})
        assert body["content"].strip() == "[KO] さようなら"


def test_failure_modes_have_llama_servers_status_codes():
    with LlamaStub() as stub:
        stub.completion_mode = "error"
        status, body = post(stub.url + "/completion", {"prompt": PROMPT})
        assert (status, body["error"]["code"]) == (500, 500)
        stub.completion_mode = "slot"
        assert post(stub.url + "/completion", {"prompt": PROMPT})[0] == 503
        stub.completion_mode = "empty"
        status, body = post(stub.url + "/completion", {"prompt": PROMPT})
        assert status == 200 and body["content"].strip() == ""
        assert post(stub.url + "/v1/chat/completions", {})[0] == 404
        assert get(stub.url + "/props")[0] == 404


def test_the_apps_own_client_works_against_the_stand_in():
    """The real `resonance_llama` client (health + translate_text), not a re-implementation of it."""
    import subprocess

    root = Path(__file__).resolve().parents[2]

    def ask(url, text):
        done = subprocess.run(["cargo", "run", "-q", "-p", "resonance-llama", "--example", "translate_once", "--", url, text],
                              cwd=root, capture_output=True, text=True, encoding="utf-8", timeout=600)
        return done.returncode, done.stdout.splitlines()

    with LlamaStub(loading_seconds=0) as stub:
        stub.translations["こんにちは"] = "안녕하세요"
        assert ask(stub.url, "こんにちは") == (0, ["ready", "안녕하세요"])
        stub.health_mode = "down"
        stub.completion_mode = "error"
        code, lines = ask(stub.url, "こんにちは")
        assert code == 1 and lines[0] == "not ready" and lines[1].startswith("error:")
        stub.completion_mode = "empty"
        code, lines = ask(stub.url, "こんにちは")
        assert code == 1 and "empty" in lines[1]
        stub.completion_mode = "close"
        code, lines = ask(stub.url, "こんにちは")
        assert code == 1 and lines[1].startswith("error:")
