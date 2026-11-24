import { element, isRecord, setAttr, setShown, setText, syncChildren, text } from "./dom";
import type { ChartHit, ChartMark, ChartPathCommand, ChartScene, ComponentRenderer, Json } from "./types";

const svgNamespace = "http://www.w3.org/2000/svg";
const colorToken = /^[a-z][a-z0-9-]{0,31}$/;
const textRoles = new Set(["title", "axis_title", "tick", "legend", "value", "note"]);
const markLimit = 6000;

function svg<K extends keyof SVGElementTagNameMap>(tag: K, parent?: Element): SVGElementTagNameMap[K] {
  const node = document.createElementNS(svgNamespace, tag);
  if (parent) parent.append(node);
  return node;
}

function finite(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value);
}

function format(value: number): string {
  return String(Math.round(value * 100) / 100);
}

function setNumbers(node: Element, values: Record<string, unknown>): boolean {
  for (const [name, value] of Object.entries(values)) {
    if (value === undefined) continue;
    if (!finite(value)) return false;
    node.setAttribute(name, format(value));
  }
  return true;
}

export function tokenColor(token: unknown): string | null {
  if (typeof token !== "string" || !colorToken.test(token)) return null;
  return token.startsWith("series-") ? `var(--pui-${token}, currentColor)` : `var(--pui-chart-${token}, currentColor)`;
}

function paint(node: SVGElement | HTMLElement, property: string, token: unknown): void {
  const color = tokenColor(token);
  if (color) node.style.setProperty(property, color);
  else if (property === "fill" || property === "stroke") node.style.setProperty(property, "none");
}

function pathData(commands: unknown): string | null {
  if (!Array.isArray(commands) || commands.length === 0) return null;
  const parts: string[] = [];
  for (const command of commands as readonly ChartPathCommand[]) {
    if (!isRecord(command as unknown as Json)) return null;
    if (command.op === "Z") {
      parts.push("Z");
    } else if (command.op === "M" || command.op === "L") {
      if (!finite(command.x) || !finite(command.y)) return null;
      parts.push(`${command.op}${format(command.x)} ${format(command.y)}`);
    } else if (command.op === "A") {
      if (!finite(command.radius) || !finite(command.x) || !finite(command.y)) return null;
      const radius = format(Math.abs(command.radius));
      parts.push(`A${radius} ${radius} 0 ${command.large_arc === true ? 1 : 0} ${command.clockwise === true ? 1 : 0} ${format(command.x)} ${format(command.y)}`);
    } else {
      return null;
    }
  }
  return parts.join("");
}

function readScene(value: Json | undefined): ChartScene | null {
  if (!isRecord(value)) return null;
  const box = value["view_box"];
  if (!isRecord(box) || !Array.isArray(value["marks"])) return null;
  const { x, y, width, height } = box;
  if (!finite(x) || !finite(y) || !finite(width) || !finite(height) || width <= 0 || height <= 0) return null;
  return value as unknown as ChartScene;
}

function drawMark(mark: ChartMark, parent: SVGGElement): void {
  if (!isRecord(mark as unknown as Json)) return;
  switch (mark.type) {
    case "rect": {
      const node = svg("rect");
      if (!finite(mark.width) || !finite(mark.height)) return;
      if (!setNumbers(node, { x: mark.x, y: mark.y, width: Math.max(0, mark.width), height: Math.max(0, mark.height) })) return;
      paint(node, "fill", mark.fill);
      parent.append(node);
      return;
    }
    case "line": {
      const node = svg("line");
      if (!setNumbers(node, { x1: mark.x1, y1: mark.y1, x2: mark.x2, y2: mark.y2, "stroke-width": mark.stroke_width ?? 1 })) return;
      paint(node, "stroke", mark.stroke);
      parent.append(node);
      return;
    }
    case "path": {
      const data = pathData(mark.commands);
      if (data === null) return;
      const node = svg("path");
      node.setAttribute("d", data);
      if (!setNumbers(node, { "stroke-width": mark.stroke_width, "fill-opacity": mark.fill_opacity })) return;
      paint(node, "fill", mark.fill);
      paint(node, "stroke", mark.stroke);
      parent.append(node);
      return;
    }
    case "circle": {
      const node = svg("circle");
      if (!finite(mark.r) || !setNumbers(node, { cx: mark.cx, cy: mark.cy, r: Math.max(0, mark.r) })) return;
      paint(node, "fill", mark.fill);
      parent.append(node);
      return;
    }
    case "text": {
      const node = svg("text");
      if (!setNumbers(node, { x: mark.x, y: mark.y, "font-size": mark.size })) return;
      node.setAttribute("text-anchor", mark.anchor === "middle" || mark.anchor === "end" ? mark.anchor : "start");
      if (textRoles.has(mark.role)) node.setAttribute("class", `pui-mark-${mark.role.replace("_", "-")}`);
      paint(node, "fill", mark.color);
      node.textContent = typeof mark.text === "string" ? mark.text : "";
      parent.append(node);
      return;
    }
  }
}

function drawHit(hit: ChartHit, parent: SVGGElement): SVGElement | null {
  if (!finite(hit.row) || hit.row < 0) return null;
  let node: SVGElement;
  if (hit.shape === "polygon") {
    if (!Array.isArray(hit.points) || hit.points.length < 3) return null;
    const points: string[] = [];
    for (const point of hit.points) {
      if (!Array.isArray(point) || !finite(point[0]) || !finite(point[1])) return null;
      points.push(`${format(point[0])},${format(point[1])}`);
    }
    node = svg("polygon");
    node.setAttribute("points", points.join(" "));
  } else if (hit.shape === "rect") {
    node = svg("rect");
    if (!setNumbers(node, { x: hit.x, y: hit.y, width: hit.width, height: hit.height })) return null;
  } else {
    return null;
  }
  node.setAttribute("class", "pui-hit");
  node.setAttribute("data-row", String(Math.floor(hit.row)));
  node.setAttribute("role", "button");
  node.setAttribute("tabindex", "-1");
  node.setAttribute("aria-pressed", "false");
  node.setAttribute("aria-label", typeof hit.label === "string" ? hit.label : "");
  parent.append(node);
  return node;
}

