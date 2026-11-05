import "./styles/global.css";
import { api, hasTauri, onEngineEvent } from "./api";
import { Rail, type Route } from "./ui/Rail";
import { Toolbar } from "./ui/Toolbar";
import type { RecentProject, StatusReport } from "./types";
import { renderEngine } from "./views/Engine";
import { modelInstructions, renderProjects } from "./views/Projects";
import { createDialog, dialogField, openDialog } from "./ui/Dialog";
import { Button } from "./ui/Button";
import { Input } from "./ui/Input";
import { loadingBar, memoryBar } from "./ui/MemoryBar";
import { Pill } from "./ui/Pill";
import { renderSearch } from "./views/Search";
import { renderMcp } from "./views/Mcp";
import { renderSettings } from "./views/Settings";
import { openWelcome } from "./views/Welcome";
import type { View } from "./views/view";

const app = document.getElementById("app")!;

const RECENT_KEY = "c2.recent";
const MAX_RECENT = 12;

/** Projetos abertos recentemente, mais recente primeiro. */
function loadRecents(): RecentProject[] {
  try {
    const bruto = JSON.parse(localStorage.getItem(RECENT_KEY) ?? "[]") as RecentProject[];
    return bruto
      .filter((item) => typeof item?.path === "string" && item.path.length > 0)
      .sort((a, b) => (b.opened_at ?? 0) - (a.opened_at ?? 0))
      .slice(0, MAX_RECENT);
  } catch {
    return [];
  }
}

function saveRecents(lista: RecentProject[]): void {
  localStorage.setItem(RECENT_KEY, JSON.stringify(lista.slice(0, MAX_RECENT)));
}

function rememberProject(path: string): void {
  if (!path) return;
  const normalizar = (valor: string) => valor.replace(/\\/g, "/").toLowerCase();
  const lista = loadRecents().filter((item) => normalizar(item.path) !== normalizar(path));
  lista.unshift({ path, opened_at: Date.now() });
  saveRecents(lista);
}

function removeRecent(path: string): void {
  const normalizar = (valor: string) => valor.replace(/\\/g, "/").toLowerCase();
  saveRecents(loadRecents().filter((item) => normalizar(item.path) !== normalizar(path)));
  render();
}

async function openRecent(path: string): Promise<void> {
  state.root = path.replace(/\\/g, "/");
  localStorage.setItem("c2.root", state.root);
  rememberProject(state.root);
  await loadStatus();
  state.route = state.status?.initialized ? "engine" : "projects";
  render();
}

interface AppState {
  root: string;
  route: Route;
  status: StatusReport | null;
  running: boolean;
  busy: boolean;
  logs: string[];
  error: string | null;
}

const state: AppState = {
  root: localStorage.getItem("c2.root") ?? "",
  route: "projects",
  status: null,
  running: false,
  busy: false,
  logs: [],
  error: null,
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
    state.error = null;
  } catch (error) {
    state.status = null;
    state.error = String(error);
  }
}

async function pickProject(): Promise<string | null> {
  const path = await api.pickFolder();
  if (!path) return null;
  state.root = path.replace(/\\/g, "/");
  localStorage.setItem("c2.root", state.root);
  rememberProject(state.root);
  await loadStatus();
  // Projeto pronto vai direto para a Engine; sem config ainda, fica em Projetos.
  state.route = state.status?.initialized ? "engine" : "projects";
  render();
  return state.root;
}

/** Abre o seletor, valida o cabeçalho e grava o modelo no workspace. */
async function chooseModelForWelcome(): Promise<string | null> {
  if (!state.root) return null;
  const escolhido = await api.pickModel();
  if (!escolhido) return null;
  try {
    await api.modelInspect(escolhido);
    await api.modelSet(state.root, escolhido);
    await loadStatus();
    log(`modelo selecionado: ${escolhido}`);
    return escolhido;
  } catch (error) {
    await openDialog({
      title: "Modelo inválido",
      body: dialogField("Erro", String(error), true),
      confirmLabel: "Entendi",
      cancelLabel: "",
    });
    return null;
  }
}

