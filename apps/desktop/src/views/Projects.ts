import { api, hasTauri, openExternal } from "../api";
import { Button } from "../ui/Button";
import { Card, cardBody, cardFooter } from "../ui/Card";
import { dialogField, dialogPills, openDialog } from "../ui/Dialog";
import { Pill } from "../ui/Pill";
import { Toggle } from "../ui/Toggle";
import type { ModelInspection, RecentProject, StatusReport } from "../types";
import type { View } from "./view";

/** Modelo recomendado para o C2 (validado nesta máquina). */
export const MODELO_RECOMENDADO = {
  repo: "lmstudio-community/gemma-4-E2B-it-GGUF",
  arquivo: "gemma-4-E2B-it-Q4_K_M.gguf",
  tamanho: "~3,2 GB",
  url: "https://huggingface.co/lmstudio-community/gemma-4-E2B-it-GGUF/tree/main",
};
import styles from "./views.module.css";

export interface ProjectsContext {
  root: string;
  status: StatusReport | null;
  error: string | null;
  onPickProject: () => void;
  onChanged: () => void;
  /// Chamado depois de criar o .c2/config.json (a janela vai para a Engine).
  onInitialized: () => void;
  /// Projetos abertos recentemente (mais recente primeiro).
  recents: RecentProject[];
  onOpenRecent: (path: string) => void;
  onRemoveRecent: (path: string) => void;
  /// Reabre o assistente de primeiros passos.
  onOnboarding: () => void;
}

/** Tela inicial: abrir/inicializar projeto, modelo e runtime. */
export async function renderProjects(ctx: ProjectsContext): Promise<View> {
  const page = document.createElement("div");
  page.className = `${styles.page} ${"content"}`;

  const header = document.createElement("div");
  header.className = styles.header;
  const title = document.createElement("h1");
  title.className = "pageTitle";
  title.textContent = "Projetos";
  const hint = document.createElement("p");
  hint.className = "pageHint";
  hint.textContent =
    "Abra a pasta de um projeto para o C2 indexar: o índice fica em .c2/ e os metadados ao lado de cada arquivo.";
  // uma linha só: sem limite de 70ch neste hint
  hint.style.maxWidth = "none";
  header.append(title, hint);
  page.appendChild(header);

  if (!ctx.status) {
    const empty = document.createElement("div");
    empty.className = styles.emptyState;
    const text = document.createElement("p");
    text.className = "pageHint";
    text.textContent = ctx.root
      ? "Projeto ainda não inicializado. Clique em Inicializar para criar .c2/config.json."
      : "Nenhum projeto aberto.";
    empty.appendChild(text);

    if (ctx.error) {
      const detail = document.createElement("p");
      detail.className = "pageHint";
      detail.style.color = "var(--danger)";
      detail.textContent = ctx.error;
      empty.appendChild(detail);
    }
    const actions = document.createElement("div");
    actions.className = styles.actions;
    actions.appendChild(
      Button({
        label: "Abrir projeto",
        iconName: "projects",
        variant: "primary",
        onClick: ctx.onPickProject,
      }),
    );
    if (ctx.root) {
      actions.appendChild(
        Button({
          label: "Inicializar",
          variant: "secondary",
          onClick: async () => {
            try {
              await api.init(ctx.root);
            } catch (error) {
              console.error(error);
            }
            ctx.onInitialized();
          },
        }),
      );
    }
    empty.appendChild(actions);
    page.appendChild(empty);
    return { element: page };
  }

  const status = ctx.status;
  const grid = document.createElement("div");
  grid.className = styles.grid;

  grid.appendChild(
    projectCard("Projeto", status.root, [
      ["Arquivos descobertos", String(status.files)],
      ["Metadados", String(status.metadata)],
      ["Sensíveis pulados", String(status.skipped.sensitive)],
    ]),
  );

  grid.appendChild(modelCard(ctx, status));

  grid.appendChild(
    projectCard("Runtime", status.runtime.healthy ? "saudável" : "parado", [
      ["Binário", status.runtime.binary ?? "não localizado"],
      ["Origem", status.runtime.source ?? "-"],
      ["Versão", status.runtime.version ?? "-"],
    ]),
  );

  grid.appendChild(
    projectCard("Segurança", `política ${status.index.secret_policy}`, [
      ["Máx. por arquivo", `${Math.round(status.index.max_file_bytes / 1024)} KiB`],
      ["Extensões", String(status.index.extensions)],
      ["Teto por ciclo", `${Math.round(status.index.max_total_bytes_per_run / 1024 / 1024)} MiB`],
    ]),
  );

  page.appendChild(grid);

  const actions = document.createElement("div");
  actions.className = styles.actions;

  actions.appendChild(
    Button({
      label: "Trocar projeto",
      iconName: "projects",
      onClick: ctx.onPickProject,
    }),
  );
  actions.appendChild(
    Button({
      label: "Primeiros passos",
      variant: "ghost",
      onClick: ctx.onOnboarding,
    }),
  );
  page.appendChild(actions);

  if (ctx.recents.length > 0) {
    page.appendChild(recentCard(ctx));
  }

  if (!hasTauri) {
    const note = document.createElement("p");
    note.className = "pageHint";
    note.textContent =
      "Modo preview no navegador: dados falsos. Rode `pnpm tauri dev` para usar o Core real.";
    page.appendChild(note);
  }

  return { element: page };
}

