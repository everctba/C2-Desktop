import { api, onEngineEvent } from "../api";
import type { FileEntry, IndexReport, MetaFile, RelatedResult, StatusReport } from "../types";
import { Button } from "../ui/Button";
import { Card, cardBody } from "../ui/Card";
import { Pill } from "../ui/Pill";
import { TreeRow } from "../ui/TreeRow";
import styles from "./views.module.css";

export interface EngineContext {
  root: string;
  status: StatusReport | null;
  running: boolean;
  busy: boolean;
  onPlay: () => void;
  onStop: () => void;
  onReindex: (full: boolean) => void;
}

import type { View } from "./view";

interface ProgressEvent {
  rel: string;
  decision: string;
  index: number;
  total: number;
}

/**
 * Tela Engine: contadores, estado do motor/runtime, atividade atual e log.
 * Os números e o log são atualizados por evento e por polling leve, direto no
 * DOM — sem re-render da tela (c2-e14t2).
 */
export async function renderEngine(ctx: EngineContext): Promise<View> {
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

  /* estado atual */
  const stateRow = document.createElement("div");
  stateRow.className = styles.actions;
  let enginePill = Pill(ctx.running ? "engine rodando" : "engine parado", ctx.running ? "success" : "neutral");
  let runtimePill = Pill("runtime —", "neutral");
  stateRow.append(enginePill, runtimePill);
  page.appendChild(stateRow);

  /* contadores */
  const metrics = document.createElement("div");
  metrics.className = "cardGrid";
  const filesMetric = metricCard("Arquivos", "descobertos");
  const metaMetric = metricCard("Metadados", "indexados");
  const pendingMetric = metricCard("Pendentes", "sem metadado");
  const callsMetric = metricCard("Chamadas LLM", "nesta máquina");
  metrics.append(filesMetric.card, metaMetric.card, pendingMetric.card, callsMetric.card);
  page.appendChild(metrics);

  /* atividade */
  const activity = document.createElement("div");
  activity.className = styles.detail;
  const activityTitle = document.createElement("span");
  activityTitle.className = styles.fieldLabel;
  activityTitle.textContent = "atividade";
  const activityValue = document.createElement("span");
  activityValue.className = styles.fieldValue;
  activityValue.textContent = "ocioso";
  activity.append(activityTitle, activityValue);
  page.appendChild(activity);

  /* ações */
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

  /* log ao vivo */
  const logCard = Card({ title: "Log", accent: 7 });
  const logBody = cardBody(logCard);
  const logList = document.createElement("div");
  logList.className = styles.log;
  logBody.appendChild(logList);
  page.appendChild(logCard);

  /* arquivos indexados */
  const listCard = Card({ title: "Arquivos indexados", accent: 2 });
  const body = cardBody(listCard);
  const list = document.createElement("div");
  list.className = "list";
  body.appendChild(list);
  const detail = document.createElement("div");
  detail.className = styles.detail;
  detail.style.display = "none";
  body.appendChild(detail);
  page.appendChild(listCard);

  const appendLog = (message: string) => {
    const line = document.createElement("div");
    line.className = styles.logLine;
    const time = document.createElement("span");
    time.className = styles.logTime;
    time.textContent = new Date().toLocaleTimeString("pt-BR");
    const text = document.createElement("span");
    text.textContent = message;
    line.append(time, text);
    logList.prepend(line);
    while (logList.childElementCount > 80) logList.lastElementChild?.remove();
  };

  const setActivity = (message: string) => {
    activityValue.textContent = message;
  };

  const replacePill = (current: HTMLElement, next: HTMLElement) => {
    current.replaceWith(next);
    return next;
  };

  const refreshStatus = async () => {
    if (!ctx.root) return;
    try {
      const status = await api.status(ctx.root);
      if (!status) return;
      filesMetric.set(String(status.files));
      metaMetric.set(String(status.metadata));
      pendingMetric.set(String(Math.max(0, status.files - status.metadata)));
      const tokens = status.state.prompt_tokens + status.state.completion_tokens;
      callsMetric.set(`${status.state.llm_calls}`, `${tokens.toLocaleString("pt-BR")} tokens`);

      runtimePill = replacePill(
        runtimePill,
        Pill(status.runtime.healthy ? "runtime saudável" : "runtime parado", status.runtime.healthy ? "success" : "neutral"),
      );
      const running = await api.engineRunning();
      enginePill = replacePill(
        enginePill,
        Pill(running ? "engine rodando" : "engine parado", running ? "success" : "neutral"),
      );
    } catch (error) {
      appendLog(`erro ao atualizar estado: ${String(error)}`);
    }
  };

  if (ctx.status) {
    filesMetric.set(String(ctx.status.files));
    metaMetric.set(String(ctx.status.metadata));
    pendingMetric.set(String(Math.max(0, ctx.status.files - ctx.status.metadata)));
    const tokens = ctx.status.state.prompt_tokens + ctx.status.state.completion_tokens;
    callsMetric.set(`${ctx.status.state.llm_calls}`, `${tokens.toLocaleString("pt-BR")} tokens`);
    runtimePill = replacePill(
      runtimePill,
      Pill(ctx.status.runtime.healthy ? "runtime saudável" : "runtime parado", ctx.status.runtime.healthy ? "success" : "neutral"),
    );
    setActivity(ctx.running ? "engine iniciado, aguardando atividade" : "parado — clique em Iniciar para indexar");
    appendLog(`workspace carregado · ${ctx.status.files} arquivos · ${ctx.status.metadata} metadados`);
  }

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

  const unsubscribe = [
    onEngineEvent("engine://progress", (payload) => {
      const progress = payload as ProgressEvent;
      if (!progress?.rel) return;
      setActivity(`indexando ${progress.index}/${progress.total} · ${progress.decision} · ${progress.rel}`);
    }),
    onEngineEvent("engine://cycle", (payload) => {
      const report = payload as IndexReport;
      appendLog(
        `ciclo: ${report.indexed} indexados, ${report.unchanged} inalterados, ` +
          `${report.errors} erros, ${report.calls} chamadas`,
      );
      setActivity("aguardando próximo ciclo");
      void refreshStatus();
    }),
    onEngineEvent("engine://error", (payload) => {
      appendLog(`erro: ${String(payload)}`);
      void refreshStatus();
    }),
    onEngineEvent("engine://stopped", () => {
      appendLog("engine encerrado");
      setActivity("parado");
      void refreshStatus();
    }),
  ];

  const timer = window.setInterval(() => void refreshStatus(), 2500);
  if (ctx.running) appendLog("engine em execução — acompanhando eventos");

  return {
    element: page,
    dispose: () => {
      window.clearInterval(timer);
      for (const off of unsubscribe) off();
    },
  };
}

function metricCard(
  label: string,
  hint: string,
): { card: HTMLElement; set: (value: string, extra?: string) => void } {
  const card = Card();
  const body = cardBody(card);
  const value = document.createElement("div");
  value.className = "metric";
  value.textContent = "0";
  const caption = document.createElement("div");
  caption.className = "metricLabel";
  caption.textContent = label;
  const extra = document.createElement("div");
  extra.className = "pageHint";
  extra.textContent = hint;
  body.append(value, caption, extra);
  return {
    card,
    set: (next: string, extraText?: string) => {
      value.textContent = next;
      if (extraText) extra.textContent = extraText;
    },
  };
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
