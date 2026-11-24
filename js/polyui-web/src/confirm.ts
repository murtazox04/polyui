import { element, setShown, uniqueId } from "./dom";
import type { ConfirmOp, Labels } from "./types";

export interface ConfirmHandle {
  close(): void;
}

export interface ConfirmHooks {
  readonly labels: Labels;
  readonly delayMs: number;
  readonly onConfirm: () => void;
  readonly onDismiss: () => void;
}

export function openConfirm(op: ConfirmOp, hooks: ConfirmHooks): ConfirmHandle {
  const dialog = element("dialog", "pui-confirm");
  const panel = element("div", "pui-confirm-panel", dialog);
  const title = element("h2", "pui-confirm-title", panel);
  const body = element("p", "pui-confirm-body", panel);
  const status = element("p", "pui-confirm-status", panel);
  const timer = element("div", "pui-confirm-timer", panel);
  element("span", "pui-confirm-bar", timer);
  const actions = element("div", "pui-actions", panel);
  const cancel = element("button", "pui-button", actions);
  const confirm = element("button", "pui-button", actions);

  title.id = uniqueId("confirm");
  body.id = uniqueId("confirm");
  dialog.setAttribute("aria-labelledby", title.id);
  dialog.setAttribute("aria-describedby", body.id);
  status.setAttribute("role", "status");
  cancel.type = "button";
  confirm.type = "button";
  cancel.setAttribute("data-tone", "secondary");
  confirm.setAttribute("data-tone", "primary");
  cancel.autofocus = true;

  title.textContent = typeof op.title === "string" ? op.title : "";
  body.textContent = typeof op.body === "string" ? op.body : "";
  setShown(body, body.textContent !== "");
  setShown(status, false);
  cancel.textContent = hooks.labels.cancel;
  confirm.textContent = typeof op.confirm_label === "string" && op.confirm_label !== "" ? op.confirm_label : "OK";

  const timers: ReturnType<typeof setTimeout>[] = [];
  let open = true;

  const close = (): void => {
    if (!open) return;
    open = false;
    for (const id of timers) clearTimeout(id);
    if (dialog.open) dialog.close();
    dialog.remove();
  };

  const dismiss = (): void => {
    if (!open) return;
    close();
    hooks.onDismiss();
  };

  confirm.addEventListener("click", () => {
    if (!open || confirm.disabled) return;
    close();
    hooks.onConfirm();
  });
  cancel.addEventListener("click", dismiss);
  dialog.addEventListener("cancel", (event) => {
    event.preventDefault();
    dismiss();
  });
  dialog.addEventListener("click", (event) => {
    if (event.target === dialog) dismiss();
  });

  if (hooks.delayMs > 0) {
    confirm.disabled = true;
    timers.push(
      setTimeout(() => {
        if (!dialog.hasAttribute("data-expired")) confirm.disabled = false;
      }, hooks.delayMs),
    );
  }

  const expires = op.expires_ms;
  if (typeof expires === "number" && Number.isFinite(expires) && expires > 0) {
    dialog.style.setProperty("--pui-expire", `${Math.round(expires)}ms`);
    dialog.setAttribute("data-timed", "");
    timers.push(
      setTimeout(() => {
        dialog.setAttribute("data-expired", "");
        confirm.disabled = true;
        status.textContent = hooks.labels.expired;
        setShown(status, true);
      }, expires),
    );
  }

  document.body.append(dialog);
  if (typeof dialog.showModal === "function") dialog.showModal();
  else dialog.setAttribute("open", "");
  return { close };
}