function projectCard(title: string, headline: string, fields: Array<[string, string]>): HTMLElement {
  const card = Card({ title, accent: 3 });
  const body = cardBody(card);

  const value = document.createElement("div");
  value.className = styles.fieldValue;
  value.textContent = headline;
  value.title = headline;
  body.appendChild(value);

  for (const [label, fieldValue] of fields) {
    const field = document.createElement("div");
    field.className = styles.field;
    const fieldLabel = document.createElement("span");
    fieldLabel.className = styles.fieldLabel;
    fieldLabel.textContent = label;
    const text = document.createElement("span");
    text.className = styles.fieldValue;
    text.textContent = fieldValue;
    text.title = fieldValue;
    field.append(fieldLabel, text);
    body.appendChild(field);
  }

  const footer = cardFooter(card);
  footer.appendChild(Pill("workspace", "neutral", "xs"));
  return card;
}

/** Card do modelo: mostra o GGUF atual e abre o seletor com validação. */
function modelCard(ctx: ProjectsContext, status: StatusReport): HTMLElement {
  const card = Card({ title: "Modelo", accent: 4 });
  const body = cardBody(card);

  const name = document.createElement("div");
  name.className = styles.fieldValue;
  name.textContent = status.model.path ? status.model.name : "nenhum modelo selecionado";
  body.appendChild(name);

  if (!status.model.path) {
    const aviso = Pill("selecione um .gguf para poder iniciar", "warning", "xs");
    body.appendChild(aviso);
  }

  const cfg = document.createElement("div");
  cfg.className = styles.field;
  const cfgLabel = document.createElement("span");
  cfgLabel.className = styles.fieldLabel;
  cfgLabel.textContent = "GGUF";
  const cfgPath = document.createElement("span");
  cfgPath.className = styles.mono;
  cfgPath.textContent = status.model.path ?? "não configurado";
  cfgPath.title = status.model.path ?? "";
  cfg.append(cfgLabel, cfgPath);
  body.appendChild(cfg);

  const endpoint = document.createElement("div");
  endpoint.className = styles.field;
  const endpointLabel = document.createElement("span");
  endpointLabel.className = styles.fieldLabel;
  endpointLabel.textContent = "Endpoint";
  const endpointValue = document.createElement("span");
  endpointValue.className = styles.fieldValue;
  endpointValue.textContent = `${status.model.host}:${status.model.port}`;
  endpoint.append(endpointLabel, endpointValue);
  body.appendChild(endpoint);

  const footer = cardFooter(card);
  footer.appendChild(
    Pill(status.model.provider === "local" ? "local" : "nuvem", "neutral", "xs"),
  );
  footer.appendChild(
    Button({
      label: "Selecionar .gguf…",
      iconName: "projects",
      size: "sm",
      onClick: () => void pickModelFlow(ctx),
    }),
  );
  const raciocinio = Toggle({
    label: "raciocínio",
    checked: !status.model.disable_thinking,
    tone: "brand",
    onChange: async (ligado) => {
      try {
        await api.configSet(ctx.root, "model.disable_thinking", ligado ? "false" : "true");
        ctx.onChanged();
      } catch (erro) {
        await openDialog({
          title: "Não foi possível alterar o raciocínio",
          body: dialogField("Erro", String(erro), true),
          confirmLabel: "Entendi",
          cancelLabel: "",
        });
      }
    },
  });
  raciocinio.root.title =
    "Raciocínio LIGADO (verde): o modelo pensa antes de responder. " +
    "Medido em 6 arquivos: ~11,1 s e ~730 tokens por arquivo; descrições um pouco " +
    "mais específicas em código ambíguo, mas pode alucinar siglas. " +
    "DESLIGADO: ~3,5 s e ~127 tokens por arquivo; resposta menor e mais estável, " +
    "ideal para indexar em lote. Clique para alternar.";
  footer.appendChild(raciocinio.root);

  body.appendChild(modelInstructions());
  return card;
}

