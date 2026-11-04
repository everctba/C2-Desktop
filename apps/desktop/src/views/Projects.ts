import { api, hasTauri } from "../api";
import { Button } from "../ui/Button";
import { Card, cardBody, cardFooter } from "../ui/Card";
import { dialogField, dialogPills, openDialog } from "../ui/Dialog";
import { Pill } from "../ui/Pill";
import type { ModelInspection, StatusReport } from "../types";
import styles from "./views.module.css";

export interface ProjectsContext {
  root: string;
  status: StatusReport | null;
  error: string | null;
  onPickProject: () => void;
  onChanged: () => void;
}

/** Tela inicial: abrir/inicializar projeto, modelo e runtime. */
export async function renderProjects(ctx: ProjectsContext): Promise<HTMLElement> {
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
    "Abra a pasta de um projeto para o C2 indexar. O índice fica em .c2/ e os metadados ao lado de cada arquivo.";
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
            location.reload();
          },
        }),
      );
    }
    empty.appendChild(actions);
    page.appendChild(empty);
    return page;
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
  page.appendChild(actions);

  if (!hasTauri) {
    const note = document.createElement("p");
    note.className = "pageHint";
    note.textContent =
      "Modo preview no navegador: dados falsos. Rode `pnpm tauri dev` para usar o Core real.";
    page.appendChild(note);
  }

  return page;
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
  name.textContent = status.model.name;
  body.appendChild(name);

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
    Button({
      label: "Selecionar .gguf…",
      iconName: "projects",
      size: "sm",
      onClick: () => void pickModelFlow(ctx),
    }),
  );
  footer.appendChild(
    Pill(status.model.disable_thinking ? "sem raciocínio" : "com raciocínio", "neutral", "xs"),
  );
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
