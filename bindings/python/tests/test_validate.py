import json

import pytest

import polyui


def test_show_ui_returns_a_surface(validator, bills_card):
    surface = validator.show_ui(bills_card)
    assert isinstance(surface, polyui.Surface)
    assert surface.root == "card"
    assert [component["id"] for component in surface.components] == [
        "card",
        "list",
        "gas",
        "power",
    ]
    assert surface.get("gas")["props"]["tone"] == "primary"
    assert validator.summary(surface, "s1") == (
        'surface s1 open: Card "Your bills" > List(2) > '
        '[Button "Gas 29 322 so\'m" (act), Button "[ref]" (act)]'
    )


def test_show_ui_accepts_json_text(validator, bills_card):
    assert validator.show_ui(json.dumps(bills_card)) == validator.show_ui(bills_card)


def test_problems_are_raised_with_model_text_and_structure(validator):
    payload = {
        "root": "card",
        "components": [
            {"id": "card", "type": "Card", "children": ["missing"]},
            {"id": "pay", "type": "PayButton"},
        ],
    }
    with pytest.raises(polyui.ValidationError) as caught:
        validator.show_ui(payload)
    error = caught.value
    assert isinstance(error, polyui.PolyUIError)
    assert error.operation == "show"
    assert str(error).startswith("3 problems, nothing was shown:\n1. ")
    assert len(error.problems) == 3
    unknown = next(problem for problem in error.problems if problem.get("id") == "pay")
    assert unknown["list"] == "components"
    assert unknown["index"] == 1
    assert "is not in the catalog" in unknown["message"]
    assert unknown["at"] == 'components[1] id "pay"'
    for problem in error.problems:
        assert problem["at"] in str(error)


def test_invalid_json_text_is_a_validation_error(validator):
    with pytest.raises(polyui.ValidationError) as caught:
        validator.show_ui('{"root": ')
    assert caught.value.problems[0]["at"] == "input"
    assert "invalid JSON" in caught.value.problems[0]["message"]


def test_action_arguments_are_checked(validator):
    payload = {
        "root": "pay",
        "components": [
            {
                "id": "pay",
                "type": "Button",
                "props": {
                    "label": "Pay",
                    "on_press": {"action": "pay_bill", "args": {"bill": 17}},
                },
            }
        ],
    }
    with pytest.raises(polyui.ValidationError) as caught:
        validator.show_ui(payload)
    assert caught.value.problems == [
        {
            "at": 'components[0] id "pay" prop "on_press.args.bill"',
            "message": "expected text, got a number.",
            "list": "components",
            "index": 0,
            "id": "pay",
            "prop": "on_press.args.bill",
        }
    ]


def test_update_and_close(validator, bills_card):
    surface = validator.show_ui({**bills_card, "surface": "s1"})
    update = validator.update_ui(
        {
            "surface": "s1",
            "remove": ["power"],
            "upsert": [{"id": "list", "type": "List", "children": ["gas"]}],
            "data": [{"path": "/total", "value": 29322}],
        },
        surface,
    )
    assert update.surface == "s1"
    assert update.remove == ["power"]
    assert update.data == [{"op": "set", "path": "/total", "value": 29322}]
    changed = surface.apply(update)
    assert changed.get("power") is None
    assert changed.data == {"total": 29322}
    assert validator.close_ui({"surface": "s1"}) == "s1"
    assert polyui.closed_summary("s1") == "surface s1 closed"
    with pytest.raises(polyui.ValidationError) as caught:
        validator.update_ui({"surface": "s1"}, surface)
    assert caught.value.operation == "update"
    with pytest.raises(polyui.ValidationError) as caught:
        validator.close_ui({})
    assert caught.value.operation == "close"


def test_surface_survives_a_dict_round_trip(validator, bills_card):
    surface = validator.show_ui(bills_card)
    stored = json.loads(json.dumps(surface.to_dict()))
    assert polyui.Surface.from_dict(stored) == surface
    with pytest.raises(TypeError):
        polyui.Surface.from_dict({"components": []})


def test_bad_action_specs_raise_catalog_errors(catalogs):
    with pytest.raises(polyui.CatalogError) as caught:
        polyui.Validator(catalogs, actions={"pay": {"requires_confirm": "yes"}})
    assert caught.value.path == "pay.requires_confirm"
    with pytest.raises(polyui.CatalogError):
        polyui.Validator(catalogs, ref_sources={"account": {"kind": "nope"}})
    with pytest.raises(TypeError):
        polyui.Validator(catalogs, resolver="not callable")


def test_extension_catalogs_and_restriction(catalogs):
    extension = {
        "id": "acme",
        "version": 1,
        "types": {
            "Link": {
                "description": "A link.",
                "role": "button",
                "props": {
                    "href": {"kind": "url", "required": True},
                    "label": {"kind": "string", "max": 40},
                },
                "alt": "{label}",
                "say": "{label}",
            }
        },
    }
    extended = catalogs.with_catalog(extension)
    assert extended.tags == ["core@1", "chart@1", "acme@1"]
    assert "acme:Link" in extended
    assert "acme:Link" not in catalogs
    assert extended.with_catalog(json.dumps({**extension, "id": "beta"})).tags[-1] == "beta@1"
    assert extended.restrict(["core@1", "acme@1"]).tags == ["core@1", "acme@1"]
    assert extended.negotiate(["acme@1", "chart@2"]) == ["acme@1"]
    with pytest.raises(polyui.CatalogError):
        extended.with_catalog(extension)
    with pytest.raises(polyui.CatalogError):
        catalogs.with_catalog({"id": "core", "version": 1, "types": {}})
    validator = polyui.Validator(extended)
    surface = validator.show_ui(
        {
            "root": "link",
            "components": [
                {
                    "id": "link",
                    "type": "acme:Link",
                    "props": {"href": "https://example.com", "label": "Docs"},
                }
            ],
        }
    )
    downgraded = validator.downgrade(surface, ["core@1"])
    assert downgraded.get("link")["type"] == "Text"
    assert downgraded.get("link")["props"]["text"] == "Docs"
