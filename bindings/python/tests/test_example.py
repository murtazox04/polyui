import sys
from pathlib import Path

import pytest

pytest.importorskip("fastapi")
pytest.importorskip("httpx")

from fastapi.testclient import TestClient

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "examples"))

from fastapi_app import app

BUTTON = {
    "root": "pay",
    "components": [
        {
            "id": "pay",
            "type": "Button",
            "props": {
                "label": {"$ref": "account:gas"},
                "on_press": {"action": "pay_bill", "args": {"bill": "b_17"}},
            },
        }
    ],
}


@pytest.fixture
def client() -> TestClient:
    return TestClient(app)


def test_valid_show_ui_returns_ui_open(client):
    response = client.post("/show_ui", json=BUTTON, headers={"x-user": "u1"})
    assert response.status_code == 200
    body = response.json()
    assert body["message"]["t"] == "ui.open"
    assert body["message"]["components"][0]["props"]["label"] == "**** 0083"
    assert body["tool_result"] == 'surface s1 open: Button "[ref]" (act)'


def test_invalid_show_ui_returns_problems(client):
    response = client.post("/show_ui", json={"root": "x", "components": []})
    assert response.status_code == 422
    detail = response.json()["detail"]
    assert detail["tool_result"].startswith("2 problems, nothing was shown")
    assert [problem["at"] for problem in detail["problems"]] == ["components", "root"]


def test_refused_refs_never_reach_the_client(client):
    response = client.post("/show_ui", json=BUTTON, headers={"x-user": "u9"})
    assert response.status_code == 422
    assert "0083" not in response.text
