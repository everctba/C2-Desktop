import { icon, type IconName } from "../icons/index";
import { t } from "../i18n";
import styles from "./Rail.module.css";

export type Route =
  | "projects"
  | "engine"
  | "search"
  | "memory"
  | "forest"
  | "mcp"
  | "settings"
  | "about";

export interface RailProps {
  active: Route;
  onNavigate: (route: Route) => void;
}

interface Item {
  route: Route;
  icon: IconName;
  label: string;
}

const ITEMS: Item[] = [
  { route: "projects", icon: "projects", label: "Projetos" },
  { route: "engine", icon: "engine", label: "Engine" },
  { route: "search", icon: "search", label: "Busca" },
  { route: "memory", icon: "note", label: "Memória" },
  { route: "forest", icon: "forest", label: "Floresta" },
  { route: "mcp", icon: "mcp", label: "MCP" },
  { route: "settings", icon: "settings", label: "Config" },
];

const SOBRE: Item = { route: "about", icon: "info", label: "Sobre" };

export function Rail(props: RailProps): HTMLElement {
  const rail = document.createElement("nav");
  rail.className = styles.rail;
  rail.setAttribute("aria-label", "Navegação principal");

  for (const item of ITEMS) rail.appendChild(criarItem(item, props));

  const spacer = document.createElement("div");
  spacer.className = styles.spacer;
  rail.appendChild(spacer);

  // "Sobre" fica no rodape do rail, separado da navegacao principal.
  rail.appendChild(criarItem(SOBRE, props));
  return rail;
}

function criarItem(item: Item, props: RailProps): HTMLButtonElement {
  const button = document.createElement("button");
  button.type = "button";
  button.className = `${styles.item} ${props.active === item.route ? styles.active : ""}`.trim();
  button.setAttribute("aria-current", props.active === item.route ? "page" : "false");
  button.innerHTML = icon(item.icon, 18);

  const label = document.createElement("span");
  label.className = styles.label;
  label.textContent = t(item.label);
  button.appendChild(label);

  button.addEventListener("click", () => props.onNavigate(item.route));
  return button;
}
