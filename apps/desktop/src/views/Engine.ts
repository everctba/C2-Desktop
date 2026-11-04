import { api } from "../api";
import type { FileEntry, MetaFile, RelatedResult, StatusReport } from "../types";
import { Button } from "../ui/Button";
import { Card, cardBody } from "../ui/Card";
import { Pill } from "../ui/Pill";
import { TreeRow } from "../ui/TreeRow";
import styles from "./views.module.css";

export interface EngineContext {
  root: string;
  status: StatusReport | null;
  onPlay: () => void;
  onStop: () => void;
  onReindex: (full: boolean) => void;
  running: boolean;
  busy: boolean;
}

/** Tela principal: estado do engine, contadores e arquivos indexados. */
export async function renderEngine(ctx: EngineContext): Promise<HTMLElement> {
  const page = document.createElement("div");
  page.className = styles.page;

  const header = document.createElement("div");
  header.className = styles.header;
  const title = document.createElement("h1");
  title.className = "pageTitle";
  title.textContent = "Engine";
  const hint = document.createElement("p");
  hint.className = "pageHint";
  hint.textContent =
    "PLAY indexa continuamente e supervisiona o llama-server. REINDEX faz uma passada só e para o runtime.";
  header.append(title, hint);
  page.appendChild(header);

  const metrics = document.createElement("div");
  metrics.className = "cardGrid";
  metrics.append(
    metricCard("Arquivos", ctx.status?.files ?? 0, "descobertos"),
    metricCard("Metadados", ctx.status?.metadata ?? 0, "indexados"),
    metricCard("Chamadas LLM", ctx.status?.state.llm_calls ?? 0, "desde o início"),
    metricCard(
      "Tokens",
      (ctx.status?.state.prompt_tokens ?? 0) + (ctx.status?.state.completion_tokens ?? 0),
      "entrada + saída",
    ),
  );
  page.appendChild(metrics);

  const actions = document.createElement("div");
  actions.className = styles.actions;
  actions.append(
    Button({
      label: ctx.running ? "Parar engine" : "Iniciar engine",
      iconName: ctx.running ? "stop" : "play",
      variant: ctx.running ? "secondary" : "primary",
      busy: ctx.busy,
      onClick: ctx.running ? ctx.onStop : ctx.onPlay,
    }),
    Button({
      label: "Reindexar (10)",
      iconName: "refresh",
      disabled: ctx.busy,
      onClick: () => ctx.onReindex(false),
    }),
    Button({
      label: "Reindexar tudo",
      disabled: ctx.busy,
      onClick: () => ctx.onReindex(true),
    }),
  );
  page.appendChild(actions);

  const listCard = Card({ title: "Arquivos indexados", accent: 2 });
  const body = cardBody(listCard);
  const list = document.createElement("div");
  list.className = "list";
  body.appendChild(list);

  const detail = document.createElement("div");
  detail.className = styles.detail;
  detail.style.display = "none";
  body.appendChild(detail);

  let entries: FileEntry[] = [];
  try {
    entries = await api.listFiles(ctx.root);
  } catch {
    entries = [];
  }

  if (entries.length === 0) {
    const empty = document.createElement("div");
    empty.className = "empty";
    empty.textContent = "nenhum metadado ainda — rode REINDEXAR";
    list.appendChild(empty);
  } else {
    for (const entry of entries) {
      list.appendChild(
        TreeRow({
          label: entry.file,
          secondary: entry.summary,
          badge: { text: `${entry.links} links`, tone: entry.links > 0 ? "neutral" : "warning" },
          onClick: async () => {
            detail.style.display = "grid";
            await showDetail(detail, ctx.root, entry.file);
          },
        }),
      );
    }
  }

  page.appendChild(listCard);
  return page;
}

function metricCard(label: string, value: number, hint: string): HTMLElement {
  const card = Card();
  const body = cardBody(card);
  const metric = document.createElement("div");
  metric.className = "metric";
  metric.textContent = value.toLocaleString("pt-BR");
  const caption = document.createElement("div");
  caption.className = "metricLabel";
  caption.textContent = label;
  const small = document.createElement("div");
  small.className = "pageHint";
  small.textContent = hint;
  body.append(metric, caption, small);
  return card;
}

async function showDetail(container: HTMLElement, root: string, file: string): Promise<void> {
  container.innerHTML = "";
  const titleRow = document.createElement("div");
  titleRow.className = styles.detailTitle;
  const title = document.createElement("span");
  title.className = "mono";
  title.textContent = file;
  titleRow.appendChild(title);
  container.appendChild(titleRow);

  try {
    const meta: MetaFile = await api.readMeta(root, file);
    const fields: Array<[string, string]> = [
      ["what", meta.semantic.what],
      ["why", meta.semantic.why],
      ["goal", meta.semantic.goal],
      ["summary", meta.semantic.summary],
      ["responsibility", meta.semantic.responsibility],
    ];
    for (const [label, value] of fields) {
      const field = document.createElement("div");
      field.className = styles.field;
      const fieldLabel = document.createElement("span");
      fieldLabel.className = styles.fieldLabel;
      fieldLabel.textContent = label;
      const text = document.createElement("span");
      text.className = styles.fieldValue;
      text.style.whiteSpace = "normal";
      text.textContent = value || "—";
      field.append(fieldLabel, text);
      container.appendChild(field);
    }

    if (meta.links.length > 0) {
      const linksRow = document.createElement("div");
      linksRow.className = styles.evidence;
      linksRow.appendChild(Pill("links", "neutral", "xs"));
      for (const link of meta.links) {
        linksRow.appendChild(Pill(`${link.type}: ${link.to}`, "accent", "xs"));
      }
      container.appendChild(linksRow);
    }

    const related: RelatedResult = await api.related(root, file);
    const relatedText = document.createElement("pre");
    relatedText.className = styles.pre;
    relatedText.textContent = [
      `usa (${related.uses.length}):`,
      ...related.uses.map((link) => `  ${link.to}  ${link.type}`),
      `usado por (${related.used_by.length}):`,
      ...related.used_by.map((link) => `  ${link.to}  ${link.type}`),
    ].join("\n");
    container.appendChild(relatedText);
  } catch (error) {
    const err = document.createElement("div");
    err.className = "empty";
    err.textContent = String(error);
    container.appendChild(err);
  }
}
