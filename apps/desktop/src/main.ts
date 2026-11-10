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
import { renderMemory } from "./views/Memory";
import { renderForest } from "./views/Forest";
import { renderMcp } from "./views/Mcp";
import { renderSettings } from "./views/Settings";
import { renderAbout } from "./views/About";
import { initNews } from "./news";
import { idiomaAtual, initLang, t } from "./i18n";
import { iniciarProbeUI, perfBoot } from "./perf";
import { openWelcome } from "./views/Welcome";
import type { View } from "./views/view";

const app = document.getElementById("app")!;

const RECENT_KEY = "c2.recent";
const AUTO_ENGINE_KEY = "c2.autoEngine";
let autoEngineBloqueado = false;
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
  await sincronizarPromptIdioma();
  await loadStatus();
  state.route = state.status?.initialized ? "engine" : "projects";
  autoEngineBloqueado = false;
  render();
  void iniciarEngineAuto();
}

interface AppState {
  root: string;
  route: Route;
  status: StatusReport | null;
  running: boolean;
  busy: boolean;
  logs: string[];
  error: string | null;
  gpuUsedMb: number | null;
}

const state: AppState = {
  root: localStorage.getItem("c2.root") ?? "",
  route: "projects",
  status: null,
  running: false,
  busy: false,
  logs: [],
  error: null,
  gpuUsedMb: null,
};

function log(message: string): void {
  const time = new Date().toLocaleTimeString("pt-BR");
  state.logs.unshift(`${time} ${message}`);
  state.logs = state.logs.slice(0, 40);
}

async function sincronizarPromptIdioma(): Promise<void> {
  if (!state.root) return;
  try {
    await api.promptSync(state.root, idiomaAtual());
  } catch {
    /* config antiga/sem workspace: segue */
  }
}

async function loadStatus(): Promise<void> {
  if (!state.root) {
    state.status = null;
    state.gpuUsedMb = null;
    return;
  }
  try {
    state.status = await api.status(state.root);
    state.running = await api.engineRunning();
    state.gpuUsedMb = null;
    if (
      state.status?.runtime.healthy &&
      state.status.model.provider !== "openai_compat"
    ) {
      try {
        const gpu = await api.gpuInfo();
        state.gpuUsedMb = gpu?.used_mb ?? null;
      } catch {
        /* sem nvidia-smi/GPU */
      }
    }
    state.error = null;
  } catch (error) {
    state.status = null;
    state.gpuUsedMb = null;
    state.error = String(error);
  }
}

async function pickProject(): Promise<string | null> {
  const path = await api.pickFolder();
  if (!path) return null;
  state.root = path.replace(/\\/g, "/");
  localStorage.setItem("c2.root", state.root);
  rememberProject(state.root);
  await sincronizarPromptIdioma();
  await loadStatus();
  // Projeto pronto vai direto para a Engine; sem config ainda, fica em Projetos.
  state.route = state.status?.initialized ? "engine" : "projects";
  autoEngineBloqueado = false;
  render();
  void iniciarEngineAuto();
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
      title: t("Modelo inválido"),
      body: dialogField(t("Erro"), String(error), true),
      confirmLabel: t("Entendi"),
      cancelLabel: "",
    });
    return null;
  }
}

/** Assistente de primeira execução: baixar modelo, abrir pasta, selecionar. */
function openWelcomeFlow(modo: "local" | "nuvem" = "local"): void {
  openWelcome({
    root: () => state.root,
    pickProject: () => pickProject(),
    chooseModel: () => chooseModelForWelcome(),
    finish: () => {
      localStorage.setItem("c2.onboarded", "1");
      state.route = modo === "nuvem" ? "settings" : "engine";
      autoEngineBloqueado = false;
      render();
      void iniciarEngineAuto();
    },
    openSettings: () => {
      state.route = "settings";
      render();
    },
    dismiss: () => {
      localStorage.setItem("c2.onboarded", "1");
    },
  }, modo);
}

