import { api } from "../api";
import { t } from "../i18n";
import type {
  AgentNote,
  MetaFile,
  NoteEntry,
  NotesInventory,
  UsageEvent,
  UsageSummary,
} from "../types";
import { Button } from "../ui/Button";
import { Card, cardBody } from "../ui/Card";
import { dialogField, openDialog } from "../ui/Dialog";
import { Pill } from "../ui/Pill";
import { Select } from "../ui/Select";
import type { View } from "./view";
import styles from "./views.module.css";

export interface MemoryContext {
  root: string;
}

const KINDS = ["teleology", "invariant", "gotcha", "correction", "pending", "note"];
const KIND_PRIORITY: Record<string, number> = {
  correction: 0,
  gotcha: 1,
  invariant: 2,
  pending: 3,
  teleology: 4,
  note: 5,
};

/** Pagina de memoria: anotacoes, mapa de confusao e estatisticas de uso. */
export async function renderMemory(ctx: MemoryContext): Promise<View> {
  const page = document.createElement("div");
  page.className = styles.page;

  const header = document.createElement("div");
  header.className = styles.header;
  const title = document.createElement("h1");
  title.className = "pageTitle";
  title.textContent = t("Memória");
  const hint = document.createElement("p");
  hint.className = "pageHint";
  hint.textContent = t(
    "O que a IA aprendeu neste projeto: gotchas, correções, invariantes e pendências. Notas obsoletas aparecem para revisão.",
  );
  header.append(title, hint);
  page.appendChild(header);

  let inventory: NotesInventory | null = null;
  let usage: UsageSummary | null = null;
  let events: UsageEvent[] = [];
  try {
    [inventory, usage, events] = await Promise.all([
      api.notesInventory(ctx.root),
      api.usageSummary(ctx.root),
      api.usageEvents(ctx.root, 2000),
    ]);
  } catch {
    inventory = null;
  }

  const stats = document.createElement("div");
  stats.className = "cardGrid";
  page.appendChild(stats);

  const addStat = (label: string, value: string, hintText?: string) => {
    const card = Card();
    const body = cardBody(card);
    const valor = document.createElement("div");
    valor.className = "pageTitle";
    valor.style.fontSize = "var(--text-xl)";
    valor.textContent = value;
    const rotulo = document.createElement("div");
    rotulo.className = styles.fieldLabel;
    rotulo.textContent = label;
    body.append(valor, rotulo);
    if (hintText) {
      const dica = document.createElement("div");
      dica.className = styles.breakdown;
      dica.textContent = hintText;
      body.appendChild(dica);
    }
    stats.appendChild(card);
  };

  const byKind = inventory?.by_kind ?? {};
  const gotchas = (byKind.gotcha ?? 0) + (byKind.correction ?? 0);
  addStat(t("Anotações"), String(inventory?.total ?? 0), `${inventory?.stale ?? 0} ${t("obsoletas")}`);
  addStat(t("Gotchas + correções"), String(gotchas), t("o que mais evita retrabalho"));
  addStat(t("Chamadas MCP/CLI"), String(usage?.calls ?? 0), t("desde o último log"));
  addStat(t("Metadados lidos"), String(usage?.meta_reads ?? 0), t("leituras efetivas"));

  /* Mapa de confusão */
  const confusion = Card({ title: t("Mapa de confusão"), accent: 3 });
  const confusionBody = cardBody(confusion);
  const confusionTexto = document.createElement("p");
  confusionTexto.className = "pageHint";
  confusionTexto.style.maxWidth = "none";
  confusionTexto.textContent = t(
    "Onde a IA acumula gotchas/correções — candidatos a documentação, testes ou refatoração.",
  );
  confusionBody.appendChild(confusionTexto);
  const dominios = new Map<string, { total: number; confuso: number }>();
  for (const nota of inventory?.notes ?? []) {
    const atual = dominios.get(nota.domain) ?? { total: 0, confuso: 0 };
    atual.total += 1;
    if (nota.note.kind === "gotcha" || nota.note.kind === "correction") atual.confuso += 1;
    dominios.set(nota.domain, atual);
  }
  const ranking = [...dominios.entries()].sort(
    (a, b) => b[1].confuso - a[1].confuso || b[1].total - a[1].total,
  );
  if (ranking.length === 0) {
    const vazio = document.createElement("div");
    vazio.className = "empty";
    vazio.textContent = t("Nenhuma anotação ainda.");
    confusionBody.appendChild(vazio);
  } else {
    for (const [dominio, valor] of ranking.slice(0, 6)) {
      const linha = document.createElement("div");
      linha.className = styles.field;
      const nome = document.createElement("span");
      nome.className = styles.mono;
      nome.textContent = dominio;
      const numero = document.createElement("span");
      numero.className = styles.breakdown;
      numero.textContent = `${valor.confuso} gotchas/correções · ${valor.total} notas`;
      linha.append(nome, numero);
      confusionBody.appendChild(linha);
    }
  }
  page.appendChild(confusion);

  /* Filtros */
  const filtros = Card({ title: t("Anotações"), accent: 2 });
  const filtrosBody = cardBody(filtros);
  const controles = document.createElement("div");
  controles.className = styles.actions;
  let filtroKind = "todos";
  let filtroDominio = "todos";
  let filtroPeriodo = "todos";
  const lista = document.createElement("div");
  lista.className = "list";

  const renderLista = () => {
    lista.innerHTML = "";
    const agora = Date.now();
    const dias = filtroPeriodo === "hoje" ? 1 : Number(filtroPeriodo);
    const notas = (inventory?.notes ?? [])
      .filter((item) => filtroKind === "todos" || item.note.kind === filtroKind)
      .filter((item) => filtroDominio === "todos" || item.domain === filtroDominio)
      .filter((item) => {
        if (filtroPeriodo === "todos") return true;
        const quando = Date.parse(item.note.at);
        return Number.isFinite(quando) && agora - quando <= dias * 86_400_000;
      })
      .sort(
        (a, b) =>
          (KIND_PRIORITY[a.note.kind] ?? 9) - (KIND_PRIORITY[b.note.kind] ?? 9) ||
          b.note.at.localeCompare(a.note.at),
      );
    if (notas.length === 0) {
      const vazio = document.createElement("div");
      vazio.className = "empty";
      vazio.textContent = t("Nenhuma anotação com esses filtros.");
      lista.appendChild(vazio);
      return;
    }
    for (const nota of notas) lista.appendChild(linhaNota(nota));
  };

  const linhaNota = (item: NoteEntry): HTMLElement => {
    const row = document.createElement("div");
    row.className = styles.resultRow;
    const head = document.createElement("div");
    head.className = styles.resultHead;
    const file = document.createElement("span");
    file.className = styles.resultFile;
    file.textContent = item.file;
    head.appendChild(file);
    head.appendChild(Pill(item.note.kind, "neutral", "xs"));
    if (item.stale) head.appendChild(Pill(t("possivelmente obsoleta"), "warning", "xs"));
    if (item.missing_hash) head.appendChild(Pill(t("sem hash de origem"), "neutral", "xs"));
    row.appendChild(head);

    const texto = document.createElement("div");
    texto.className = styles.resultSummary;
    texto.textContent = item.note.text;
    row.appendChild(texto);

    const meta = document.createElement("div");
    meta.className = styles.breakdown;
    meta.textContent = [
      item.note.task ? `${t("tarefa")}: ${item.note.task}` : "",
      item.note.by,
      item.note.model ?? "",
      item.note.at,
    ]
      .filter(Boolean)
      .join(" · ");
    row.appendChild(meta);

    const acoes = document.createElement("div");
    acoes.className = styles.actions;
    acoes.style.marginTop = "var(--space-s)";
    acoes.append(
      Button({
        label: t("abrir metadado"),
        size: "xs",
        variant: "secondary",
        onClick: () => void abrirMetadado(ctx.root, item.file, item.note),
      }),
      Button({
        label: t("apagar nota"),
        size: "xs",
        variant: "ghost",
        onClick: () => {
          void (async () => {
            const ok = await openDialog({
              title: t("Apagar anotação?"),
              body: dialogField(item.file, item.note.text, true),
              confirmLabel: t("Apagar"),
              cancelLabel: t("Cancelar"),
              destructive: true,
            });
            if (!ok) return;
            await api.noteRemove(ctx.root, item.file, item.index).catch(() => {});
            inventory = await api.notesInventory(ctx.root).catch(() => inventory);
            renderLista();
          })();
        },
      }),
    );
    row.appendChild(acoes);
    return row;
  };

  const dominiosDisponiveis = ["todos", ...new Set((inventory?.notes ?? []).map((n) => n.domain))];
  controles.append(
    Select({
      label: t("tipo"),
      value: "todos",
      options: ["todos", ...KINDS],
      onChange: (valor) => {
        filtroKind = valor;
        renderLista();
      },
    }).root,
    Select({
      label: t("domínio"),
      value: "todos",
      options: dominiosDisponiveis,
      onChange: (valor) => {
        filtroDominio = valor;
        renderLista();
      },
    }).root,
    Select({
      label: t("período"),
      value: "todos",
      options: ["todos", "hoje", "7", "30"],
      onChange: (valor) => {
        filtroPeriodo = valor;
        renderLista();
      },
    }).root,
  );
  filtrosBody.append(controles, lista);
  page.appendChild(filtros);

  /* estatísticas de uso */
  const usoCard = Card({ title: t("Estatísticas de uso"), accent: 6 });
  const usoBody = cardBody(usoCard);
  const atores = usage?.by_actor ?? {};
  const ferramentas = usage?.by_tool ?? {};
  const resumo = document.createElement("div");
  resumo.className = styles.breakdown;
  resumo.textContent = [
    `MCP ${atores.mcp ?? 0}`,
    `CLI ${atores.cli ?? 0}`,
    `desktop ${atores.desktop ?? 0}`,
    `${usage?.annotations ?? 0} ${t("anotações")}`,
    `${events.length} ${t("eventos")}`,
  ].join(" · ");
  usoBody.appendChild(resumo);
  const topFerramentas = Object.entries(ferramentas)
    .sort((a, b) => b[1] - a[1])
    .slice(0, 8);
  for (const [ferramenta, quantidade] of topFerramentas) {
    const linha = document.createElement("div");
    linha.className = styles.field;
    const nome = document.createElement("span");
    nome.className = styles.mono;
    nome.textContent = ferramenta;
    const valor = document.createElement("span");
    valor.className = styles.breakdown;
    valor.textContent = `${quantidade} chamadas`;
    linha.append(nome, valor);
    usoBody.appendChild(linha);
  }
  page.appendChild(usoCard);

  renderLista();

  return { element: page };
}

async function abrirMetadado(root: string, file: string, nota: AgentNote): Promise<void> {
  const meta: MetaFile | null = await api.readMeta(root, file).catch(() => null);
  const corpo = document.createElement("div");
  corpo.style.display = "grid";
  corpo.style.gap = "var(--space-s)";
  corpo.appendChild(dialogField("arquivo", file, true));
  if (meta) {
    for (const [label, value] of [
      ["what", meta.semantic.what],
      ["why", meta.semantic.why],
      ["goal", meta.semantic.goal],
      ["summary", meta.semantic.summary],
      ["responsibility", meta.semantic.responsibility],
    ] as Array<[string, string]>) {
      corpo.appendChild(dialogField(label, value));
    }
  }
  corpo.appendChild(dialogField("anotação", nota.text, true));
  await openDialog({
    title: file,
    body: corpo,
    confirmLabel: t("Fechar"),
    cancelLabel: "",
  });
}
