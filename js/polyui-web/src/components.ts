import { choice, element, flag, isRecord, pulse, setAttr, setShown, setText, syncChildren, text, uniqueId } from "./dom";
import type { ComponentRenderer, ComponentState, ComponentView, Json, Props } from "./types";

const textStyles = ["body", "title", "subtitle", "caption", "muted"] as const;
const cardTones = ["plain", "info", "success", "warning", "danger"] as const;
const gaps = ["none", "small", "medium", "large"] as const;
const aligns = ["start", "center", "end", "between"] as const;
const statTones = ["neutral", "positive", "negative", "warning"] as const;
const badgeTones = ["neutral", "info", "success", "warning", "danger"] as const;
const aspects = ["square", "wide", "tall", "auto"] as const;
const buttonTones = ["primary", "secondary", "danger"] as const;
const inputKinds = ["text", "number", "phone", "email", "multiline"] as const;

type InputKind = (typeof inputKinds)[number];

function headingLevel(style: string): string | null {
  if (style === "title") return "3";
  if (style === "subtitle") return "4";
  return null;
}

export const renderText: ComponentRenderer = () => {
  const node = element("p", "pui-text");
  return {
    element: node,
    update({ props }) {
      const style = choice(props, "style", textStyles, "body");
      const level = headingLevel(style);
      setAttr(node, "data-style", style);
      setAttr(node, "role", level ? "heading" : null);
      setAttr(node, "aria-level", level);
      setText(node, text(props, "text"));
    },
  };
};

export const renderCard: ComponentRenderer = () => {
  const node = element("div", "pui-card");
  const head = element("div", "pui-card-head", node);
  const title = element("h3", "pui-card-title", head);
  const subtitle = element("p", "pui-card-subtitle", head);
  const body = element("div", "pui-card-body", node);
  return {
    element: node,
    update({ props }) {
      setAttr(node, "data-tone", choice(props, "tone", cardTones, "plain"));
      setText(title, text(props, "title"));
      setText(subtitle, text(props, "subtitle"));
      setShown(title, title.textContent !== "");
      setShown(subtitle, subtitle.textContent !== "");
      setShown(head, !title.hidden || !subtitle.hidden);
    },
    setChildren(children) {
      syncChildren(body, children);
    },
  };
};

export const renderRow: ComponentRenderer = () => {
  const node = element("div", "pui-row");
  return {
    element: node,
    update({ props }) {
      setAttr(node, "data-gap", choice(props, "gap", gaps, "medium"));
      setAttr(node, "data-align", choice(props, "align", aligns, "start"));
    },
    setChildren(children) {
      syncChildren(node, children);
    },
  };
};

export const renderList: ComponentRenderer = () => {
  const node = element("div", "pui-list");
  const title = element("p", "pui-list-title", node);
  let list: HTMLUListElement | HTMLOListElement = element("ul", "pui-items", node);
  const wrappers = new WeakMap<Element, HTMLLIElement>();
  const wrap = (child: Element): HTMLLIElement => {
    let item = wrappers.get(child);
    if (!item) {
      item = element("li", "pui-item");
      item.setAttribute("data-pui-wrap", "");
      wrappers.set(child, item);
    }
    if (child.parentNode !== item) item.append(child);
    return item;
  };
  return {
    element: node,
    update({ props }) {
      const ordered = flag(props, "ordered");
      if (ordered !== (list.tagName === "OL")) {
        const next = element(ordered ? "ol" : "ul", "pui-items");
        next.append(...list.childNodes);
        list.replaceWith(next);
        list = next;
      }
      setText(title, text(props, "title"));
      setShown(title, title.textContent !== "");
      setAttr(list, "aria-label", text(props, "title"));
    },
    setChildren(children) {
      syncChildren(list, children.map(wrap));
    },
  };
};

