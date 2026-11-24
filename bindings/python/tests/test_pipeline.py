import json
from itertools import count

import pytest
from conftest import ACTIONS, lookup

import polyui

USER = {"id": "u1"}


def ids():
    numbers = count(1)
    return lambda: f"act_{next(numbers)}"


def test_refs_never_reach_the_model(validator, bills_card):
    seen = []

    def spy(user, source, key):
        seen.append((user, source, key))
        return lookup(user, source, key)

    spying = polyui.Validator(
        validator.catalogs,
        actions=ACTIONS,
        ref_sources={"account": {"kind": "string", "max": 40}},
        resolver=spy,
    )
    prepared = spying.prepare_show(bills_card, user=USER, next_id=ids())
    assert seen == [(USER, "account", "power")]
    assert "**** 4471" not in prepared.summary
    assert '"[ref]"' in prepared.summary
    assert "**** 4471" not in json.dumps(prepared.model.to_dict())
    assert prepared.model.get("power")["props"]["label"] == {"$ref": "account:power"}
    assert prepared.client.get("power")["props"]["label"] == "**** 4471"
    message = prepared.messages[0]
    assert message["t"] == "ui.open"
    assert "**** 4471" in json.dumps(message)
    assert "$ref" not in json.dumps(message)


def test_resolve_refs_alone(validator, bills_card):
    surface = validator.show_ui(bills_card)
    resolved = validator.resolve_refs(surface, USER)
    assert resolved.get("power")["props"]["label"] == "**** 4471"
    assert surface.get("power")["props"]["label"] == {"$ref": "account:power"}


@pytest.mark.parametrize("user", [None, {"id": "someone else"}])
def test_refused_refs_are_validation_errors(validator, bills_card, user):
    with pytest.raises(polyui.ValidationError) as caught:
        validator.prepare_show(bills_card, user=user)
    assert caught.value.problems[0]["id"] == "power"
    assert "not your account" not in str(caught.value)


def test_missing_values_are_refused(validator):
    payload = {
        "root": "t",
        "components": [{"id": "t", "type": "Text", "props": {"text": {"$ref": "account:nope"}}}],
    }
    with pytest.raises(polyui.ValidationError):
        validator.prepare_show(payload, user=USER)


def test_unregistered_sources_are_rejected(validator):
    payload = {
        "root": "t",
        "components": [{"id": "t", "type": "Text", "props": {"text": {"$ref": "card:main"}}}],
    }
    with pytest.raises(polyui.ValidationError) as caught:
        validator.show_ui(payload)
    assert "card" in caught.value.problems[0]["message"]


def test_resolver_failures_propagate(catalogs):
    def broken(user, source, key):
        raise ConnectionError("database is down")

    failing = polyui.Validator(
        catalogs, ref_sources={"account": {"kind": "string", "max": 40}}, resolver=broken
    )
    payload = {
        "root": "t",
        "components": [{"id": "t", "type": "Text", "props": {"text": {"$ref": "account:gas"}}}],
    }
    with pytest.raises(ConnectionError, match="database is down"):
        failing.prepare_show(payload)


def test_actions_are_sealed_into_handles(validator, bills_card):
    prepared = validator.prepare_show(bills_card, user=USER, next_id=ids())
    assert prepared.actions == [
        {
            "action_id": "act_1",
            "component": "gas",
            "prop": "on_press",
            "action": "pay_bill",
            "args": {"bill": "b_17"},
            "with": [],
            "requires_confirm": True,
        },
        {
            "action_id": "act_2",
            "component": "power",
            "prop": "on_press",
            "action": "pay_bill",
            "args": {"bill": "b_18"},
            "with": [],
            "requires_confirm": True,
        },
    ]
    shown = {component["id"]: component for component in prepared.messages[0]["components"]}
    assert shown["gas"]["props"]["on_press"] == {"action_id": "act_1", "confirm": True}
    assert "b_17" not in json.dumps(prepared.messages[0])
    assert prepared.model.get("gas")["props"]["on_press"]["action"] == "pay_bill"


def test_seal_actions_with_random_ids(validator, bills_card):
    surface = validator.show_ui(bills_card)
    sealed, actions = validator.seal_actions(surface)
    assert len(actions) == 2
    assert actions[0]["action_id"].startswith("act_")
    assert len(actions[0]["action_id"]) == len("act_") + 16
    assert actions[0]["action_id"] != actions[1]["action_id"]
    assert sealed.get("gas")["props"]["on_press"]["action_id"] == actions[0]["action_id"]


