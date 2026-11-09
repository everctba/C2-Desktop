import { api, onEngineEvent } from "../api";
import { t } from "../i18n";
import type { FileEntry, IndexReport, MetaFile, RelatedResult, StatusReport } from "../types";
import { Button } from "../ui/Button";
import { Card, cardBody } from "../ui/Card";
import { Pill } from "../ui/Pill";
import { TreeRow } from "../ui/TreeRow";
import styles from "./views.module.css";
import { limparMetadados } from "./Cleanup";

export interface EngineContext {
  root: string;
  status: StatusReport | null;
  running: boolean;
  busy: boolean;
  onPlay: () => void;
  onStop: () => void;
  onReindex: (full: boolean) => void;
  onCleanupDone: () => void;
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
function runtimePillFor(status: StatusReport): HTMLElement {
  if (status.model.provider === "openai_compat") {
    return Pill(t("runtime externo"), "neutral");
  }
  return Pill(
    status.runtime.healthy ? "runtime saudável" : "runtime parado",
    status.runtime.healthy ? "success" : "neutral",
  );
}

export async function renderEngine(ctx: EngineContext): Promise<View> {
  const page = document.createElement("div");
  page.className = styles.page;

  const header = document.createElement("div");
  header.className = styles.header;
  const title = document.createElement("h1");
  title.className = "pageTitle";
  title.textContent = t("Engine");
  const hint = document.createElement("p");
  hint.className = "pageHint";
  hint.textContent = t(
    "PLAY indexa continuamente e supervisiona o llama-server. REINDEX faz uma passada só e para o runtime.",
  );
  header.append(title, hint);
  page.appendChild(header);

  /* estado atual */
  const stateRow = document.createElement("div");
  stateRow.className = styles.actions;
  let enginePill = Pill(t(ctx.running ? "engine rodando" : "engine parado"), ctx.running ? "success" : "neutral");
  let runtimePill = Pill(t("runtime —"), "neutral");
  stateRow.append(enginePill, runtimePill);
  page.appendChild(stateRow);

  /* contadores */
  const metrics = document.createElement("div");
  metrics.className = "cardGrid";
  const filesMetric = metricCard(t("Arquivos"), t("descobertos"));
  const metaMetric = metricCard(t("Metadados"), t("indexados"));
  const pendingMetric = metricCard(t("Pendentes"), t("sem metadado"));
  const callsMetric = metricCard(t("Chamadas LLM"), t("nesta máquina"));
  metrics.append(filesMetric.card, metaMetric.card, pendingMetric.card, callsMetric.card);
  page.appendChild(metrics);

  /* atividade */
  const activity = document.createElement("div");
  activity.className = styles.detail;
  const activityTitle = document.createElement("span");
  activityTitle.className = styles.fieldLabel;
  activityTitle.textContent = t("atividade");
  const activityValue = document.createElement("span");
  activityValue.className = styles.fieldValue;
  activityValue.textContent = t("ocioso");
  activity.append(activityTitle, activityValue);
  page.appendChild(activity);

  /* ações */
  const actions = document.createElement("div");
  actions.className = styles.actions;
  actions.append(
    Button({
      label: t(ctx.running ? "Parar engine" : "Iniciar engine"),
      iconName: ctx.running ? "stop" : "play",
      variant: ctx.running ? "warning" : "primary",
      busy: ctx.busy,
      onClick: ctx.running ? ctx.onStop : ctx.onPlay,
    }),
    Button({
      label: t("Reindexar tudo"),
      disabled: ctx.busy,
      title: t("Força reindexar mesmo sem mudanças; consome o modelo em todos os arquivos."),
      onClick: () => ctx.onReindex(true),
    }),
  );
  let botaoLimpar: HTMLButtonElement | undefined;
  let progressoReindex: HTMLElement | undefined;
  let textoProgresso: HTMLElement | undefined;
  if (ctx.root) {
    actions.appendChild(
      Button({
        label: t("Abrir pasta no Explorer"),
        iconName: "projects",
        size: "sm",
        variant: "ghost",
        onClick: () => {
          void api.openProjectFolder(ctx.root).catch(() => {});
        },
      }),
    );
    progressoReindex = document.createElement("div");
    progressoReindex.style.display = "none";
    progressoReindex.style.alignItems = "center";
    progressoReindex.style.gap = "var(--space-s)";
    textoProgresso = document.createElement("span");
    textoProgresso.className = "pageHint";
    textoProgresso.style.maxWidth = "none";
    textoProgresso.style.whiteSpace = "nowrap";
    const abortar = Button({
      label: t("Abortar reindexação"),
      size: "xs",
      variant: "ghost",
      onClick: () => {
        void api.reindexCancel().catch(() => {});
      },
    });
    progressoReindex.append(textoProgresso, abortar);
    actions.appendChild(progressoReindex);

    botaoLimpar = Button({
      label: t("Remove all c2.json files"),
      size: "sm",
      variant: "destructive",
      disabled: (ctx.status?.metadata ?? 0) === 0,
      onClick: () => {
        void limparMetadados(ctx.root, {
          setStatus: () => {},
          onDone: () => ctx.onCleanupDone(),
        });
      },
    });
    botaoLimpar.style.marginLeft = "auto";
    actions.appendChild(botaoLimpar);
  }
  page.appendChild(actions);

  const dicaReindex = document.createElement("p");
  dicaReindex.className = "pageHint";
  dicaReindex.style.maxWidth = "none";
  dicaReindex.textContent = t(
    "Reindexar tudo força reindexar mesmo os arquivos cujo hash não mudou. " +
      "Use quando: trocou o modelo/prompt de indexação; quer regenerar todos os metadados; " +
      "suspeita que algum metadado está ruim. É caro: chama o modelo para todos os arquivos " +
      "(na nuvem, custa tokens).",
  );
  page.appendChild(dicaReindex);

  /* log ao vivo */
  const logCard = Card({ title: t("Log"), accent: 7 });
  const logBody = cardBody(logCard);
  const logList = document.createElement("div");
  logList.className = styles.log;
  logBody.appendChild(logList);
  page.appendChild(logCard);

  /* arquivos indexados */
  const listCard = Card({ title: t("Arquivos indexados"), accent: 2 });
  const body = cardBody(listCard);
  const list = document.createElement("div");
  list.className = "list";
  body.appendChild(list);
  const detail = document.createElement("div");
  detail.className = styles.detail;
  detail.style.display = "none";
  body.appendChild(detail);
  page.appendChild(listCard);

  let carregandoLista = false;
  let ultimaContagem = ctx.status?.metadata ?? -1;
  let timerLista: number | undefined;

  const renderizarLista = (entries: FileEntry[]) => {
    list.innerHTML = "";
    if (entries.length === 0) {
      const empty = document.createElement("div");
      empty.className = "empty";
      empty.textContent = t("nenhum metadado ainda — dê play para começar.");
      list.appendChild(empty);
      return;
    }
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
  };

  const refreshLista = async () => {
    if (!ctx.root || carregandoLista) return;
    carregandoLista = true;
    try {
      renderizarLista(await api.listFiles(ctx.root));
    } catch {
      /* mantem a lista atual em caso de leitura parcial */
    } finally {
      carregandoLista = false;
    }
  };

  const agendarLista = () => {
    if (timerLista !== undefined) return;
    timerLista = window.setTimeout(() => {
      timerLista = undefined;
      void refreshLista();
    }, 400);
  };

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
      // Sem .c2.json nao ha o que limpar; o botao fica cinza.
      if (botaoLimpar) botaoLimpar.disabled = status.metadata === 0;
      const tokens = status.state.prompt_tokens + status.state.completion_tokens;
      callsMetric.set(`${status.state.llm_calls}`, `${tokens.toLocaleString("pt-BR")} tokens`);
      if (status.metadata !== ultimaContagem) {
        ultimaContagem = status.metadata;
        agendarLista();
      }

      runtimePill = replacePill(
        runtimePill,
        runtimePillFor(status),
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
      runtimePillFor(ctx.status),
    );
    setActivity(
      ctx.running
        ? t("engine iniciado, aguardando atividade")
        : t("parado — clique em Iniciar para indexar"),
    );
    appendLog(
      `${t("workspace carregado")} · ${ctx.status.files} ${t("arquivos")} · ${ctx.status.metadata} ${t("metadados")}`,
    );
  }

