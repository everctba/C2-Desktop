import styles from "./Pill.module.css";

export type PillTone = "neutral" | "success" | "warning" | "danger" | "accent";

/**
 * Accent determinístico por string (§3.2): a mesma tag sempre recebe a mesma
 * cor, sem estado global.
 */
export function accentFor(seed: string): number {
  let hash = 0;
  for (let i = 0; i < seed.length; i++) {
    hash = (hash * 31 + seed.charCodeAt(i)) >>> 0;
  }
  return (hash % 10) + 1;
}

export function Pill(text: string, tone: PillTone = "neutral", size: "xs" | "sm" = "sm"): HTMLSpanElement {
  const pill = document.createElement("span");
  pill.className = `${styles.pill} ${styles[size]}`;
  pill.textContent = text;

  if (tone === "accent") {
    const accent = accentFor(text);
    pill.style.setProperty("--pill-bg", `var(--accent-${accent}-10)`);
    pill.style.setProperty("--pill-color", `var(--accent-${accent})`);
  } else {
    pill.classList.add(styles[tone]);
  }
  return pill;
}
