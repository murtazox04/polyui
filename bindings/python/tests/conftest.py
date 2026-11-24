import json
from pathlib import Path

import pytest

import polyui

FIXTURES = Path(__file__).resolve().parents[3] / "tests" / "conformance"

ACTIONS = {
    "pay_bill": {
        "args": {"bill": {"kind": "string", "required": True, "max": 32}},
        "requires_confirm": True,
    },
    "pick_offer": {"args": {"offer": {"kind": "string", "required": True}}},
}

SECRETS = {("account", "gas"): "**** 0083", ("account", "power"): "**** 4471"}


def load_fixture(kind: str, name: str) -> dict:
    return json.loads((FIXTURES / kind / name).read_text())


def lookup(user, source, key):
    if user is None or user.get("id") != "u1":
        raise polyui.RefRefusedError("not your account")
    return SECRETS.get((source, key))


@pytest.fixture
def catalogs() -> polyui.Catalogs:
    return polyui.Catalogs.builtin()


@pytest.fixture
def validator(catalogs: polyui.Catalogs) -> polyui.Validator:
    return polyui.Validator(
        catalogs,
        actions=ACTIONS,
        ref_sources={"account": {"kind": "string", "max": 40}},
        resolver=lookup,
    )


@pytest.fixture
def bills_card() -> dict:
    return {
        "root": "card",
        "components": [
            {
                "id": "card",
                "type": "Card",
                "props": {"title": "Your bills"},
                "children": ["list"],
            },
            {"id": "list", "type": "List", "children": ["gas", "power"]},
            {
                "id": "gas",
                "type": "Button",
                "props": {
                    "label": "Gas 29 322 so'm",
                    "tone": "primary",
                    "on_press": {"action": "pay_bill", "args": {"bill": "b_17"}},
                },
            },
            {
                "id": "power",
                "type": "Button",
                "props": {
                    "label": {"$ref": "account:power"},
                    "on_press": {"action": "pay_bill", "args": {"bill": "b_18"}},
                },
            },
        ],
    }