export const renderStat: ComponentRenderer = () => {
  const node = element("div", "pui-stat");
  const label = element("span", "pui-stat-label", node);
  const figure = element("p", "pui-stat-figure", node);
  const value = element("span", "pui-stat-value", figure);
  const unit = element("span", "pui-stat-unit", figure);
  const hint = element("p", "pui-stat-hint", node);
  let drawn = false;
  return {
    element: node,
    update({ props }) {
      setAttr(node, "data-tone", choice(props, "tone", statTones, "neutral"));
      setText(label, text(props, "label"));
      const changed = setText(value, text(props, "value"));
      setText(unit, text(props, "unit"));
      setText(hint, text(props, "hint"));
      setShown(unit, unit.textContent !== "");
      setShown(hint, hint.textContent !== "");
      if (changed && drawn) pulse(figure);
      drawn = true;
    },
  };
};

export const renderBadge: ComponentRenderer = () => {
  const node = element("span", "pui-badge");
  let drawn = false;
  return {
    element: node,
    update({ props }) {
      setAttr(node, "data-tone", choice(props, "tone", badgeTones, "neutral"));
      if (setText(node, text(props, "text")) && drawn) pulse(node);
      drawn = true;
    },
  };
};

export const renderImage: ComponentRenderer = (api) => {
  const node = element("span", "pui-image");
  const missing = element("span", "pui-image-missing", node);
  let picture: HTMLImageElement | null = null;
  return {
    element: node,
    update({ props }) {
      setAttr(node, "data-aspect", choice(props, "aspect", aspects, "auto"));
      const alt = text(props, "alt");
      const src = api.safeUrl(props["src"]);
      setText(missing, alt);
      setShown(missing, src === null);
      if (src === null) {
        picture?.remove();
        picture = null;
        return;
      }
      if (!picture) {
        picture = element("img", "pui-img");
        picture.loading = "lazy";
        picture.decoding = "async";
        picture.referrerPolicy = "no-referrer";
        picture.addEventListener("error", () => {
          setShown(missing, true);
          picture?.setAttribute("hidden", "");
        });
        node.prepend(picture);
      }
      picture.alt = alt;
      if (picture.getAttribute("src") !== src) {
        picture.hidden = false;
        setShown(missing, false);
        picture.src = src;
      }
    },
  };
};

export const renderButton: ComponentRenderer = (api) => {
  const node = element("button", "pui-button");
  node.type = "button";
  const label = element("span", "pui-button-label", node);
  node.addEventListener("click", () => {
    if (!node.disabled) api.fire("on_press");
  });
  return {
    element: node,
    update({ props }) {
      setAttr(node, "data-tone", choice(props, "tone", buttonTones, "secondary"));
      setText(label, text(props, "label"));
      const disabled = flag(props, "disabled") || !isRecord(props["on_press"]);
      if (node.disabled !== disabled) node.disabled = disabled;
    },
  };
};

interface ChoiceOption {
  readonly id: string;
  readonly label: string;
  readonly detail: string;
}

interface OptionView {
  readonly row: HTMLLabelElement;
  readonly input: HTMLInputElement;
  readonly label: HTMLSpanElement;
  readonly detail: HTMLSpanElement;
}

function readOptions(props: Props): ChoiceOption[] {
  const raw = props["options"];
  if (!Array.isArray(raw)) return [];
  const seen = new Set<string>();
  const options: ChoiceOption[] = [];
  for (const item of raw) {
    if (!isRecord(item) || typeof item["id"] !== "string" || seen.has(item["id"])) continue;
    seen.add(item["id"]);
    options.push({ id: item["id"], label: text(item, "label"), detail: text(item, "detail") });
  }
  return options;
}

function selectedIds(value: Json | undefined): Set<string> {
  if (typeof value === "string") return new Set([value]);
  if (Array.isArray(value)) return new Set(value.filter((item): item is string => typeof item === "string"));
  return new Set();
}