/** Assistente de primeira execução: baixar modelo, abrir pasta, selecionar. */
function openWelcomeFlow(): void {
  openWelcome({
    root: () => state.root,
    pickProject: () => pickProject(),
    chooseModel: () => chooseModelForWelcome(),
    finish: () => {
      localStorage.setItem("c2.onboarded", "1");
      state.route = "engine";
      render();
    },
    openSettings: () => {
      state.route = "settings";
      render();
    },
    dismiss: () => {
      localStorage.setItem("c2.onboarded", "1");
    },
  });
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

/**
 * Garante que existe um `.gguf` valido antes de iniciar o engine.
 * Sem modelo abre um popup explicativo e obriga a selecionar (c2-e14).
 */
async function ensureModel(): Promise<boolean> {
  if (!state.root) {
    await pickProject();
    return false;
  }
  await loadStatus();

  const configurado = state.status?.model.path ?? null;
  let motivo = "";
  if (!configurado) {
    motivo = "Nenhum modelo .gguf selecionado neste workspace.";
  } else {
    try {
      await api.modelInspect(configurado);
    } catch (error) {
      motivo = `O modelo configurado não pode ser lido: ${String(error)}`;
    }
  }
  if (!motivo) return true;

  const body = document.createElement("div");
  body.style.display = "grid";
  body.style.gap = "var(--space-s)";
  body.appendChild(
    dialogField(
      "Para indexar, o C2 precisa de um modelo local (GGUF). Ele não é copiado: o workspace guarda só o caminho.",
      motivo,
    ),
  );
  body.appendChild(modelInstructions());

  const escolher = await openDialog({
    title: "Selecione um modelo para iniciar",
    body,
    confirmLabel: "Selecionar .gguf…",
  });
  if (!escolher) return false;

  const escolhido = await api.pickModel();
  if (!escolhido) return false;

  try {
    await api.modelInspect(escolhido);
    await api.modelSet(state.root, escolhido);
    await loadStatus();
    log("modelo selecionado");
    return true;
  } catch (error) {
    await openDialog({
      title: "Modelo inválido",
      body: dialogField("Erro", String(error), true),
      confirmLabel: "Entendi",
      cancelLabel: "",
    });
    return false;
  }
}

/**
 * Antes de iniciar o engine: se o runtime não estiver no ar, abre o modal de
 * carga na GPU (contexto, camadas e barra de VRAM) — estilo LM Studio.
 */
async function ensureModelLoaded(): Promise<boolean> {
  if (!state.root) return false;
  await loadStatus();
  if (state.status?.runtime.healthy) return true;

  let plano: Awaited<ReturnType<typeof api.runtimePlan>>;
  try {
    plano = await api.runtimePlan(state.root);
  } catch (error) {
    await openDialog({
      title: "Não foi possível planejar a carga",
      body: dialogField("Erro", String(error), true),
      confirmLabel: "Entendi",
      cancelLabel: "",
    });
    return false;
  }

  return await new Promise<boolean>((resolve) => {
    let settled = false;
    const handle = createDialog({
      title: "Carregar modelo na GPU",
      onClose: () => {
        if (!settled) {
          settled = true;
          resolve(false);
        }
      },
    });
    const concluir = (valor: boolean) => {
      if (settled) return;
      settled = true;
      handle.close();
      resolve(valor);
    };

    handle.body.appendChild(
      dialogField(
        `${plano.gguf.name ?? "modelo"} · ${plano.gguf.quantization}`,
        plano.gguf.path,
        true,
      ),
    );

    const gpuRow = document.createElement("div");
    gpuRow.style.display = "flex";
    gpuRow.style.gap = "var(--space-2xs)";
    gpuRow.style.flexWrap = "wrap";
    gpuRow.appendChild(
      Pill(
        plano.gpu
          ? `${plano.gpu.name} · ${(plano.gpu.total_mb / 1024).toFixed(1)} GB (${(plano.gpu.free_mb / 1024).toFixed(1)} livres)`
          : "GPU não detectada — o modelo vai para a CPU",
        plano.gpu ? "neutral" : "warning",
        "xs",
      ),
    );
    handle.body.appendChild(gpuRow);

    const campos = document.createElement("div");
    campos.style.display = "grid";
    campos.style.gridTemplateColumns = "1fr 1fr";
    campos.style.gap = "var(--space-s)";

    const ctxField = Input({
      label: "contexto (tokens)",
      value: String(plano.ctx_size),
      size: "sm",
      mono: true,
    });
    const layersField = Input({
      label: "camadas na GPU (0 = CPU)",
      value: String(plano.gpu_layers),
      size: "sm",
      mono: true,
    });
    campos.append(ctxField.root, layersField.root);
    handle.body.appendChild(campos);

    const barra = document.createElement("div");
    barra.appendChild(memoryBar(plano.estimate));
    handle.body.appendChild(barra);

    const erro = document.createElement("div");
    erro.style.color = "var(--danger)";
    erro.style.fontSize = "var(--text-2xs)";
    erro.style.display = "none";
    handle.body.appendChild(erro);

    let recalculo: number | undefined;
    const recalcular = () => {
      window.clearTimeout(recalculo);
      recalculo = window.setTimeout(async () => {
        const ctx = Number.parseInt(ctxField.input.value, 10);
        const camadas = Number.parseInt(layersField.input.value, 10);
        if (!Number.isFinite(ctx) || ctx < 512) return;
        if (!Number.isFinite(camadas) || camadas < 0) return;
        try {
          const novo = await api.runtimePlan(state.root, ctx, camadas);
          barra.replaceChildren(memoryBar(novo.estimate));
        } catch {
          /* mantém a barra anterior */
        }
      }, 250);
    };
    ctxField.input.addEventListener("input", recalcular);
    layersField.input.addEventListener("input", recalcular);

    let btnCancelar: HTMLButtonElement;
    let btnCarregar: HTMLButtonElement;

    const carregar = async () => {
      const ctx = Number.parseInt(ctxField.input.value, 10);
      const camadas = Number.parseInt(layersField.input.value, 10);
      if (!Number.isFinite(ctx) || ctx < 512) {
        erro.textContent = "Contexto inválido (mínimo 512 tokens).";
        erro.style.display = "block";
        return;
      }
      if (!Number.isFinite(camadas) || camadas < 0) {
        erro.textContent = "Camadas na GPU inválidas.";
        erro.style.display = "block";
        return;
      }

      btnCancelar.disabled = true;
      btnCarregar.disabled = true;
      btnCarregar.textContent = "Carregando…";
      erro.style.display = "none";
      barra.replaceChildren(loadingBar());

      try {
        await api.modelLoad(state.root, ctx, camadas);
        await loadStatus();
        log(`modelo carregado na GPU (ctx ${ctx}, ${camadas} camadas)`);
        concluir(true);
      } catch (error) {
        erro.textContent = String(error);
        erro.style.display = "block";
        btnCancelar.disabled = false;
        btnCarregar.disabled = false;
        btnCarregar.textContent = "Carregar na GPU";
        barra.replaceChildren(memoryBar(plano.estimate));
      }
    };

    btnCancelar = Button({
      label: "Cancelar",
      variant: "ghost",
      onClick: () => concluir(false),
    });
    btnCarregar = Button({
      label: "Carregar na GPU",
      variant: "primary",
      onClick: () => void carregar(),
    });
    handle.footer.append(btnCancelar, btnCarregar);
  });
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
      if (!(await ensureModel())) return;
      if (!(await ensureModelLoaded())) return;
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

let disposeView: (() => void) | null = null;

function render(): void {
  void (async () => {
    disposeView?.();
    disposeView = null;
    let view: View;
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
      case "settings":
        view = await renderSettings({
          root: state.root,
          status: state.status,
          onSaved: () =>
            void (async () => {
              await loadStatus();
              render();
            })(),
          onPickProject: () => void pickProject(),
        });
        break;
      case "mcp":
        view = await renderMcp({ root: state.root });
        break;
      case "search":
        view = await renderSearch({ root: state.root });
        break;
      default:
        view = await renderProjects({
          root: state.root,
          status: state.status,
          error: state.error,
          onPickProject: () => void pickProject(),
          onChanged: () =>
            void (async () => {
              await loadStatus();
              render();
            })(),
          onInitialized: () =>
            void (async () => {
              await loadStatus();
              state.route = "engine";
              render();
            })(),
          recents: loadRecents(),
          onOpenRecent: (caminho) => void openRecent(caminho),
          onRemoveRecent: (caminho) => removeRecent(caminho),
          onOnboarding: () => openWelcomeFlow(),
        });
    }
    disposeView = view.dispose ?? null;
    shell(view.element);
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
  // `c2 open --root <dir>` vence o ultimo projeto salvo na janela.
  const fromArgs = await api.initialRoot();
  if (fromArgs) {
    state.root = fromArgs.replace(/\\/g, "/");
    localStorage.setItem("c2.root", state.root);
    rememberProject(state.root);
  }
  await loadStatus();
  if (fromArgs && state.status?.initialized) {
    // `c2 open --root` abre direto na Engine.
    state.route = "engine";
  } else if (!state.status && state.root) {
    state.route = "projects";
  }
  if (state.status) log("workspace carregado");
  subscribeEvents();
  render();

  // Primeira execucao sem modelo: abre o assistente de primeiros passos.
  const primeiraVez = localStorage.getItem("c2.onboarded") !== "1";
  if (primeiraVez && !state.status?.model.path) {
    window.setTimeout(openWelcomeFlow, 350);
  }

  // As telas cuidam da propria atualizacao: a Engine assina eventos do Core
  // e atualiza contadores/log no DOM, sem re-render (c2-e14t2).
}

void boot();
