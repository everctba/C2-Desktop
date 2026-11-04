import "./styles/global.css";
import { api, hasTauri, onEngineEvent } from "./api";
import { Rail, type Route } from "./ui/Rail";
import { Toolbar } from "./ui/Toolbar";
import type { StatusReport } from "./types";
import { renderEngine } from "./views/Engine";
import { modelInstructions, renderProjects } from "./views/Projects";
import { createDialog, dialogField, openDialog } from "./ui/Dialog";
import { Button } from "./ui/Button";
import { Input } from "./ui/Input";
import { loadingBar, memoryBar } from "./ui/MemoryBar";
import { Pill } from "./ui/Pill";
import { renderSearch } from "./views/Search";
import { renderSettings } from "./views/Settings";
import type { View } from "./views/view";

const app = document.getElementById("app")!;

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
  if (!state.root) {
    const fromArgs = await api.initialRoot();
    if (fromArgs) {
      state.root = fromArgs.replace(/\\/g, "/");
      localStorage.setItem("c2.root", state.root);
    }
  }
  await loadStatus();
  if (!state.status && state.root) state.route = "projects";
  if (state.status) log("workspace carregado");
  subscribeEvents();
  render();

  // As telas cuidam da propria atualizacao: a Engine assina eventos do Core
  // e atualiza contadores/log no DOM, sem re-render (c2-e14t2).
}

void boot();
