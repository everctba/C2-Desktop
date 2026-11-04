import "./styles/global.css";
import { api, hasTauri, onEngineEvent } from "./api";
import { Rail, type Route } from "./ui/Rail";
import { Toolbar } from "./ui/Toolbar";
import type { StatusReport } from "./types";
import { renderEngine } from "./views/Engine";
import { renderProjects } from "./views/Projects";
import { renderSearch } from "./views/Search";

const app = document.getElementById("app")!;

interface AppState {
  root: string;
  route: Route;
  status: StatusReport | null;
  running: boolean;
  busy: boolean;
  logs: string[];
}

const state: AppState = {
  root: localStorage.getItem("c2.root") ?? "",
  route: "projects",
  status: null,
  running: false,
  busy: false,
  logs: [],
};

function log(message: string): void {
  const time = new Date().toLocaleTimeString("pt-BR");
  state.logs.unshift(`${time} ${message}`);
  state.logs = state.logs.slice(0, 40);
}

async function loadStatus(): Promise<void> {
  if (!state.root) {
    state.status = null;
    return;
  }
  try {
    state.status = await api.status(state.root);
    state.running = await api.engineRunning();
  } catch {
    state.status = null;
  }
}

async function pickProject(): Promise<void> {
  const path = await api.pickFolder();
  if (!path) return;
  state.root = path.replace(/\\/g, "/");
  localStorage.setItem("c2.root", state.root);
  await loadStatus();
  if (!state.status) state.route = "projects";
  render();
}

async function runReindex(full: boolean): Promise<void> {
  if (!state.root) return;
  state.busy = true;
  render();
  log(full ? "reindex completo iniciado" : "reindex (limite 10) iniciado");
  try {
    const report = await api.reindex(state.root, full, full ? undefined : 10);
    log(
      `reindex: ${report.indexed} indexados, ${report.unchanged} inalterados, ` +
        `${report.errors} erros, ${report.blocked} bloqueados por segredo`,
    );
  } catch (error) {
    log(`erro: ${String(error)}`);
  } finally {
    state.busy = false;
    await loadStatus();
    render();
  }
}

async function toggleEngine(): Promise<void> {
  if (!state.root) return;
  state.busy = true;
  render();
  try {
    if (state.running) {
      await api.engineStop(state.root);
      state.running = false;
      log("engine parado");
    } else {
      await api.engineStart(state.root);
      state.running = true;
      log("engine iniciado");
    }
  } catch (error) {
    log(`erro: ${String(error)}`);
  } finally {
    state.busy = false;
    await loadStatus();
    render();
  }
}

function shell(content: HTMLElement): void {
  app.innerHTML = "";
  app.className = "shell";

  app.appendChild(
    Rail({
      active: state.route,
      onNavigate: (route) => {
        state.route = route;
        render();
      },
    }),
  );

  const workspace = document.createElement("div");
  workspace.className = "workspace";

  const projectName = state.root ? state.root.split("/").filter(Boolean).pop() ?? state.root : "";
  workspace.appendChild(
    Toolbar({
      projectName,
      projectPath: state.root,
      running: state.running,
      busy: state.busy,
      onPlay: () => void toggleEngine(),
      onStop: () => void toggleEngine(),
      onReindex: () => void runReindex(false),
      onPickProject: () => void pickProject(),
    }),
  );

  const body = document.createElement("div");
  body.className = "content";
  body.appendChild(content);
  workspace.appendChild(body);
  app.appendChild(workspace);
}

function render(): void {
  void (async () => {
    let view: HTMLElement;
    switch (state.route) {
      case "engine":
        view = await renderEngine({
          root: state.root,
          status: state.status,
          running: state.running,
          busy: state.busy,
          onPlay: () => void toggleEngine(),
          onStop: () => void toggleEngine(),
          onReindex: (full: boolean) => void runReindex(full),
        });
        break;
      case "search":
        view = await renderSearch({ root: state.root });
        break;
      default:
        view = await renderProjects({
          root: state.root,
          status: state.status,
          onPickProject: () => void pickProject(),
        });
    }
    shell(view);
  })();
}

function subscribeEvents(): void {
  if (!hasTauri) return;
  onEngineEvent("engine://cycle", (payload) => {
    const report = payload as { indexed?: number; errors?: number };
    log(`ciclo: ${report.indexed ?? 0} indexados, ${report.errors ?? 0} erros`);
  });
  onEngineEvent("engine://error", (payload) => log(`erro do engine: ${String(payload)}`));
  onEngineEvent("engine://stopped", () => {
    state.running = false;
    log("engine encerrado");
    render();
  });
}

async function boot(): Promise<void> {
  await loadStatus();
  if (!state.status && state.root) state.route = "projects";
  if (state.status) log("workspace carregado");
  subscribeEvents();
  render();

  window.setInterval(() => {
    void (async () => {
      if (!state.root || state.busy) return;
      await loadStatus();
      if (state.route === "engine") render();
    })();
  }, 4000);
}

void boot();
