import type { Json, Props } from "./types";

let idCounter = 0;

export function uniqueId(prefix: string): string {
  idCounter += 1;
  return `pui-${prefix}-${idCounter}`;
}

export function element<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  className?: string,
  parent?: Element,
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (parent) parent.append(node);
  return node;
}

export function setText(node: Element, text: string): boolean {
  if (node.textContent === text) return false;
  node.textContent = text;
  return true;
}

export function setAttr(node: Element, name: string, value: string | null | undefined): void {
  if (value === null || value === undefined || value === "") {
    if (node.hasAttribute(name)) node.removeAttribute(name);
  } else if (node.getAttribute(name) !== value) {
    node.setAttribute(name, value);
  }
}

export function setShown(node: HTMLElement, shown: boolean): void {
  if (node.hidden === shown) node.hidden = !shown;
}

export function text(props: Props, key: string): string {
  const value = props[key];
  if (typeof value === "string") return value;
  if (typeof value === "number" && Number.isFinite(value)) return String(value);
  return "";
}

export function flag(props: Props, key: string): boolean {
  return props[key] === true;
}

export function choice<T extends string>(props: Props, key: string, allowed: readonly T[], fallback: T): T {
  const value = props[key];
  return allowed.find((item) => item === value) ?? fallback;
}

export function isRecord(value: Json | undefined): value is { [key: string]: Json } {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

export function syncChildren(parent: Element, desired: readonly Element[]): void {
  let cursor: ChildNode | null = parent.firstChild;
  for (const child of desired) {
    if (child === cursor) {
      cursor = cursor.nextSibling;
    } else {
      parent.insertBefore(child, cursor);
    }
  }
  while (cursor) {
    const next = cursor.nextSibling;
    cursor.remove();
    cursor = next;
  }
}

export function pulse(node: Element): void {
  node.setAttribute("data-pui-pulse", node.getAttribute("data-pui-pulse") === "a" ? "b" : "a");
}
