import { Button } from "./Button";
import { t } from "../i18n";
import { Pill } from "./Pill";
import styles from "./Toolbar.module.css";

export interface ToolbarProps {
  projectName: string;
  projectPath: string;
  running: boolean;
  busy: boolean;
  modelLoaded: boolean;
  vramMb: number | null;
  onPlay: () => void;
  onStop: () => void;
}

export function Toolbar(props: ToolbarProps): HTMLElement {
  const bar = document.createElement("header");
  bar.className = styles.toolbar;

  const project = document.createElement("div");
  project.className = styles.project;

  const name = document.createElement("span");
  name.className = styles.name;
  name.textContent = props.projectName || "nenhum projeto aberto";
  project.appendChild(name);

  if (props.projectPath) {
    const path = document.createElement("span");
    path.className = styles.path;
    path.textContent = props.projectPath;
    path.title = props.projectPath;
    project.appendChild(path);
  }
  bar.appendChild(project);

  bar.appendChild(
    Pill(t(props.running ? "engine rodando" : "engine parado"), props.running ? "success" : "neutral", "xs"),
  );

  if (props.modelLoaded) {
    const banner = document.createElement("div");
    banner.className = `${styles.modelBanner} ${props.running ? styles.modelBannerOn : styles.modelBannerIdle}`;
    banner.setAttribute("data-model-banner", "1");
    const titulo = document.createElement("span");
    titulo.textContent = t(props.running ? "Model on GPU" : "Model on GPU loaded");
    const vram = document.createElement("span");
    vram.setAttribute("data-model-vram", "1");
    vram.textContent =
      props.vramMb !== null ? `VRAM ${(props.vramMb / 1024).toFixed(1)} GB` : "VRAM —";
    banner.append(titulo, vram);
    bar.appendChild(banner);
  }

  const spacer = document.createElement("div");
  spacer.className = styles.spacer;
  bar.appendChild(spacer);

  bar.appendChild(
    Button({
      label: t(props.running ? "Parar" : "Iniciar"),
      iconName: props.running ? "stop" : "play",
      variant: props.running ? "warning" : "primary",
      busy: props.busy,
      disabled: !props.projectPath,
      onClick: props.running ? props.onStop : props.onPlay,
    }),
  );


  return bar;
}