/** Primeiro uso: escolha entre IA local e API Key/nuvem. */
function escolherModoInicial(): Promise<"local" | "nuvem" | null> {
  return new Promise((resolve) => {
    let decidido = false;
    const handle = createDialog({
      title: t("Como você quer rodar a IA?"),
      onClose: () => {
        if (!decidido) {
          decidido = true;
          resolve(null);
        }
      },
    });
    handle.element.style.width = "min(560px, 100%)";

    const corpo = document.createElement("div");
    corpo.style.display = "grid";
    corpo.style.gridTemplateColumns = "repeat(2, minmax(0, 1fr))";
    corpo.style.gap = "var(--space-s)";
    const intro = document.createElement("p");
    intro.textContent = t(
      "Escolha o caminho inicial. Dá para trocar depois em Config → Modo de uso.",
    );
    intro.style.margin = "0";
    intro.style.color = "var(--text-muted)";
    intro.style.gridColumn = "1 / -1";
    corpo.appendChild(intro);

    const finalizar = (modo: "local" | "nuvem") => {
      if (decidido) return;
      decidido = true;
      handle.close();
      resolve(modo);
    };

    const opcao = (
      titulo: string,
      descricao: string,
      modo: "local" | "nuvem",
    ): HTMLButtonElement => {
      const box = document.createElement("button");
      box.type = "button";
      box.className = "modoInicialCard";
      const tituloEl = document.createElement("strong");
      tituloEl.textContent = titulo;
      const descEl = document.createElement("span");
      descEl.textContent = descricao;
      box.append(tituloEl, descEl);
      box.addEventListener("click", () => finalizar(modo));
      return box;
    };

    corpo.append(
      opcao(
        t("IA local (padrão)"),
        t("Modelo GGUF na sua GPU. Privado e sem custo por token. Precisa de GPU de 8 GB."),
        "local",
      ),
      opcao(
        t("API Key / nuvem"),
        t("OpenAI, Groq, DeepSeek, OpenRouter ou servidor local (LM Studio/Ollama/vLLM). Sem GPU."),
        "nuvem",
      ),
    );
    handle.body.appendChild(corpo);

    const depois = Button({
      label: t("Decidir depois"),
      variant: "ghost",
      onClick: () => finalizar("local"),
    });
    handle.footer.appendChild(depois);
    depois.focus();
  });
}

const falhasProvedorAvisadas = new Set<string>();

/**
 * Falha do provedor de nuvem: avisa com popup e deixa claro que NADA foi
 * trocado para o modelo local automaticamente (o local precisa da GPU).
 */
async function avisarFalhaProvedor(mensagem: string): Promise<void> {
  const chave = mensagem.trim().slice(0, 240);
  if (!chave || falhasProvedorAvisadas.has(chave)) return;
  falhasProvedorAvisadas.add(chave);
  const corpo = document.createElement("div");
  corpo.appendChild(dialogField("Erro do provedor", mensagem, true));
  const aviso = document.createElement("p");
  aviso.style.marginTop = "var(--space-s)";
  aviso.style.maxWidth = "none";
  aviso.textContent =
    "O C2 NÃO trocou automaticamente para o modelo local: ele precisa da GPU e ela " +
    "pode estar em uso por outra coisa. Para usar o modelo local de propósito, mude " +
    "em Config → Provedor de IA.";
  corpo.appendChild(aviso);
  await openDialog({
    title: t("Provedor de IA falhou"),
    body: corpo,
    confirmLabel: t("Entendi"),
    cancelLabel: "",
  });
}

