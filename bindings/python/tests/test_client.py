import json

import pytest
from conftest import FIXTURES, load_fixture

import polyui

HELLO = {
    "v": 1,
    "t": "hello",
    "catalogs": ["core@1", "chart@1"],
    "modalities": ["screen", "voice"],
    "locale": "en",
}


def run_step(client: polyui.Client, step: dict):
    if "receive" in step:
        return client.receive(step["receive"])
    if "receive_text" in step:
        return client.receive(step["receive_text"])
    if "set_value" in step:
        call = step["set_value"]
        return client.set_value(call["surface"], call["path"], call["value"])
    if "fire" in step:
        call = step["fire"]
        return client.fire(call["surface"], call["component"], call["prop"], call.get("detail"))
    if "confirm" in step:
        call = step["confirm"]
        return client.confirm(call["surface"], call["action_id"], call["nonce"])
    if "say" in step:
        return client.say(step["say"])
    if "match_voice" in step:
        call = step["match_voice"]
        return client.match_voice(call["surface"], call["text"])
    raise AssertionError(f"unknown step {sorted(step)}")


def test_open_basic_fixture():
    case = load_fixture("client", "open_basic.json")
    client = polyui.Client(case["hello"])
    step = case["steps"][0]
    assert client.receive(step["receive"]) == step["expect"]
    assert client.receive(json.dumps(step["receive"]))["surface"] == "s1"
    assert client.surfaces == ["s1"]
    assert client.fallback("s1") == step["expect"]["fallback"]
    action = client.fire("s1", "pay", "on_press")
    assert action["t"] == "ui.action"
    assert action["action_id"] == "act_4kq9"


@pytest.mark.parametrize("name", sorted(path.name for path in (FIXTURES / "client").glob("*.json")))
def test_client_conformance_fixtures(name):
    case = load_fixture("client", name)
    client = polyui.Client(case["hello"])
    for number, step in enumerate(case["steps"]):
        assert run_step(client, step) == step["expect"], f"{name} step {number}"


def test_bad_hello_is_a_protocol_error():
    with pytest.raises(polyui.ProtocolError):
        polyui.Client({"v": 1, "t": "hello"})
    with pytest.raises(polyui.ProtocolError):
        polyui.Client("{not json")


def test_server_output_round_trips_through_the_client(validator, bills_card):
    prepared = validator.prepare_show(bills_card, user={"id": "u1"})
    client = polyui.Client(HELLO)
    batch = client.receive(prepared.messages[0])
    created = [op["id"] for op in batch["ops"] if op["op"] == "create"]
    assert created == ["card", "list", "gas", "power"]
    action = client.fire("s1", "power", "on_press")
    assert action["action_id"] == prepared.actions[1]["action_id"]
    assert client.confirm("s1", action["action_id"], "n_1") is None
    sheet = client.receive(
        {
            "v": 1,
            "t": "ui.confirm",
            "surface": "s1",
            "action_id": action["action_id"],
            "nonce": "n_1",
            "title": "Pay power",
        }
    )
    assert sheet["ops"][0]["op"] == "confirm"
    assert client.confirm("s1", action["action_id"], "n_1")["confirm"] == "n_1"
    utterances = client.say("s1")
    assert utterances[0]["id"] == "card"
    assert "**** 4471" in json.dumps(utterances)


def test_inputs_and_voice():
    client = polyui.Client(HELLO)
    client.receive(
        {
            "v": 1,
            "t": "ui.open",
            "surface": "s1",
            "seq": 1,
            "root": "form",
            "components": [
                {
                    "id": "form",
                    "type": "Card",
                    "props": {"title": "Plan"},
                    "children": ["phone", "plan"],
                },
                {"id": "phone", "type": "Input", "props": {"label": "Phone", "value": "/phone"}},
                {
                    "id": "plan",
                    "type": "ChoiceGroup",
                    "props": {
                        "label": "Plan",
                        "value": "/plan",
                        "options": [
                            {"id": "basic", "label": "Basic"},
                            {"id": "max", "label": "Max"},
                        ],
                    },
                },
            ],
            "data": {},
        }
    )
    batch = client.set_value("s1", "/phone", "+998901234567")
    assert batch["ops"][0]["value"] == "+998901234567"
    assert client.data("s1") == {"phone": "+998901234567"}
    found = client.match_voice("s1", "the second one")
    assert found["component"] == "plan"
    assert found["option"] == "max"
    assert client.data("s1")["plan"] == "max"
    assert client.match_voice("s1", "") is None
    assert client.fire("s1", "nothing", "on_press") is None