  await refreshLista();
  ultimaContagem = ctx.status?.metadata ?? 0;

  const unsubscribe = [
    onEngineEvent("engine://progress", (payload) => {
      const progress = payload as ProgressEvent;
      if (!progress?.rel) return;
      setActivity(`indexando ${progress.index}/${progress.total} · ${progress.decision} · ${progress.rel}`);
      agendarLista();
    }),
    onEngineEvent("engine://cycle", (payload) => {
      const report = payload as IndexReport;
      appendLog(
        `ciclo: ${report.indexed} indexados, ${report.unchanged} inalterados, ` +
          `${report.errors} erros, ${report.calls} chamadas`,
      );
      setActivity(t("aguardando próximo ciclo"));
      void refreshStatus();
      void refreshLista();
    }),
    onEngineEvent("reindex://progress", (payload) => {
      const progresso = payload as { index?: number; total?: number; rel?: string };
      if (!progressoReindex || !textoProgresso) return;
      const total = progresso.total ?? 0;
      const indice = progresso.index ?? 0;
      const pct = total > 0 ? Math.round((indice / total) * 100) : 0;
      progressoReindex.style.display = "flex";
      textoProgresso.textContent = `${t("reindexando")}… ${pct}% (${indice} ${t("de")} ${total})`;
    }),
    onEngineEvent("engine://error", (payload) => {
      appendLog(`erro: ${String(payload)}`);
      void refreshStatus();
    }),
    onEngineEvent("engine://stopped", () => {
      appendLog("engine encerrado");
      setActivity(t("parado"));
      void refreshStatus();
    }),
  ];

  const timer = window.setInterval(() => void refreshStatus(), 2500);
  if (ctx.running) appendLog("engine em execução — acompanhando eventos");

  return {
    element: page,
    dispose: () => {
      window.clearInterval(timer);
      if (timerLista !== undefined) window.clearTimeout(timerLista);
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
      linksRow.appendChild(Pill(t("links"), "neutral", "xs"));
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
