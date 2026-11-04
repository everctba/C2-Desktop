import { Pill, type PillTone } from "./Pill";
import styles from "./TreeRow.module.css";

export interface TreeRowProps {
  label: string;
  secondary?: string;
  badge?: { text: string; tone: PillTone };
  selected?: boolean;
  onClick?: () => void;
}

/** Linha de lista/árvore. Densidade de ferramenta, hover só muda cor. */
export function TreeRow(props: TreeRowProps): HTMLDivElement {
  const row = document.createElement("div");
  row.className = `${styles.row} ${props.selected ? styles.selected : ""}`.trim();
  row.tabIndex = 0;
  row.setAttribute("role", "treeitem");
  row.setAttribute("aria-selected", props.selected ? "true" : "false");

  const label = document.createElement("span");
  label.className = styles.primary;
  label.textContent = props.label;
  label.title = props.label;
  row.appendChild(label);

  if (props.badge) {
    const badge = Pill(props.badge.text, props.badge.tone, "xs");
    badge.classList.add(styles.badge);
    row.appendChild(badge);
  }

  if (props.secondary) {
    const secondary = document.createElement("span");
    secondary.className = styles.secondary;
    secondary.textContent = props.secondary;
    row.appendChild(secondary);
  }

  if (props.onClick) {
    row.addEventListener("click", props.onClick);
    row.addEventListener("keydown", (event) => {
      if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        props.onClick?.();
      }
    });
  }
  return row;
}