async function pickModelFlow(ctx: ProjectsContext): Promise<void> {
  const path = await api.pickModel();
  if (!path) return;

  let inspection: ModelInspection;
  try {
    inspection = await api.modelInspect(path);
  } catch (error) {
    await openDialog({
      title: "Modelo inválido",
      body: dialogField("Erro", String(error), true),
      confirmLabel: "Entendi",
      cancelLabel: "",
    });
    return;
  }

  const confirm = await openDialog({
    title: "Usar este modelo?",
    body: inspectionBody(inspection),
    confirmLabel: "Usar modelo",
  });
  if (!confirm) return;

  try {
    await api.modelSet(ctx.root, path);
    ctx.onChanged();
  } catch (error) {
    await openDialog({
      title: "Falha ao salvar",
      body: dialogField("Erro", String(error), true),
      confirmLabel: "Entendi",
      cancelLabel: "",
    });
  }
}

function inspectionBody(inspection: ModelInspection): HTMLElement {
  const container = document.createElement("div");
  container.style.display = "grid";
  container.style.gap = "var(--space-s)";

  const gguf = inspection.gguf;
  container.appendChild(dialogField("Nome", gguf.name ?? "sem nome no cabeçalho"));
  container.appendChild(
    dialogField("Arquitetura", gguf.architecture ?? "desconhecida"),
  );
  container.appendChild(
    dialogField(
      "Tamanho",
      `${(gguf.size_bytes / 1024 / 1024 / 1024).toFixed(2)} GB · ${gguf.tensor_count} tensores · GGUF v${gguf.version}`,
    ),
  );

  const pills = dialogPills();
  pills.appendChild(Pill(gguf.quantization, "accent", "xs"));

  if (inspection.gpu) {
    const gpu = inspection.gpu;
    pills.appendChild(
      Pill(
        `VRAM livre ${(gpu.free_mb / 1024).toFixed(1)} de ${(gpu.total_mb / 1024).toFixed(1)} GB`,
        "neutral",
        "xs",
      ),
    );
  }

  const fits: Record<string, { label: string; tone: "success" | "warning" | "danger" | "neutral" }> = {
    fits: { label: "cabe na VRAM", tone: "success" },
    tight: { label: "no limite da VRAM", tone: "warning" },
    too_big: { label: "não cabe na VRAM livre", tone: "danger" },
    unknown: { label: "sem GPU detectada", tone: "neutral" },
  };
  const verdict = fits[inspection.fit] ?? fits.unknown;
  pills.appendChild(Pill(verdict.label, verdict.tone, "xs"));
  container.appendChild(pills);

  container.appendChild(
    dialogField("Caminho (não será copiado)", gguf.path, true),
  );
  return container;
}

/**
 * Instruções para baixar um modelo do HuggingFace (sem copiar para o
 * workspace: o C2 só guarda o caminho).
 */
