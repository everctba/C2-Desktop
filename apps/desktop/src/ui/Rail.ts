import { icon, type IconName } from "../icons/index";
import styles from "./Rail.module.css";

export type Route = "projects" | "engine" | "search" | "mcp" | "settings";

export interface RailProps {
  active: Route;
  onNavigate: (route: Route) => void;
}

const ITEMS: Array<{ route: Route; icon: IconName; label: string }> = [
  { route: "projects", icon: "projects", label: "Projetos" },
  { route: "engine", icon: "engine", label: "Engine" },
  { route: "search", icon: "search", label: "Busca" },
  { route: "mcp", icon: "mcp", label: "MCP" },
  { route: "settings", icon: "settings", label: "Config" },
];

export function Rail(props: RailProps): HTMLElement {
  const rail = document.createElement("nav");
  rail.className = styles.rail;
  rail.setAttribute("aria-label", "Navegação principal");

  for (const item of ITEMS) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = `${styles.item} ${props.active === item.route ? styles.active : ""}`.trim();
    button.setAttribute("aria-current", props.active === item.route ? "page" : "false");
    button.innerHTML = icon(item.icon, 18);

    const label = document.createElement("span");
    label.className = styles.label;
    label.textContent = item.label;
    button.appendChild(label);

    button.addEventListener("click", () => props.onNavigate(item.route));
    rail.appendChild(button);
  }

  const spacer = document.createElement("div");
  spacer.className = styles.spacer;
  rail.appendChild(spacer);
  return rail;
}
