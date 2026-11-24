import { describe, expect, it } from "vitest";
import type { ChartScene } from "../src/types";
import { harness } from "./setup";

const scene: ChartScene = {
  view_box: { x: 0, y: 0, width: 300, height: 160 },
  kind: "bar",
  title: "Monthly spend",
  description: "Spend peaked in October.",
  marks: [
    { type: "line", x1: 30, y1: 140, x2: 290, y2: 140, stroke: "axis", stroke_width: 1 },
    { type: "rect", x: 40, y: 60, width: 40, height: 80, fill: "series-1" },
    { type: "rect", x: 120, y: 20, width: 40, height: 120, fill: "series-2" },
    { type: "rect", x: 200, y: 100, width: 40, height: 40, fill: "url(javascript:alert(1))" },
    { type: "path", commands: [{ op: "M", x: 30, y: 100 }, { op: "L", x: 290, y: 40 }, { op: "A", radius: 10, large_arc: true, clockwise: true, x: 280, y: 50 }, { op: "Z" }], stroke: "series-3", stroke_width: 2, fill: "series-3", fill_opacity: 0.2 },
    { type: "path", commands: [{ op: "M", x: Number.NaN, y: 0 }], stroke: "series-3" },
    { type: "circle", cx: 140, cy: 20, r: 4, fill: "series-3" },
    { type: "text", x: 60, y: 155, text: "<b>Sep</b>", anchor: "middle", role: "tick", color: "muted", size: 10 },
    { type: "rect", x: Number.NaN, y: 0, width: 10, height: 10, fill: "series-1" },
  ],
  legend: [
    { label: "Gas", color: "series-1" },
    { label: "Power", color: "series-2" },
  ],
  hits: [
    { shape: "rect", row: 0, label: "Sep: 80", x: 40, y: 20, width: 40, height: 120 },
    { shape: "rect", row: 1, label: "Oct: 120", x: 120, y: 20, width: 40, height: 120 },
    { shape: "polygon", row: 2, label: "Nov: 40", points: [[200, 20], [240, 20], [240, 140], [200, 140]] },
  ],
};

function playChart(h: ReturnType<typeof harness>, withScene: boolean): void {
  h.play({
    surface: "s1",
    ops: [
      {
        op: "create",
        id: "chart",
        type: "chart:Chart",
        props: { title: "Monthly spend", kind: "bar", x: "month", y: "amount", rows: [], on_select: { action_id: "act_pick" }, ...(withScene ? { scene: scene as never } : {}) },
        a11y: { role: "figure", label: "Monthly spend" },
      },
      { op: "root", id: "chart" },
    ],
  });
}

describe("chart:Chart", () => {
  it("draws the scene with SVG elements and color tokens mapped to CSS variables", () => {
    const h = harness();
    playChart(h, true);
    const figure = h.find('[data-pui-id="chart"]');
    expect(figure.tagName).toBe("FIGURE");
    expect(figure.querySelector("figcaption")?.textContent).toBe("Monthly spend");
    const svg = figure.querySelector("svg");
    expect(svg?.namespaceURI).toBe("http://www.w3.org/2000/svg");
    expect(svg?.getAttribute("viewBox")).toBe("0 0 300 160");
    const rects = [...figure.querySelectorAll("g[aria-hidden] rect")] as SVGElement[];
    expect(rects).toHaveLength(3);
    expect(rects[0]?.getAttribute("height")).toBe("80");
    expect(rects[0]?.style.getPropertyValue("fill")).toContain("--pui-series-1");
    expect(rects[2]?.style.getPropertyValue("fill")).toBe("none");
    const path = figure.querySelector("g[aria-hidden] path");
    expect(figure.querySelectorAll("g[aria-hidden] path")).toHaveLength(1);
    expect(path?.getAttribute("d")).toBe("M30 100L290 40A10 10 0 1 1 280 50Z");
    expect(path?.getAttribute("fill-opacity")).toBe("0.2");
    expect(figure.querySelector("svg desc")?.textContent).toBe("Spend peaked in October.");
    expect(figure.querySelector("polygon.pui-hit")?.getAttribute("points")).toBe("200,20 240,20 240,140 200,140");
    expect(figure.querySelector("text")?.getAttribute("class")).toBe("pui-mark-tick");
    expect((figure.querySelector("text") as SVGElement | null)?.style.getPropertyValue("fill")).toContain("--pui-chart-muted");
    expect(figure.querySelector("text")?.textContent).toBe("<b>Sep</b>");
    expect(figure.querySelector("text")?.children).toHaveLength(0);
    expect([...figure.querySelectorAll(".pui-legend-item")].map((item) => item.textContent)).toEqual(["Gas", "Power"]);
  });

  it("makes hits focusable buttons and fires on_select with the row", () => {
    const h = harness();
    playChart(h, true);
    const hits = [...document.querySelectorAll<SVGElement>(".pui-hit")];
    expect(hits).toHaveLength(3);
    expect(hits.map((hit) => hit.getAttribute("tabindex"))).toEqual(["0", "-1", "-1"]);
    expect(hits[1]?.getAttribute("role")).toBe("button");
    expect(hits[1]?.getAttribute("aria-label")).toBe("Oct: 120");
    hits[1]?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(h.core.calls.at(-1)).toEqual({ method: "fire", args: ["s1", "chart", "on_select", '{"row":1}'] });
    expect(h.messages().at(-1)).toMatchObject({ t: "ui.action", action_id: "act_pick", detail: { row: 1 } });
    expect(hits[1]?.getAttribute("aria-pressed")).toBe("true");
    hits[0]?.focus();
    hits[0]?.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true }));
    expect(document.activeElement).toBe(hits[1]);
    expect(hits.map((hit) => hit.getAttribute("tabindex"))).toEqual(["-1", "0", "-1"]);
    hits[1]?.dispatchEvent(new KeyboardEvent("keydown", { key: "End", bubbles: true }));
    expect(document.activeElement).toBe(hits[2]);
    hits[2]?.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    expect(h.core.calls.at(-1)).toEqual({ method: "fire", args: ["s1", "chart", "on_select", '{"row":2}'] });
  });

  it("does not redraw when the same scene arrives again", () => {
    const h = harness();
    playChart(h, true);
    const svg = h.find("svg");
    h.play({ surface: "s1", ops: [{ op: "update", id: "chart", props: { title: "Monthly spend", scene: scene as never } }] });
    expect(h.find("svg")).toBe(svg);
  });

  it("shows the title and an unavailable note without a scene", () => {
    const h = harness();
    playChart(h, false);
    const figure = h.find('[data-pui-id="chart"]');
    expect(figure.querySelector("svg")).toBeNull();
    expect(figure.querySelector("figcaption")?.textContent).toBe("Monthly spend");
    expect(figure.querySelector(".pui-chart-empty")?.textContent).toBe("Chart unavailable");
  });

  it("lays out a dashboard grid with clamped columns", () => {
    const h = harness();
    h.play({
      surface: "s1",
      ops: [
        { op: "create", id: "dash", type: "chart:Dashboard", props: { title: "November", columns: 9 } },
        { op: "create", id: "s", type: "Stat", props: { label: "Spent", value: "41 722" } },
        { op: "children", id: "dash", children: ["s"] },
        { op: "root", id: "dash" },
      ],
    });
    const grid = h.find('[data-pui-id="dash"] .pui-grid');
    expect(grid.getAttribute("data-columns")).toBe("4");
    expect(grid.firstElementChild?.getAttribute("data-pui-id")).toBe("s");
  });
});