export const renderChoiceGroup: ComponentRenderer = (api) => {
  const node = element("fieldset", "pui-choices");
  const legend = element("legend", "pui-legend-text", node);
  const body = element("div", "pui-options", node);
  const name = uniqueId("choice");
  const views = new Map<string, OptionView>();
  let props: Props = {};
  let order: ChoiceOption[] = [];

  body.addEventListener("change", () => {
    const path = props["value"];
    if (typeof path !== "string") return;
    const checked = order.filter((option) => views.get(option.id)?.input.checked).map((option) => option.id);
    const next: Json = flag(props, "multiple") ? checked : (checked[0] ?? null);
    api.setValue(path, next);
    if (isRecord(props["on_change"])) api.fire("on_change");
  });

  const viewFor = (option: ChoiceOption, multiple: boolean): OptionView => {
    let view = views.get(option.id);
    if (!view) {
      const row = element("label", "pui-option");
      const input = element("input", "pui-option-input", row);
      input.setAttribute("value", option.id);
      const copy = element("span", "pui-option-copy", row);
      view = { row, input, label: element("span", "pui-option-label", copy), detail: element("span", "pui-option-detail", copy) };
      views.set(option.id, view);
    }
    const type = multiple ? "checkbox" : "radio";
    if (view.input.type !== type) view.input.type = type;
    if (view.input.name !== name) view.input.name = name;
    setText(view.label, option.label);
    setText(view.detail, option.detail);
    setShown(view.detail, option.detail !== "");
    return view;
  };

  return {
    element: node,
    update(state: ComponentState) {
      props = state.props;
      const multiple = flag(props, "multiple");
      order = readOptions(props);
      setText(legend, text(props, "label"));
      setShown(legend, legend.textContent !== "");
      setAttr(node, "data-multiple", multiple ? "" : null);
      const keep = new Set(order.map((option) => option.id));
      for (const id of views.keys()) if (!keep.has(id)) views.delete(id);
      const rows = order.map((option) => viewFor(option, multiple));
      syncChildren(body, rows.map((view) => view.row));
      const selected = selectedIds(state.value);
      for (const option of order) {
        const view = views.get(option.id);
        const checked = selected.has(option.id);
        if (view && view.input.checked !== checked) view.input.checked = checked;
      }
    },
  };
};

const recentLimit = 8;

function inputType(kind: InputKind): string {
  if (kind === "phone") return "tel";
  if (kind === "email") return "email";
  return "text";
}

function displayValue(value: Json | undefined): string {
  if (typeof value === "string") return value;
  if (typeof value === "number" && Number.isFinite(value)) return String(value);
  return "";
}

export const renderInput: ComponentRenderer = (api) => {
  const node = element("div", "pui-field");
  const label = element("label", "pui-label", node);
  const controlId = uniqueId("input");
  label.htmlFor = controlId;
  let control: HTMLInputElement | HTMLTextAreaElement | null = null;
  let path: string | null = null;
  const recent: string[] = [];

  const onInput = (event: Event): void => {
    if (path === null || !(event.target instanceof HTMLInputElement || event.target instanceof HTMLTextAreaElement)) return;
    const value = event.target.value;
    recent.push(value);
    if (recent.length > recentLimit) recent.shift();
    api.setValue(path, value);
  };

  const ensureControl = (kind: InputKind): HTMLInputElement | HTMLTextAreaElement => {
    const wantsArea = kind === "multiline";
    if (control && (control instanceof HTMLTextAreaElement) === wantsArea) return control;
    const next = wantsArea ? element("textarea", "pui-control") : element("input", "pui-control");
    next.id = controlId;
    next.addEventListener("input", onInput);
    next.addEventListener("blur", () => {
      recent.length = 0;
    });
    if (control) {
      next.value = control.value;
      control.replaceWith(next);
    } else {
      node.append(next);
    }
    control = next;
    return next;
  };

  return {
    element: node,
    update({ props, value }) {
      const kind = choice(props, "input", inputKinds, "text");
      const field = ensureControl(kind);
      path = typeof props["value"] === "string" ? props["value"] : null;
      setText(label, text(props, "label"));
      setAttr(field, "placeholder", text(props, "placeholder"));
      const max = props["max_length"];
      setAttr(field, "maxlength", typeof max === "number" && max > 0 ? String(Math.floor(max)) : null);
      const required = flag(props, "required");
      if (field.required !== required) field.required = required;
      if (field instanceof HTMLInputElement) {
        const type = inputType(kind);
        if (field.type !== type) field.type = type;
        setAttr(field, "inputmode", kind === "number" ? "decimal" : kind === "phone" ? "tel" : kind === "email" ? "email" : null);
        setAttr(field, "autocomplete", kind === "phone" ? "tel" : kind === "email" ? "email" : "off");
      }
      setAttr(field, "spellcheck", kind === "email" || kind === "phone" || kind === "number" ? "false" : null);
      const desired = displayValue(value);
      if (field.value === desired) return;
      const focused = field.ownerDocument.activeElement === field;
      if (focused && recent.includes(desired)) return;
      if (!focused) {
        field.value = desired;
        return;
      }
      const start = field.selectionStart ?? desired.length;
      const end = field.selectionEnd ?? start;
      field.value = desired;
      field.setSelectionRange(Math.min(start, desired.length), Math.min(end, desired.length));
    },
  };
};

