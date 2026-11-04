import { Button } from "./Button";
import styles from "./Dialog.module.css";

export interface DialogOptions {
  title: string;
  body: HTMLElement;
  confirmLabel?: string;
  cancelLabel?: string;
  destructive?: boolean;
}

/**
 * Dialog modal (§8.5). Resolve `true` no confirmar e `false` no cancelar
 * (botão, Escape ou clique no backdrop).
 */
export function openDialog(options: DialogOptions): Promise<boolean> {
  return new Promise((resolve) => {
    const backdrop = document.createElement("div");
    backdrop.className = styles.backdrop;
    backdrop.setAttribute("role", "presentation");

    const dialog = document.createElement("div");
    dialog.className = styles.dialog;
    dialog.setAttribute("role", "dialog");
    dialog.setAttribute("aria-modal", "true");
    dialog.setAttribute("aria-label", options.title);

    const header = document.createElement("div");
    header.className = styles.header;
    const title = document.createElement("span");
    title.className = styles.title;
    title.textContent = options.title;
    header.appendChild(title);
    dialog.appendChild(header);

    const body = document.createElement("div");
    body.className = styles.body;
    body.appendChild(options.body);
    dialog.appendChild(body);

    const footer = document.createElement("div");
    footer.className = styles.footer;

    const close = (value: boolean) => {
      document.removeEventListener("keydown", onKey);
      backdrop.remove();
      resolve(value);
    };

    const cancel = Button({
      label: options.cancelLabel ?? "Cancelar",
      variant: "ghost",
      onClick: () => close(false),
    });
    const confirm = Button({
      label: options.confirmLabel ?? "Confirmar",
      variant: options.destructive ? "destructive" : "primary",
      onClick: () => close(true),
    });
    footer.append(cancel, confirm);
    dialog.appendChild(footer);

    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") close(false);
    };
    document.addEventListener("keydown", onKey);
    backdrop.addEventListener("click", (event) => {
      if (event.target === backdrop) close(false);
    });

    backdrop.appendChild(dialog);
    document.body.appendChild(backdrop);
    confirm.focus();
  });
}

/** Linha label/valor usada nos dialogs (design system: label 10px uppercase). */
export function dialogField(label: string, value: string, mono = false): HTMLElement {
  const field = document.createElement("div");
  field.className = styles.field;
  const labelEl = document.createElement("span");
  labelEl.className = styles.fieldLabel;
  labelEl.textContent = label;
  const valueEl = document.createElement("span");
  valueEl.className = mono ? styles.mono : styles.fieldValue;
  valueEl.textContent = value;
  field.append(labelEl, valueEl);
  return field;
}

export function dialogPills(): HTMLElement {
  const pills = document.createElement("div");
  pills.className = styles.pills;
  return pills;
}
