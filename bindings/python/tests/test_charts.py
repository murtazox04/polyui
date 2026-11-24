import pytest

import polyui

GAS = {
    "title": "Gas use",
    "kind": "bar",
    "unit": "m3",
    "rows": [{"x": "Jan", "y": 31}, {"x": "Feb", "y": 28}, {"x": "Mar", "y": 19}],
}


def test_compile_chart_returns_a_scene():
    scene = polyui.compile_chart(GAS, width=480, height=320)
    assert scene["marks"]
    assert scene["view_box"]["width"] == 480
    assert scene["view_box"]["height"] == 320


def test_chart_svg_and_description():
    svg = polyui.chart_svg(GAS)
    assert svg.startswith("<svg")
    assert svg.rstrip().endswith("</svg>")
    assert polyui.chart_svg(GAS, dark=True) != svg
    assert polyui.describe_chart(GAS).startswith("Gas use.")
    assert polyui.describe_chart(GAS, locale="ru") != polyui.describe_chart(GAS)


def test_bad_chart_props_raise_chart_errors():
    with pytest.raises(polyui.ChartError):
        polyui.compile_chart({"title": "x", "kind": "donut"})
    with pytest.raises(polyui.ChartError, match="rows"):
        polyui.compile_chart({**GAS, "rows": [{"x": str(n), "y": n} for n in range(1001)]})


def test_charts_are_compiled_into_surfaces(validator):
    payload = {
        "root": "chart",
        "components": [{"id": "chart", "type": "chart:Chart", "props": GAS}],
    }
    surface = validator.show_ui(payload)
    compiled = validator.compile_charts(surface, locale="en")
    chart = compiled.get("chart")
    assert chart["props"]["scene"]["marks"]
    assert chart["say"].startswith("Gas use.")
    assert "scene" not in surface.get("chart")["props"]
    prepared = validator.prepare_show(payload)
    assert prepared.messages[0]["components"][0]["props"]["scene"]["marks"]
    assert "scene" not in prepared.model.get("chart")["props"]
