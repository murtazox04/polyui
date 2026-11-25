import json
from itertools import count

import pytest
from conftest import ACTIONS, lookup

import polyui

USER = {"id": "u1"}


def ids():
    made = count(1)
    return lambda: f"h{next(made)}"


def chunks(text, size=15):
    return [text[at : at + size] for at in range(0, len(text), size)]


@pytest.fixture
def hosted(catalogs):
    return polyui.Validator(
        catalogs,
        actions=ACTIONS,
        ref_sources={"account": {"kind": "string", "max": 40}},
        resolver=lookup,
    )


def stream_all(validator, payload, **options):
    stream = polyui.ShowStream()
    sent = []
    text = json.dumps(payload)
    for at, chunk in enumerate(chunks(text)):
        for message in validator.feed_show(stream, chunk, **options):
            sent.append((at, message))
    return stream, sent, len(chunks(text))


def test_components_reach_the_client_while_the_call_streams(hosted, bills_card):
    next_id = ids()
    stream, sent, total = stream_all(hosted, bills_card, user=USER, next_id=next_id)
    kinds = [message["t"] for _, message in sent]
    assert kinds[0] == "ui.open"
    assert set(kinds[1:]) == {"ui.patch"}
    assert sent[0][0] < total // 2
    assert [message["seq"] for _, message in sent] == list(range(1, len(sent) + 1))
    streamed = json.dumps([message for _, message in sent])
    assert "**** 4471" in streamed
    assert "$ref" not in streamed
    assert stream.opened == "s1"
    assert not stream.stopped

    prepared = hosted.finish_show(stream, user=USER, next_id=next_id)
    assert prepared.messages == []
    assert prepared.seq == stream.seq
    handles = [action["action_id"] for action in prepared.actions]
    assert handles == ["h1", "h2"]
    assert all(handle in streamed for handle in handles)
    assert prepared.model.get("power")["props"]["label"] == {"$ref": "account:power"}
    assert prepared.summary == hosted.prepare_show(bills_card, user=USER).summary
    assert stream.finished
    with pytest.raises(RuntimeError, match="finished"):
        hosted.feed_show(stream, "{}")


def test_finish_without_an_early_open_is_prepare_show(hosted, bills_card):
    stream = polyui.ShowStream()
    assert hosted.feed_show(stream, json.dumps(bills_card)[:20], user=USER) == []
    prepared = hosted.finish_show(stream, bills_card, user=USER, surface_id="s7")
    assert [message["t"] for message in prepared.messages] == ["ui.open"]
    assert prepared.surface_id == "s7"


def test_a_bad_component_stops_the_stream_and_the_final_check_decides(hosted, bills_card):
    broken = json.loads(json.dumps(bills_card))
    broken["components"][3]["type"] = "Marquee"
    stream, sent, _ = stream_all(hosted, broken, user=USER)
    assert sent
    assert stream.stopped
    with pytest.raises(polyui.ValidationError) as caught:
        hosted.finish_show(stream, user=USER)
    assert "Marquee" in str(caught.value)
    assert stream.close_message() == {
        "v": 1,
        "t": "ui.close",
        "surface": "s1",
        "seq": stream.seq + 1,
    }


def test_a_refused_ref_stops_the_preview_but_not_the_final_call(hosted, bills_card):
    stream, sent, _ = stream_all(hosted, bills_card, user=USER, refs={})
    assert stream.stopped
    assert "**** 4471" not in json.dumps([message for _, message in sent])
    prepared = hosted.finish_show(stream, user=USER, refs={"account:power": "**** 4471"})
    assert prepared.client.get("power")["props"]["label"] == "**** 4471"
    upserted = [
        component["id"]
        for message in prepared.messages
        if message["t"] == "ui.patch"
        for component in message["upsert"]
    ]
    assert "power" in upserted


def test_changed_components_and_data_are_patched_at_the_end(hosted):
    first = {
        "root": "card",
        "components": [
            {"id": "card", "type": "Card", "props": {"title": "Draft"}, "children": ["t"]},
            {"id": "t", "type": "Text", "props": {"text": "Hello"}},
        ],
    }
    stream, sent, _ = stream_all(hosted, first)
    assert sent
    final = {
        "root": "card",
        "components": [
            {"id": "card", "type": "Card", "props": {"title": "Final"}},
        ],
        "data": [{"path": "/name", "value": "Ali"}],
    }
    prepared = hosted.finish_show(stream, final)
    kinds = [message["t"] for message in prepared.messages]
    assert kinds == ["ui.data", "ui.patch"]
    patch = prepared.messages[1]
    assert patch["remove"] == ["t"]
    assert patch["upsert"][0]["props"]["title"] == "Final"