/** Conteudo do modal: passos, modelo recomendado, link e copiar link. */
function modelInstructionsBody(): HTMLElement {
  const box = document.createElement("div");
  box.style.display = "grid";
  box.style.gap = "var(--space-s)";

  const atalho = document.createElement("p");
  atalho.style.cssText =
    "margin: 0; padding: var(--space-s); border: 1px solid var(--border-muted); " +
    "border-radius: var(--radius); font-size: var(--text-s); line-height: 1.45; " +
    "color: var(--text-muted);";
  atalho.innerHTML =
    "Já usa modelos <strong>GGUF</strong> no <strong>LM Studio</strong> (ou em outra " +
    "pasta)? Você não precisa baixar nada: feche este aviso, clique em " +
    "<strong>Selecionar .gguf…</strong> e escolha o modelo que já está na sua máquina.";
  box.appendChild(atalho);

  const passos = document.createElement("ol");
  passos.style.cssText =
    "margin: 0 0 0 var(--space-l); display: grid; gap: var(--space-3xs); " +
    "font-size: var(--text-s); color: var(--text-muted); line-height: 1.45;";
  const itens = [
    "Abra o repositório recomendado no HuggingFace (link abaixo).",
    `Baixe o arquivo ${MODELO_RECOMENDADO.arquivo} (${MODELO_RECOMENDADO.tamanho}, Q4_K_M).`,
    "Salve numa pasta sua (ex.: D:\\modelos) — o C2 não copia o arquivo.",
    "Clique em “Selecionar .gguf…” e escolha esse arquivo.",
  ];
  for (const texto of itens) {
    const li = document.createElement("li");
    li.textContent = texto;
    passos.appendChild(li);
  }
  box.appendChild(passos);

  const recomendado = document.createElement("div");
  recomendado.className = styles.field;
  const label = document.createElement("span");
  label.className = styles.fieldLabel;
  label.textContent = "recomendado · gemma 4 E2B";
  const valor = document.createElement("span");
  valor.className = styles.mono;
  valor.textContent = MODELO_RECOMENDADO.repo;
  recomendado.append(label, valor);
  box.appendChild(recomendado);

  const acoes = document.createElement("div");
  acoes.className = styles.actions;

  const link = document.createElement("a");
  link.href = MODELO_RECOMENDADO.url;
  link.target = "_blank";
  link.rel = "noreferrer";
  link.className = styles.mono;
  link.textContent = MODELO_RECOMENDADO.url;
  link.style.color = "var(--accent-3)";
  link.title = "Abrir no navegador";
  link.addEventListener("click", (evento) => {
    evento.preventDefault();
    void openExternal(MODELO_RECOMENDADO.url);
  });
  acoes.appendChild(link);

  acoes.appendChild(
    Button({
      label: "Copiar link",
      size: "xs",
      variant: "secondary",
      onClick: async () => {
        try {
          const { writeText } = await import("@tauri-apps/plugin-clipboard-manager");
          await writeText(MODELO_RECOMENDADO.url);
        } catch {
          try {
            await navigator.clipboard.writeText(MODELO_RECOMENDADO.url);
          } catch {
            /* sem clipboard: usuario copia manualmente */
          }
        }
      },
    }),
  );

  box.appendChild(acoes);
  return box;
}

/** Botao "Como baixar um modelo" — abre as instrucoes em modal. */
export function modelInstructions(): HTMLElement {
  const botao = document.createElement("button");
  botao.type = "button";
  botao.className = styles.linkButton;
  botao.textContent = "Como baixar um modelo";
  botao.addEventListener("click", () => {
    void openDialog({
      title: "Como baixar um modelo",
      body: modelInstructionsBody(),
      confirmLabel: "Fechar",
      cancelLabel: "",
    });
  });
  return botao;
}

/** Lista de projetos recentes: clique abre, X remove do historico. */
function recentCard(ctx: ProjectsContext): HTMLElement {
  const card = Card({ title: "Projetos recentes", accent: 6 });
  const body = cardBody(card);

  const atual = (caminho: string) =>
    caminho.replace(/\\/g, "/").toLowerCase() === ctx.root.replace(/\\/g, "/").toLowerCase();

  for (const item of ctx.recents) {
    const row = document.createElement("div");
    row.className = styles.recentRow;
    row.title = item.path;
    row.addEventListener("click", () => ctx.onOpenRecent(item.path));

    const info = document.createElement("div");
    info.className = styles.recentInfo;
    const nome = document.createElement("span");
    nome.className = styles.recentName;
    nome.textContent = item.path.split("/").filter(Boolean).pop() ?? item.path;
    const caminho = document.createElement("span");
    caminho.className = styles.recentPath;
    caminho.textContent = item.path;
    info.append(nome, caminho);
    row.appendChild(info);

    if (atual(item.path)) {
      row.appendChild(Pill("atual", "accent", "xs"));
    }

    const remover = Button({
      iconOnly: true,
      iconName: "close",
      variant: "ghost",
      size: "xs",
      onClick: () => ctx.onRemoveRecent(item.path),
    });
    remover.setAttribute("aria-label", `Remover ${item.path} dos recentes`);
    remover.title = "Remover dos recentes";
    remover.addEventListener("click", (evento) => evento.stopPropagation());
    row.appendChild(remover);

    body.appendChild(row);
  }

  return card;
}
