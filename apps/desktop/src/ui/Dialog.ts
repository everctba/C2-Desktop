import { Button } from "./Button";
import styles from "./Dialog.module.css";

export interface DialogOptions {
  title: string;
  body: HTMLElement;
  confirmLabel?: string;
  cancelLabel?: string;
  destructive?: boolean;
}

export interface DialogHandle {
  element: HTMLElement;
  body: HTMLElement;
  footer: HTMLElement;
  /** Fecha o dialog (dispara `onClose`). */
  close: () => void;
}

interface CreateOptions {
  title: string;
  onClose?: () => void;
}

/**
 * Cria um dialog modal (§8.5) sem botões prontos — o chamador monta o rodapé.
 * Escape e clique no backdrop fecham.
 */
export function createDialog(options: CreateOptions): DialogHandle {
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
  dialog.appendChild(body);

  const footer = document.createElement("div");
  footer.className = styles.footer;
  dialog.appendChild(footer);

  const onKey = (event: KeyboardEvent) => {
    if (event.key === "Escape") close();
  };

  const close = () => {
    document.removeEventListener("keydown", onKey);
    backdrop.remove();
    options.onClose?.();
  };

  document.addEventListener("keydown", onKey);
  backdrop.addEventListener("click", (event) => {
    if (event.target === backdrop) close();
  });

  backdrop.appendChild(dialog);
  document.body.appendChild(backdrop);

  return { element: dialog, body, footer, close };
}

/** Dialog de confirmação: resolve `true` no confirmar e `false` ao cancelar. */
export function openDialog(options: DialogOptions): Promise<boolean> {
  return new Promise((resolve) => {
    let settled = false;
    const handle = createDialog({
      title: options.title,
      onClose: () => {
        if (!settled) {
          settled = true;
          resolve(false);
        }
      },
    });

    handle.body.appendChild(options.body);

    const finish = (value: boolean) => {
      if (settled) return;
      settled = true;
      handle.close();
      resolve(value);
    };

    if (options.cancelLabel !== "") {
      handle.footer.appendChild(
        Button({
          label: options.cancelLabel ?? "Cancelar",
          variant: "ghost",
          onClick: () => finish(false),
        }),
      );
    }
    const confirm = Button({
      label: options.confirmLabel ?? "Confirmar",
      variant: options.destructive ? "destructive" : "primary",
      onClick: () => finish(true),
    });
    handle.footer.appendChild(confirm);
    confirm.focus();
  });
}

/** Linha label/valor usada nos dialogs (label 10px uppercase). */
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
