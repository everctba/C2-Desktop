import styles from "./Card.module.css";

export interface CardProps {
  title?: string;
  accent?: number;
  interactive?: boolean;
  onClick?: () => void;
}

/** Card sem borda: o fundo é --bg-surface-2 e o gap do grid é a divisória. */
export function Card(props: CardProps = {}): HTMLDivElement {
  const card = document.createElement("div");
  card.className = [styles.card, props.interactive ? styles.interactive : ""]
    .filter(Boolean)
    .join(" ");

  if (props.title) {
    const header = document.createElement("div");
    header.className = styles.header;

    const title = document.createElement("span");
    title.className = "widgetTitle";
    if (props.accent) {
      const dot = document.createElement("span");
      dot.className = "dot";
      dot.style.background = `var(--accent-${props.accent})`;
      title.appendChild(dot);
    }
    title.appendChild(document.createTextNode(props.title));
    header.appendChild(title);
    card.appendChild(header);
  }

  if (props.onClick) card.addEventListener("click", props.onClick);
  return card;
}

export function cardBody(card: HTMLElement): HTMLElement {
  const existing = card.querySelector<HTMLElement>(`.${styles.body}`);
  if (existing) return existing;
  const body = document.createElement("div");
  body.className = styles.body;
  card.appendChild(body);
  return body;
}

export function cardFooter(card: HTMLElement): HTMLElement {
  const existing = card.querySelector<HTMLElement>(`.${styles.footer}`);
  if (existing) return existing;
  const footer = document.createElement("div");
  footer.className = styles.footer;
  card.appendChild(footer);
  return footer;
}
