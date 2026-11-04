import { icon, type IconName } from "../icons/index";
import styles from "./Button.module.css";

export type ButtonVariant = "primary" | "secondary" | "ghost" | "destructive";
export type ButtonSize = "micro" | "xs" | "sm" | "md" | "lg";

export interface ButtonProps {
  label?: string;
  iconName?: IconName;
  variant?: ButtonVariant;
  size?: ButtonSize;
  iconOnly?: boolean;
  busy?: boolean;
  disabled?: boolean;
  onClick?: () => void;
}

export function Button(props: ButtonProps): HTMLButtonElement {
  const size = props.size ?? "sm";
  const button = document.createElement("button");
  button.type = "button";
  button.className = [
    styles.btn,
    styles[props.variant ?? "secondary"],
    styles[size],
    props.iconOnly ? styles.iconOnly : "",
  ]
    .filter(Boolean)
    .join(" ");

  if (props.disabled) button.disabled = true;
  if (props.busy) {
    button.setAttribute("aria-busy", "true");
    button.disabled = true;
  }
  if (props.iconOnly && !props.label) {
    button.setAttribute("aria-label", props.iconName ?? "acao");
  }

  if (props.iconName) {
    const span = document.createElement("span");
    span.innerHTML = icon(props.iconName, size === "md" || size === "lg" ? 16 : 14);
    if (props.busy) span.classList.add(styles.spinning);
    button.appendChild(span);
  }
  if (props.label) {
    const text = document.createElement("span");
    text.textContent = props.label;
    button.appendChild(text);
  }
  if (props.onClick) button.addEventListener("click", props.onClick);
  return button;
}
