import styles from "./Toggle.module.css";

export interface ToggleProps {
  label: string;
  checked: boolean;
  /** "brand" usa o verde da marca (accent-1) quando ligado. */
  tone?: "brand";
  onChange?: (checked: boolean) => void;
}

/** Switch binário do design system (§8.4). */
export function Toggle(props: ToggleProps): { root: HTMLElement; control: HTMLButtonElement } {
  const wrapper = document.createElement("div");
  wrapper.className = styles.wrapper;

  const label = document.createElement("span");
  label.className = styles.label;
  label.textContent = props.label;

  const control = document.createElement("button");
  control.type = "button";
  const classes = (ligado: boolean) =>
    [
      styles.control,
      ligado ? styles.on : "",
      ligado && props.tone === "brand" ? styles.onBrand : "",
    ]
      .filter(Boolean)
      .join(" ");
  control.className = classes(props.checked);
  control.setAttribute("role", "switch");
  control.setAttribute("aria-checked", props.checked ? "true" : "false");
  control.setAttribute("aria-label", props.label);

  control.addEventListener("click", () => {
    const next = control.getAttribute("aria-checked") !== "true";
    control.setAttribute("aria-checked", next ? "true" : "false");
    control.className = classes(next);
    props.onChange?.(next);
  });

  wrapper.append(label, control);
  return { root: wrapper, control };
}
