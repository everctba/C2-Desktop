import styles from "./Input.module.css";

export interface InputProps {
  placeholder?: string;
  value?: string;
  label?: string;
  size?: "sm" | "md";
  mono?: boolean;
  onInput?: (value: string) => void;
  onEnter?: (value: string) => void;
}

/** Campo com label visível acima (ControlRow/FormField). */
export function Input(props: InputProps = {}): { root: HTMLElement; input: HTMLInputElement } {
  const root = document.createElement("label");
  root.className = styles.field;

  if (props.label) {
    const label = document.createElement("span");
    label.className = styles.label;
    label.textContent = props.label;
    root.appendChild(label);
  }

  const input = document.createElement("input");
  input.className = `${styles.input} ${styles[props.size ?? "sm"]}`;
  input.type = "text";
  input.placeholder = props.placeholder ?? "";
  if (props.value) input.value = props.value;
  if (props.mono) input.style.fontFamily = "var(--font-mono)";

  if (props.onInput) {
    input.addEventListener("input", () => props.onInput?.(input.value));
  }
  if (props.onEnter) {
    input.addEventListener("keydown", (event) => {
      if (event.key === "Enter") props.onEnter?.(input.value);
    });
  }

  root.appendChild(input);
  return { root, input };
}