async function runReindex(full: boolean): Promise<void> {
  if (!state.root) return;
  state.busy = true;
  render();
  log(full ? "reindex completo iniciado" : "reindex (limite 10) iniciado");

  // Estimativa de custo antes de chamar a nuvem (c2-f4p.4).
  if (state.status?.model.provider === "openai_compat") {
    try {
      const estimativa = await api.costEstimate(state.root, full);
      if (estimativa.estimated_cost_usd > 0) {
        const seguir = await openDialog({
          title: t("Custo estimado"),
          body: dialogField(
            t("Estimativa"),
            `~US$ ${estimativa.estimated_cost_usd.toFixed(4)} para ${estimativa.pending} arquivo(s).` +
              (estimativa.cap_usd > 0
                ? ` Teto por execução: US$ ${estimativa.cap_usd.toFixed(2)}.`
                : " Sem teto configurado.") +
              "\n\nValores aproximados; confira os preços do provedor.",
          ),
          confirmLabel: t("Indexar"),
          cancelLabel: "Cancelar",
        });
        if (!seguir) {
          state.busy = false;
          render();
          return;
        }
      }
    } catch {
      /* sem estimativa: segue */
    }
  }

  try {
    const report = await api.reindex(state.root, full, full ? undefined : 10);
    log(
      `reindex: ${report.indexed} indexados, ${report.unchanged} inalterados, ` +
        `${report.errors} erros, ${report.blocked} bloqueados por segredo` +
          (report.cost_usd > 0
            ? ` | custo US$ ${report.cost_usd.toFixed(4)} (estimado ${report.estimated_cost_usd.toFixed(4)})`
            : ""),
    );
    if (report.errors > 0 && state.status?.model.provider === "openai_compat") {
      await avisarFalhaProvedor(
        report.last_error ?? "erro ao chamar o provedor de nuvem",
      );
    }
  } catch (error) {
    log(`erro: ${String(error)}`);
    if (state.status?.model.provider === "openai_compat") {
      await avisarFalhaProvedor(String(error));
    }
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
    title: t("Selecione um modelo para iniciar"),
    body,
    confirmLabel: t("Selecionar .gguf…"),
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
      title: t("Modelo inválido"),
      body: dialogField(t("Erro"), String(error), true),
      confirmLabel: t("Entendi"),
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
      title: t("Não foi possível planejar a carga"),
      body: dialogField(t("Erro"), String(error), true),
      confirmLabel: t("Entendi"),
      cancelLabel: "",
    });
    return false;
  }

  return await new Promise<boolean>((resolve) => {
    let settled = false;
    const handle = createDialog({
      title: t("Carregar modelo na GPU"),
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

    const criarBarraProgresso = () => {
      const root = document.createElement("div");
      root.style.display = "grid";
      root.style.gap = "var(--space-2xs)";
      const track = document.createElement("div");
      track.style.height = "10px";
      track.style.borderRadius = "999px";
      track.style.background = "var(--bg-surface-3)";
      track.style.overflow = "hidden";
      const fill = document.createElement("div");
      fill.style.height = "100%";
      fill.style.width = "0%";
      fill.style.borderRadius = "999px";
      fill.style.background = "var(--accent-3)";
      fill.style.transition = "width 250ms ease";
      track.appendChild(fill);
      const texto = document.createElement("span");
      texto.className = "pageHint";
      texto.style.maxWidth = "none";
      texto.textContent = t("Carregando modelo…");
      root.append(track, texto);
      return { root, fill, texto };
    };

    const carregar = async () => {
      const ctx = Number.parseInt(ctxField.input.value, 10);
      const camadas = Number.parseInt(layersField.input.value, 10);
      if (!Number.isFinite(ctx) || ctx < 512) {
        erro.textContent = t("Contexto inválido (mínimo 512 tokens).");
        erro.style.display = "block";
        return;
      }
      if (!Number.isFinite(camadas) || camadas < 0) {
        erro.textContent = t("Camadas na GPU inválidas.");
        erro.style.display = "block";
        return;
      }

      btnCancelar.disabled = true;
      btnCarregar.disabled = true;
      btnCarregar.textContent = t("Carregando…");
      erro.style.display = "none";

      const progresso = criarBarraProgresso();
      barra.replaceChildren(progresso.root);

      // Barra cresce com a VRAM efetivamente ocupada pelo runtime. Sem GPU,
      // mantem a barra indeterminada (loadingBar).
      const totalMb = Math.max(1, plano.estimate.total_mb);
      const baseUsada = plano.gpu?.used_mb ?? 0;
      const inicioCarga = performance.now();
      let intervalo: number | undefined;
      let ultimoPct = 0;
      if (plano.gpu) {
        let consultando = false;
        intervalo = window.setInterval(() => {
          void (async () => {
            if (consultando) return;
            consultando = true;
            try {
              const gpu = await api.gpuInfo();
              if (!gpu) return;
              const delta = Math.max(0, gpu.used_mb - baseUsada);
              const realPct = Math.min(95, Math.round((delta / totalMb) * 100));
              // O llama-server aloca a VRAM em blocos; interpola pelo tempo
              // para a barra nao ficar parada durante a leitura dos tensores.
              const tempoPct = Math.min(
                75,
                Math.round(((performance.now() - inicioCarga) / 20000) * 75),
              );
              const pct = Math.min(95, Math.max(ultimoPct, realPct, tempoPct));
              if (pct > ultimoPct) {
                ultimoPct = pct;
                progresso.fill.style.width = `${pct}%`;
                progresso.texto.textContent =
                  `${t("Carregando modelo…")} ${pct}% ` +
                  `(${(delta / 1024).toFixed(1)} / ${(totalMb / 1024).toFixed(1)} GB)`;
              }
            } catch {
              /* mantem o ultimo valor */
            } finally {
              consultando = false;
            }
          })();
        }, 500);
      } else {
        progresso.root.replaceChildren(loadingBar());
      }

      try {
        await api.modelLoad(state.root, ctx, camadas);
        if (intervalo !== undefined) window.clearInterval(intervalo);
        progresso.fill.style.width = "100%";
        progresso.texto.textContent = t("Modelo carregado");
        await loadStatus();
        log(`modelo carregado na GPU (ctx ${ctx}, ${camadas} camadas)`);
        concluir(true);
      } catch (error) {
        if (intervalo !== undefined) window.clearInterval(intervalo);
        erro.textContent = String(error);
        erro.style.display = "block";
        btnCancelar.disabled = false;
        btnCarregar.disabled = false;
        btnCarregar.textContent = t("Carregar na GPU");
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

async function unloadModel(): Promise<void> {
  if (!state.root) return;
  autoEngineBloqueado = true;
  state.busy = true;
  render();
  try {
    await api.modelUnload(state.root);
    state.running = false;
    log("modelo descarregado da VRAM");
  } catch (error) {
    log(`erro ao descarregar modelo: ${String(error)}`);
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
      autoEngineBloqueado = true;
      await api.engineStop(state.root);
      state.running = false;
      log("engine parado");
    } else {
      autoEngineBloqueado = false;
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

function autoEngineLigado(): boolean {
  return localStorage.getItem(AUTO_ENGINE_KEY) !== "0";
}

/**
 * O desktop e o modo normal do C2: com projeto + modelo local, sobe o engine
 * sozinho para o indice andar em segundo plano. MCP continua somente leitura.
 */
async function iniciarEngineAuto(): Promise<void> {
  if (!state.root || state.running || state.busy || autoEngineBloqueado) return;
  if (!autoEngineLigado()) return;
  const status = state.status;
  if (!status?.initialized || !status.model.path) return;
  if (status.model.provider === "openai_compat") return;
  try {
    await api.engineStart(state.root);
    state.running = true;
    log("engine iniciado automaticamente");
    render();
  } catch (error) {
    autoEngineBloqueado = true;
    log(`auto-start do engine falhou: ${String(error)}`);
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
      modelLoaded:
        Boolean(state.status?.runtime.healthy) &&
        state.status?.model.provider !== "openai_compat",
      modelLoading:
        Boolean(state.running) &&
        !state.status?.runtime.healthy &&
        state.status?.model.provider !== "openai_compat",
      vramMb: state.gpuUsedMb,
      onPlay: () => void toggleEngine(),
      onStop: () => void toggleEngine(),
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
          onUnloadModel: () => void unloadModel(),
          onCleanupDone: () =>
            void (async () => {
              await loadStatus();
              log("metadados removidos");
              render();
            })(),
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
      case "about":
        view = await renderAbout();
        break;
      case "search":
        view = await renderSearch({
          root: state.root,
          onNavigate: (route) => {
            state.route = route;
            render();
          },
        });
        break;
      case "memory":
        view = await renderMemory({ root: state.root });
        break;
      case "forest":
        view = await renderForest({ root: state.root });
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
    const report = payload as {
      indexed?: number;
      errors?: number;
      last_error?: string | null;
    };
    log(`ciclo: ${report.indexed ?? 0} indexados, ${report.errors ?? 0} erros`);
    if ((report.errors ?? 0) > 0 && state.status?.model.provider === "openai_compat") {
      void avisarFalhaProvedor(
        report.last_error ?? "erro no ciclo de indexacao com o provedor de nuvem",
      );
    }
  });
  onEngineEvent("engine://error", (payload) => {
    log(`erro do engine: ${String(payload)}`);
    if (state.status?.model.provider === "openai_compat") {
      void avisarFalhaProvedor(String(payload));
    }
  });
  onEngineEvent("engine://stopped", () => {
    state.running = false;
    log("engine encerrado");
    render();
  });
}

function instalarLogDeErros(): void {
  const enviar = (texto: string) => {
    try {
      void api.frontendLog(texto).catch(() => {});
    } catch {
      /* Tauri indisponivel */
    }
  };
  window.addEventListener("error", (evento) => {
    enviar(`erro: ${evento.message} (${evento.filename}:${evento.lineno}:${evento.colno})`);
  });
  window.addEventListener("unhandledrejection", (evento) => {
    enviar(`promise rejeitada: ${String(evento.reason)}`);
  });
  const erroOriginal = console.error.bind(console);
  console.error = (...args: unknown[]) => {
    erroOriginal(...args);
    enviar(`console.error: ${args.map((arg) => String(arg)).join(" ")}`);
  };
}

async function boot(): Promise<void> {
  instalarLogDeErros();
  perfBoot("js_boot_start");
  iniciarProbeUI();
  const logBoot = (etapa: string, dados: Record<string, unknown> = {}) => {
    perfBoot(etapa, dados);
    try {
      void api.frontendLog(`boot: ${etapa}`).catch(() => {});
    } catch {
      /* Tauri indisponivel */
    }
  };
  logBoot("iniciando");
  await initLang();
  logBoot("idioma_ok");
  // `c2 open --root <dir>` vence o ultimo projeto salvo na janela.
  const fromArgs = await api.initialRoot();
  logBoot("initial_root_ok");
  if (fromArgs) {
    state.root = fromArgs.replace(/\\/g, "/");
    localStorage.setItem("c2.root", state.root);
    rememberProject(state.root);
  }
  await sincronizarPromptIdioma();
  const statusInicio = performance.now();
  await loadStatus();
  logBoot("status_ok", { ms: Math.round(performance.now() - statusInicio) });
  if (fromArgs && state.status?.initialized) {
    // `c2 open --root` abre direto na Engine.
    state.route = "engine";
  } else if (!state.status && state.root) {
    state.route = "projects";
  }
  if (state.status) log("workspace carregado");
  subscribeEvents();
  window.addEventListener("c2:lang", () => {
    void (async () => {
      await sincronizarPromptIdioma();
      render();
    })();
  });
  render();
  logBoot("render_ok");
  requestAnimationFrame(() => perfBoot("first_raf"));
  void iniciarEngineAuto();

  // Primeira execucao sem modelo: abre o assistente de primeiros passos.
  const primeiraVez = localStorage.getItem("c2.onboarded") !== "1";
  if (primeiraVez && !state.status?.model.path) {
    window.setTimeout(() => {
      void (async () => {
        const modo = await escolherModoInicial();
        localStorage.setItem("c2.mode", modo ?? "local");
        openWelcomeFlow(modo ?? "local");
      })();
    }, 350);
  }

  // O banner "Model on GPU" depende do runtime real, nao do Play. Monitora a
  // transicao runtime parado -> saudavel para aparecer assim que carregar.
  let ultimoModeloCarregado = Boolean(state.status?.runtime.healthy);
  const monitorarModelo = async () => {
    if (!state.root || state.busy) return;
    await loadStatus();
    const carregado =
      Boolean(state.status?.runtime.healthy) &&
      state.status?.model.provider !== "openai_compat";
    if (carregado !== ultimoModeloCarregado) {
      ultimoModeloCarregado = carregado;
      render();
    }
  };
  window.setInterval(() => void monitorarModelo(), 5000);

  // Atualiza a VRAM do banner sem re-renderizar a tela.
  const atualizarVram = () => {
    const alvo = document.querySelector<HTMLElement>("[data-model-vram]");
    if (!alvo || !state.status?.runtime.healthy) return;
    void api
      .gpuInfo()
      .then((gpu) => {
        if (!gpu) return;
        state.gpuUsedMb = gpu.used_mb;
        alvo.textContent = `VRAM ${(gpu.used_mb / 1024).toFixed(1)} GB`;
      })
      .catch(() => {});
  };
  window.setInterval(atualizarVram, 5000);
  atualizarVram();

  // Popup de novidades (news.json estatico), sempre depois do boot.
  initNews();

  // As telas cuidam da propria atualizacao: a Engine assina eventos do Core
  // e atualiza contadores/log no DOM, sem re-render (c2-e14t2).
}

void boot().catch((erro) => {
  console.error("boot falhou", erro);
});