function actionKey(value: Json | undefined): string {
  return isRecord(value) && typeof value["action_id"] === "string" ? value["action_id"] : "";
}

export const renderConfirmSheet: ComponentRenderer = (api) => {
  const node = element("dialog", "pui-sheet");
  node.open = true;
  const titleId = uniqueId("sheet");
  node.setAttribute("aria-labelledby", titleId);
  const title = element("p", "pui-sheet-title", node);
  title.id = titleId;
  const body = element("p", "pui-sheet-body", node);
  const actions = element("div", "pui-actions", node);
  const cancel = element("button", "pui-button", actions);
  const confirm = element("button", "pui-button", actions);
  cancel.type = "button";
  confirm.type = "button";
  cancel.setAttribute("data-tone", "secondary");
  confirm.setAttribute("data-tone", "primary");
  let props: Props = {};
  let answeredFor: string | null = null;

  const answer = (prop: "on_confirm" | "on_cancel"): void => {
    if (answeredFor !== null) return;
    answeredFor = actionKey(props["on_confirm"]);
    node.setAttribute("data-answered", prop === "on_confirm" ? "confirm" : "cancel");
    confirm.disabled = true;
    cancel.disabled = true;
    if (isRecord(props[prop])) api.fire(prop);
  };
  confirm.addEventListener("click", () => answer("on_confirm"));
  cancel.addEventListener("click", () => answer("on_cancel"));

  return {
    element: node,
    update(state) {
      props = state.props;
      setText(title, text(props, "title"));
      setText(body, text(props, "body"));
      setShown(body, body.textContent !== "");
      setText(confirm, text(props, "confirm_label") || "Yes");
      setText(cancel, text(props, "cancel_label") || "No");
      if (answeredFor !== null && answeredFor !== actionKey(props["on_confirm"])) {
        answeredFor = null;
        node.removeAttribute("data-answered");
      }
      const locked = answeredFor !== null;
      confirm.disabled = locked || !isRecord(props["on_confirm"]);
      cancel.disabled = locked;
    },
  };
};

export const renderDivider: ComponentRenderer = () => ({
  element: element("hr", "pui-divider"),
  update() {},
});

export function renderFallback(label: string): ComponentView {
  const node = element("p", "pui-text");
  node.setAttribute("data-style", "muted");
  node.setAttribute("data-pui-fallback", "");
  node.textContent = label;
  return {
    element: node,
    update({ a11y }) {
      if (a11y?.label) setText(node, a11y.label);
    },
  };
}

export const coreRenderers: ReadonlyMap<string, ComponentRenderer> = new Map([
  ["Text", renderText],
  ["Card", renderCard],
  ["Row", renderRow],
  ["List", renderList],
  ["Stat", renderStat],
  ["Badge", renderBadge],
  ["Image", renderImage],
  ["Button", renderButton],
  ["ChoiceGroup", renderChoiceGroup],
  ["Input", renderInput],
  ["ConfirmSheet", renderConfirmSheet],
  ["Divider", renderDivider],
]);

export const nativeSemantics: ReadonlySet<string> = new Set([
  "Text",
  "List",
  "Image",
  "Button",
  "ChoiceGroup",
  "Input",
  "ConfirmSheet",
  "Divider",
  "chart:Chart",
]);