def test_bad_ids_from_next_id_are_rejected(validator, bills_card):
    surface = validator.show_ui(bills_card)
    with pytest.raises(ValueError, match="next_id"):
        validator.seal_actions(surface, next_id=lambda: "not valid!")


def test_prepare_update_builds_patches(validator, bills_card):
    shown = validator.prepare_show(bills_card, user=USER, next_id=ids())
    changed = validator.prepare_update(
        {
            "surface": "s1",
            "upsert": [
                {
                    "id": "gas",
                    "type": "Button",
                    "props": {
                        "label": "Gas paid",
                        "on_press": {"action": "pick_offer", "args": {"offer": "o1"}},
                    },
                }
            ],
            "data": [{"path": "/paid", "value": True}],
        },
        shown.model,
        shown.client,
        shown.seq,
        user=USER,
        next_id=lambda: "act_9",
    )
    assert changed.seq == 3
    assert [message["t"] for message in changed.messages] == ["ui.data", "ui.patch"]
    patch = changed.messages[1]
    assert patch["seq"] == 3
    assert patch["upsert"][0]["props"]["on_press"] == {"action_id": "act_9", "confirm": False}
    assert changed.touched == ["gas"]
    assert changed.actions[0]["action"] == "pick_offer"
    assert 'Button "Gas paid"' in changed.summary
    with pytest.raises(polyui.ValidationError) as caught:
        validator.prepare_update(
            {"surface": "s9", "remove": ["gas"]}, shown.model, shown.client, shown.seq
        )
    assert "is not open" in str(caught.value)


def test_downgrade_for_a_client_without_charts(validator):
    payload = {
        "root": "chart",
        "components": [
            {
                "id": "chart",
                "type": "chart:Chart",
                "props": {
                    "title": "Gas",
                    "kind": "bar",
                    "rows": [{"x": "Jan", "y": 3}, {"x": "Feb", "y": 5}],
                },
            }
        ],
    }
    prepared = validator.prepare_show(payload, declared=["core@1"])
    component = prepared.messages[0]["components"][0]
    assert component["type"] == "Text"
    assert prepared.client.get("chart")["type"] == "chart:Chart"


def test_tool_definitions_shape(catalogs):
    tools = polyui.tool_definitions(catalogs, ACTIONS)
    assert [tool["name"] for tool in tools] == ["show_ui", "update_ui", "close_ui"]
    for tool in tools:
        assert set(tool) >= {"name", "description", "input_schema"}
        assert tool["input_schema"]["type"] == "object"
        assert tool["input_schema"]["additionalProperties"] is False
        assert "strict" not in tool
    assert tools[0]["input_schema"]["required"] == ["root", "components"]
    strict = polyui.tool_definitions(catalogs, ACTIONS, strict=True)
    assert [tool["name"] for tool in strict] == ["show_ui", "update_ui", "close_ui"]
    assert any(tool.get("strict") is True for tool in strict)
    assert "chart:Chart" in json.dumps(tools[0])
    core_only = polyui.tool_definitions(catalogs.restrict(["core@1"]), ACTIONS)
    assert "chart:Chart" not in json.dumps(core_only[0])
    assert "pay_bill" in json.dumps(tools[0])


def test_validator_tool_definitions_match_module_function(validator, catalogs):
    assert validator.tool_definitions(["core@1"]) == polyui.tool_definitions(
        catalogs.restrict(["core@1"]), ACTIONS
    )


def test_message_builders(validator, bills_card):
    surface = validator.show_ui(bills_card)
    opened = polyui.open_message("s1", surface, fallback="Your bills")
    assert opened["v"] == 1
    assert opened["t"] == "ui.open"
    assert opened["seq"] == 1
    assert opened["fallback"] == "Your bills"
    patched = polyui.patch_message("s1", 2, remove=["gas"], root="card")
    assert patched == {
        "v": 1,
        "t": "ui.patch",
        "surface": "s1",
        "seq": 2,
        "remove": ["gas"],
        "root": "card",
    }
    assert polyui.close_message("s1", 3) == {"v": 1, "t": "ui.close", "surface": "s1", "seq": 3}


def test_async_resolvers_are_rejected(catalogs):
    async def later(user, source, key):
        return "**** 0083"

    validator = polyui.Validator(
        catalogs, ref_sources={"account": {"kind": "string", "max": 40}}, resolver=later
    )
    payload = {
        "root": "t",
        "components": [{"id": "t", "type": "Text", "props": {"text": {"$ref": "account:gas"}}}],
    }
    with pytest.raises(TypeError, match="awaitable"):
        validator.prepare_show(payload)
