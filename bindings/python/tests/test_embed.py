import base64
import json

import pytest

import polyui

URI = "ui://bills/tariff"
MIME = "text/html;profile=mcp-app"
HTML = "<!doctype html><p>Tariff calculator</p>"
USER = {"id": "u1"}


def entry(html=HTML, uri=URI, mime=MIME):
    return {"uri": uri, "mimeType": mime, "text": html}


def tariff_card(resource=URI):
    return {
        "root": "card",
        "components": [
            {"id": "card", "type": "Card", "props": {"title": "Tariffs"}, "children": ["calc"]},
            {
                "id": "calc",
                "type": "embed:Embed",
                "props": {"resource": resource, "title": "Tariff calculator", "height": "large"},
            },
        ],
    }


def make(ui_resource):
    return polyui.Validator(polyui.Catalogs.builtin().with_embed(), ui_resource=ui_resource)


def test_the_app_reaches_the_client_but_not_the_model():
    seen = []

    def read(user, uri):
        seen.append((user, uri))
        return {"resource": entry(), "tool_input": {"usage_gb": 236}}

    prepared = make(read).prepare_show(tariff_card(), user=USER, declared=["core@1", "embed@1"])
    assert seen == [(USER, URI)]
    app = prepared.client.get("calc")["props"]["app"]
    assert app["resource"]["text"] == HTML
    assert app["tool_input"] == {"usage_gb": 236}
    assert "app" not in prepared.model.get("calc")["props"]
    assert HTML not in prepared.summary
    assert URI not in prepared.summary
    assert HTML in json.dumps(prepared.messages[0])


def test_base64_blobs_are_decoded():
    blob = base64.b64encode(HTML.encode()).decode()
    validator = make(lambda user, uri: {"uri": uri, "mimeType": MIME, "blob": blob})
    prepared = validator.prepare_show(tariff_card())
    resource = prepared.client.get("calc")["props"]["app"]["resource"]
    assert resource["text"] == HTML
    assert "blob" not in resource


@pytest.mark.parametrize(
    "found",
    [None, entry(mime="text/html"), entry(uri="ui://bills/other"), {"uri": URI, "mimeType": MIME}],
)
def test_resources_that_are_not_apps_are_problems_for_the_model(found):
    with pytest.raises(polyui.ValidationError) as caught:
        make(lambda user, uri: found).prepare_show(tariff_card())
    problem = caught.value.problems[0]
    assert problem["id"] == "calc"
    assert problem["prop"] == "resource"
    assert "is not an MCP App this host offers" in problem["message"]


def test_apps_too_large_for_one_message_are_refused():
    big = "x" * (210 * 1024)
    with pytest.raises(polyui.ValidationError) as caught:
        make(lambda user, uri: entry(big)).prepare_show(tariff_card())
    assert "too large" in caught.value.problems[0]["message"]


def test_clients_without_embed_never_trigger_a_read():
    def read(user, uri):
        raise AssertionError("no read expected")

    prepared = make(read).prepare_show(tariff_card(), declared=["core@1"])
    assert prepared.messages[0]["components"][1]["type"] == "Text"


def test_update_ui_attaches_apps_to_upserts():
    validator = make(lambda user, uri: entry())
    card = {"id": "card", "type": "Card", "props": {"title": "Tariffs"}}
    shown = validator.prepare_show({"root": "card", "components": [card]})
    update = {
        "surface": shown.surface_id,
        "upsert": [
            {"id": "card", "type": "Card", "props": {"title": "Tariffs"}, "children": ["calc"]},
            tariff_card()["components"][1],
        ],
    }
    prepared = validator.prepare_update(update, shown.model, shown.client, shown.seq)
    assert prepared.client.get("calc")["props"]["app"]["resource"]["uri"] == URI
    assert "app" not in prepared.model.get("calc")["props"]


def test_callback_errors_and_bad_shapes_are_raised():
    def broken(user, uri):
        raise ConnectionError("mcp server is down")

    with pytest.raises(ConnectionError):
        make(broken).prepare_show(tariff_card())
    with pytest.raises(ValueError):
        make(lambda user, uri: {"resource": entry(), "tool_input": 3}).prepare_show(tariff_card())

    async def later(user, uri):
        return entry()

    with pytest.raises(TypeError):
        make(later).prepare_show(tariff_card())
    with pytest.raises(TypeError):
        make("not callable")


def test_attach_apps_alone():
    validator = make(lambda user, uri: entry())
    surface = validator.show_ui(tariff_card())
    attached = validator.attach_apps(surface, USER)
    assert attached.get("calc")["props"]["app"]["resource"]["mimeType"] == MIME
    assert "app" not in surface.get("calc")["props"]
    assert "embed:Embed" in polyui.Catalogs.builtin().with_embed()
    assert "embed:Embed" not in polyui.Catalogs.builtin()


def test_resource_uris_lists_each_app_once():
    card = tariff_card()
    card["components"].append(
        {"id": "again", "type": "embed:Embed", "props": {"resource": URI, "title": "Again"}}
    )
    card["components"].append(
        {
            "id": "plain",
            "type": "embed:Embed",
            "props": {"resource": "https://x.example", "title": "No"},
        }
    )
    validator = make(None)
    assert validator.resource_uris(card) == [URI]
    assert validator.resource_uris(json.dumps(card)) == [URI]
    assert validator.resource_uris("not json") == []
    update = {"upsert": [{"id": "calc", "type": "embed:Embed", "props": {"resource": URI}}]}
    assert validator.resource_uris(update) == [URI]


def test_prefetched_resources_replace_the_callback():
    def read(user, uri):
        raise AssertionError("the callback must not run when resources are given")

    validator = make(read)
    uris = validator.resource_uris(tariff_card())
    prepared = validator.prepare_show(
        tariff_card(),
        user=USER,
        declared=["core@1", "embed@1"],
        resources={uri: {"resource": entry(), "tool_input": {"usage_gb": 236}} for uri in uris},
    )
    app = prepared.client.get("calc")["props"]["app"]
    assert app["resource"]["text"] == HTML
    assert app["tool_input"] == {"usage_gb": 236}
    assert "app" not in prepared.model.get("calc")["props"]


def test_a_missing_prefetched_resource_is_a_problem_for_the_model():
    validator = make(None)
    for resources in ({}, {URI: None}):
        with pytest.raises(polyui.ValidationError) as caught:
            validator.prepare_show(tariff_card(), resources=resources)
        assert "not an MCP App this host offers" in str(caught.value)


def test_bad_prefetched_resources_raise():
    validator = make(None)
    with pytest.raises(ValueError, match="must be an object"):
        validator.prepare_show(tariff_card(), resources={URI: "<html>"})
    with pytest.raises(polyui.ValidationError):
        validator.prepare_show(tariff_card(), resources={URI: entry(mime="text/html")})
    with pytest.raises(TypeError, match="resources must be a mapping"):
        validator.prepare_show(tariff_card(), resources=[URI])


def test_prefetched_resources_work_for_attach_apps_and_streaming():
    validator = make(None)
    surface = validator.show_ui(tariff_card())
    attached = validator.attach_apps(surface, resources={URI: entry()})
    assert attached.get("calc")["props"]["app"]["resource"]["text"] == HTML
    stream = polyui.ShowStream()
    text = json.dumps(tariff_card())
    for message in validator.feed_show(stream, text, resources={URI: entry()}):
        assert message["t"] in ("ui.open", "ui.patch")
    prepared = validator.finish_show(stream, resources={URI: entry()})
    assert prepared.client.get("calc")["props"]["app"]["resource"]["text"] == HTML
