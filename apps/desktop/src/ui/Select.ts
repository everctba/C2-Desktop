import styles from "./Select.module.css";

export interface SelectProps {
  label: string;
  value: string;
  options: string[];
  onChange?: (value: string) => void;
}

/** Select nativo estilizado (acessível de graça). */
export function Select(props: SelectProps): { root: HTMLElement; select: HTMLSelectElement } {
  const wrapper = document.createElement("div");
  wrapper.className = styles.wrapper;

  const label = document.createElement("span");
  label.className = styles.label;
  label.textContent = props.label;

  const select = document.createElement("select");
  select.className = styles.select;
  select.setAttribute("aria-label", props.label);
  for (const option of props.options) {
    const item = document.createElement("option");
    item.value = option;
    item.textContent = option;
    if (option === props.value) item.selected = true;
    select.appendChild(item);
  }
  select.addEventListener("change", () => props.onChange?.(select.value));

  wrapper.append(label, select);
  return { root: wrapper, select };
}
