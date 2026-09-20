"""Freeze the four VLM providers' request/response shapes as JSON the Rust
port must match.

    C:/Users/kapsikkum/.trackaction/venv/Scripts/python.exe tools/gen_vlm_fixtures.py

No network call is made: httpx.Client is mocked exactly as
tests/test_vlm_providers.py does, so this only exercises the pure request
building in conrod/vlm_providers.py and the pure response parsing next to
it. Writes rust/fixtures/vlm.json; rust/crates/conrod-io/tests/vlm.rs checks
the Rust port against it.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path
from unittest.mock import MagicMock

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT))

from conrod import vlm_providers  # noqa: E402
from conrod.config import Settings  # noqa: E402

OUT = ROOT / "rust" / "fixtures" / "vlm.json"

SCHEMA = {"type": "object", "properties": {"make": {"type": ["string", "null"]}},
         "required": ["make"]}


def _client(response_json, status=200):
    client = MagicMock()
    resp = MagicMock()
    resp.status_code = status
    resp.json.return_value = response_json
    resp.raise_for_status.side_effect = (
        None if status < 400 else RuntimeError(f"status {status}"))
    client.post.return_value = resp
    return client


def _captured(client) -> dict:
    """What the provider handed to client.post, in the shape Rust compares."""
    args, kwargs = client.post.call_args
    out = {"url": args[0], "json": kwargs.get("json")}
    headers = kwargs.get("headers")
    out["headers"] = dict(headers) if headers else {}
    params = kwargs.get("params")
    out["query"] = dict(params) if params else {}
    return out


def request_cases() -> list[dict]:
    cases = []

    settings = Settings(vlm_provider="ollama", vlm_model="qwen2.5vl:7b",
                        vlm_host="http://127.0.0.1:11434")
    client = _client({"response": json.dumps({"make": "Mini"})})
    vlm_providers.call(settings, prompt="describe it", images=["b64img"],
                       schema=SCHEMA, num_predict=500, client=client)
    cases.append({"provider": "ollama", "request": _captured(client)})

    settings = Settings(vlm_provider="openai", vlm_model="gpt-4o", vlm_api_key="sk-test-123")
    client = _client({"choices": [{"message": {"content": "{}"}}]})
    vlm_providers.call(settings, prompt="describe it", images=["b64img"],
                       schema=SCHEMA, num_predict=500, client=client)
    cases.append({"provider": "openai", "request": _captured(client)})

    settings = Settings(vlm_provider="anthropic", vlm_model="claude-sonnet-5",
                        vlm_api_key="sk-ant-test")
    client = _client({"content": [{"type": "tool_use", "input": {}}]})
    vlm_providers.call(settings, prompt="describe it", images=["b64img"],
                       schema=SCHEMA, num_predict=500, client=client)
    cases.append({"provider": "anthropic", "request": _captured(client)})

    # A Claude Code OAuth token instead of a console key: Bearer, not x-api-key.
    settings = Settings(vlm_provider="anthropic", vlm_model="claude-sonnet-5",
                        vlm_api_key="sk-ant-oat01-test")
    client = _client({"content": [{"type": "tool_use", "input": {}}]})
    vlm_providers.call(settings, prompt="describe it", images=["b64img"],
                       schema=SCHEMA, num_predict=500, client=client)
    cases.append({"provider": "anthropic-claude-code", "request": _captured(client)})

    settings = Settings(vlm_provider="gemini", vlm_model="gemini-2.0-flash",
                        vlm_api_key="AIzaTest")
    client = _client({"candidates": [{"content": {"parts": [{"text": "{}"}]}}]})
    vlm_providers.call(settings, prompt="describe it", images=["b64img"],
                       schema=SCHEMA, num_predict=500, client=client)
    cases.append({"provider": "gemini", "request": _captured(client)})

    return cases


_SETTINGS_FOR = {
    "ollama": Settings(vlm_provider="ollama"),
    "openai": Settings(vlm_provider="openai", vlm_api_key="k"),
    "anthropic": Settings(vlm_provider="anthropic", vlm_api_key="k"),
    "gemini": Settings(vlm_provider="gemini", vlm_api_key="k"),
}


def _parse(provider: str, body: dict) -> dict:
    """Run the body through the real provider function and report what
    vlm_providers.call() actually did with it -- not a hand-computed guess."""
    client = _client(body)
    try:
        result = vlm_providers.call(_SETTINGS_FOR[provider], prompt="p", images=[],
                                    schema=SCHEMA, num_predict=1, client=client)
        return {"provider": provider, "body": body, "ok": True, "result": result}
    except Exception:
        return {"provider": provider, "body": body, "ok": False, "result": None}


def response_cases() -> list[dict]:
    bodies = {
        "ollama": [
            {"response": json.dumps({"make": "Subaru"})},
            {"response": "", "thinking": json.dumps({"make": "BMW"})},
            {"response": "  ", "thinking": ""},
            {"response": "not json"},
            {},
        ],
        "openai": [
            {"choices": [{"message": {"content": json.dumps({"make": "Ford"})}}]},
            {"choices": []},
            {"choices": [{"message": {"content": "not json"}}]},
            {},
        ],
        "anthropic": [
            {"content": [{"type": "tool_use", "input": {"make": "Holden"}}]},
            {"content": [
                {"type": "text", "text": "thinking out loud"},
                {"type": "tool_use", "name": "describe_vehicle", "input": {"make": "Toyota"}},
            ]},
            {"content": [{"type": "text", "text": "no thanks"}]},
            {"content": [{"type": "tool_use"}]},
            {},
        ],
        "gemini": [
            {"candidates": [{"content": {"parts": [{"text": json.dumps({"make": "Toyota"})}]}}]},
            {"candidates": [{"content": {"parts": [{"text": "not json"}]}}]},
            {"candidates": []},
            {},
        ],
    }
    return [_parse(provider, body) for provider, group in bodies.items() for body in group]


def dispatch_cases() -> dict:
    return {
        "unknown_provider_raises": True,
        "empty_provider_is_ollama": True,
    }


def ollama_hosts_cases() -> list[dict]:
    rows = [
        Settings(),
        Settings(vlm_host="http://a:11434", vlm_extra_hosts="http://b:11434, http://c:11434"),
        Settings(vlm_host="http://a:11434/", vlm_extra_hosts="http://a:11434, http://b:11434"),
        Settings(vlm_host="http://a:11434", vlm_extra_hosts=" , http://b:11434 ,, "),
    ]
    return [{"vlm_host": s.vlm_host, "vlm_extra_hosts": s.vlm_extra_hosts,
            "expected": s.ollama_hosts()} for s in rows]


def anthropic_key_kind_cases() -> list[dict]:
    rows = []
    for key, kind in [
        ("sk-ant-api03-abc", "auto"), ("sk-ant-oat01-abc", "auto"),
        ("sk-ant-ort01-abc", "auto"), ("sk-ant-api03-abc", "api-key"),
        ("sk-ant-oat01-abc", "claude-code"), ("", "auto"),
    ]:
        settings = Settings(vlm_api_key=key, anthropic_key_kind=kind)
        rows.append({"vlm_api_key": key, "anthropic_key_kind": kind,
                     "expected_kind": vlm_providers.anthropic_key_kind(settings),
                     "expected_auth": vlm_providers.anthropic_auth(settings)})
    return rows


def main() -> None:
    payload = {
        "requests": request_cases(),
        "responses": response_cases(),
        "dispatch": dispatch_cases(),
        "ollama_hosts": ollama_hosts_cases(),
        "anthropic_key_kind": anthropic_key_kind_cases(),
    }
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(payload, indent=1, sort_keys=True) + "\n", encoding="utf-8")
    print(f"{OUT.relative_to(ROOT)}  {len(payload['requests'])} request cases, "
          f"{len(payload['responses'])} response cases")


if __name__ == "__main__":
    main()
