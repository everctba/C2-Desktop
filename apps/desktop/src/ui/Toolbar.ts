import { Button } from "./Button";
import { t } from "../i18n";
import { Pill } from "./Pill";
import styles from "./Toolbar.module.css";

export interface ToolbarProps {
  projectName: string;
  projectPath: string;
  running: boolean;
  busy: boolean;
  onPlay: () => void;
  onStop: () => void;
  onReindex: () => void;
  onPickProject: () => void;
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

  const spacer = document.createElement("div");
  spacer.className = styles.spacer;
  bar.appendChild(spacer);

  bar.appendChild(
    Button({
      label: t(props.running ? "Parar" : "Iniciar"),
      iconName: props.running ? "stop" : "play",
      variant: props.running ? "secondary" : "primary",
      busy: props.busy,
      disabled: !props.projectPath,
      onClick: props.running ? props.onStop : props.onPlay,
    }),
  );

  bar.appendChild(
    Button({
      label: t("Reindexar"),
      iconName: "refresh",
      variant: "ghost",
      disabled: !props.projectPath || props.busy,
      onClick: props.onReindex,
    }),
  );

  bar.appendChild(
    Button({
      label: t("Abrir projeto"),
      variant: "ghost",
      iconName: "projects",
      onClick: props.onPickProject,
    }),
  );

  return bar;
}