export function createChartRenderer(unavailable: () => string): ComponentRenderer {
  return (api) => {
    const node = element("figure", "pui-chart");
    const caption = element("figcaption", "pui-chart-title", node);
    const stage = element("div", "pui-chart-stage", node);
    const empty = element("p", "pui-chart-empty");
    const legend = element("ul", "pui-legend", node);
    let drawnScene = "";
    let selectable = false;
    let selected = -1;
    let hits: SVGElement[] = [];

    const rowOf = (target: EventTarget | null): number => {
      if (!(target instanceof Element)) return -1;
      const hit = target.closest("[data-row]");
      return hit ? Number(hit.getAttribute("data-row")) : -1;
    };

    const markSelected = (): void => {
      for (const hit of hits) hit.setAttribute("aria-pressed", String(Number(hit.getAttribute("data-row")) === selected));
    };

    const activate = (row: number): void => {
      if (row < 0 || !selectable) return;
      selected = row;
      markSelected();
      api.fire("on_select", { row });
    };

    const moveFocus = (from: SVGElement, step: number | "first" | "last"): void => {
      const index = hits.indexOf(from);
      const nextIndex = step === "first" ? 0 : step === "last" ? hits.length - 1 : Math.min(hits.length - 1, Math.max(0, index + step));
      const next = hits[nextIndex];
      if (!next || next === from) return;
      from.setAttribute("tabindex", "-1");
      next.setAttribute("tabindex", "0");
      next.focus();
    };

    stage.addEventListener("click", (event) => activate(rowOf(event.target)));
    stage.addEventListener("keydown", (event) => {
      const target = event.target;
      if (!(target instanceof SVGElement) || !target.hasAttribute("data-row")) return;
      const steps: Record<string, number | "first" | "last"> = { ArrowRight: 1, ArrowDown: 1, ArrowLeft: -1, ArrowUp: -1, Home: "first", End: "last" };
      const step = steps[event.key];
      if (step !== undefined) {
        event.preventDefault();
        moveFocus(target, step);
      } else if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        activate(rowOf(target));
      }
    });

    const draw = (scene: ChartScene, title: string): void => {
      const focusedRow = rowOf(node.ownerDocument.activeElement);
      const canvas = svg("svg");
      canvas.setAttribute("class", "pui-chart-svg");
      const box = scene.view_box;
      canvas.setAttribute("viewBox", [box.x, box.y, box.width, box.height].map(format).join(" "));
      canvas.setAttribute("role", "group");
      canvas.setAttribute("aria-label", title);
      if (typeof scene.description === "string" && scene.description !== "") {
        const description = svg("desc", canvas);
        description.textContent = scene.description;
      }
      const marks = svg("g", canvas);
      marks.setAttribute("aria-hidden", "true");
      for (const mark of scene.marks.slice(0, markLimit)) drawMark(mark, marks);
      const layer = svg("g", canvas);
      layer.setAttribute("class", "pui-hits");
      hits = [];
      for (const hit of (scene.hits ?? []).slice(0, markLimit)) {
        const drawn = isRecord(hit as unknown as Json) ? drawHit(hit, layer) : null;
        if (drawn) hits.push(drawn);
      }
      const first = hits[0];
      if (first) first.setAttribute("tabindex", "0");
      markSelected();
      syncChildren(stage, [canvas]);
      const refocus = hits.find((hit) => Number(hit.getAttribute("data-row")) === focusedRow);
      if (refocus) {
        first?.setAttribute("tabindex", "-1");
        refocus.setAttribute("tabindex", "0");
        refocus.focus();
      }
      const items = (scene.legend ?? []).filter((item) => isRecord(item as unknown as Json) && typeof item.label === "string");
      syncChildren(
        legend,
        items.map((item) => {
          const row = element("li", "pui-legend-item");
          const swatch = element("span", "pui-swatch", row);
          paint(swatch, "--pui-swatch", item.color);
          row.append(item.label);
          return row;
        }),
      );
    };

    return {
      element: node,
      update({ props }) {
        const title = text(props, "title");
        setText(caption, title);
        selectable = isRecord(props["on_select"]);
        setAttr(node, "data-selectable", selectable ? "" : null);
        const scene = readScene(props["scene"]);
        if (!scene) {
          drawnScene = "";
          hits = [];
          setText(empty, unavailable());
          syncChildren(stage, [empty]);
          syncChildren(legend, []);
          setShown(legend, false);
          return;
        }
        const serialized = JSON.stringify(scene);
        if (serialized !== drawnScene) {
          drawnScene = serialized;
          draw(scene, title);
        }
        setShown(legend, legend.childElementCount > 0);
      },
    };
  };
}

export const renderDashboard: ComponentRenderer = () => {
  const node = element("div", "pui-dashboard");
  const title = element("h3", "pui-dashboard-title", node);
  const grid = element("div", "pui-grid", node);
  return {
    element: node,
    update({ props }) {
      setText(title, text(props, "title"));
      setShown(title, title.textContent !== "");
      const columns = props["columns"];
      const count = typeof columns === "number" && Number.isFinite(columns) ? Math.min(4, Math.max(1, Math.round(columns))) : 2;
      setAttr(grid, "data-columns", String(count));
    },
    setChildren(children) {
      syncChildren(grid, children);
    },
  };
};
