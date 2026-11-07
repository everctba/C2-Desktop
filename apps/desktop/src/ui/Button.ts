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
  title?: string;
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
  if (props.title) button.title = props.title;
  if (props.busy) {
    button.setAttribute("aria-busy", "true");
    button.disabled = true;
  }
  if (props.iconOnly && !props.label) {
    button.setAttribute("aria-label", props.iconName ?? "acao");
  }

  if (props.iconName) {
    const tamanho = size === "md" || size === "lg" ? 16 : 12;
    const span = document.createElement("span");
    span.className = styles.icon;
    span.style.width = `${tamanho}px`;
    span.style.height = `${tamanho}px`;
    span.innerHTML = icon(props.iconName, tamanho);
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
