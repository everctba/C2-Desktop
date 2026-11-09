import { api } from "../api";
import { t } from "../i18n";
import type { Hit, MetaFile, RelatedResult } from "../types";
import { Button } from "../ui/Button";
import { dialogField, dialogPills, openDialog } from "../ui/Dialog";
import { Input } from "../ui/Input";
import { Pill } from "../ui/Pill";
import { Toggle } from "../ui/Toggle";
import type { View } from "./view";
import styles from "./views.module.css";

export interface SearchContext {
  root: string;
}

const EXEMPLOS = [
  "onde calcula o hash dos arquivos",
  "cliente que fala com o modelo local",
  "busca semântica por significado",
];

/**
 * Semantic Grep na janela: busca enquanto digita (debounce), mostra score,
 * motivo e trecho, e abre o metadado completo do resultado (c2-e14t4).
 */
export async function renderSearch(ctx: SearchContext): Promise<View> {
  const page = document.createElement("div");
  page.className = styles.page;

  const header = document.createElement("div");
  header.className = styles.header;
  const title = document.createElement("h1");
  title.className = "pageTitle";
  title.textContent = t("Busca");
  const hint = document.createElement("p");
  hint.className = "pageHint";
  hint.textContent = t(
    "Descreva a responsabilidade que procura. O resultado mostra o motivo (campo e termos) e um trecho; clique para abrir o metadado completo.",
  );
  header.append(title, hint);
  page.appendChild(header);

  const results = document.createElement("div");
  results.className = "list";

  const summary = document.createElement("div");
  summary.className = styles.resultMeta;

  const mostrarVazio = () => {
    results.innerHTML = "";
    summary.textContent = "";
    const empty = document.createElement("div");
    empty.className = "empty";
    empty.textContent = t("digite pelo menos 2 caracteres — exemplos:");
    const exemplos = document.createElement("div");
    exemplos.className = styles.actions;
    exemplos.style.justifyContent = "center";
    exemplos.style.marginTop = "var(--space-m)";
    for (const exemplo of EXEMPLOS) {
      exemplos.appendChild(
        Button({
          label: t(exemplo),
          size: "xs",
          variant: "secondary",
          onClick: () => {
            field.input.value = exemplo;
            void run(exemplo);
          },
        }),
      );
    }
    empty.appendChild(exemplos);
    results.appendChild(empty);
  };

  let hibrida = false;

  const run = async (query: string) => {
    const termo = query.trim();
    if (termo.length < 2) {
      mostrarVazio();
      return;
    }

    const inicio = performance.now();
    results.innerHTML = "";
    summary.textContent = t("buscando");
    try {
      const hits = await api.grep(ctx.root, termo, 10, hibrida);
      const ms = Math.round(performance.now() - inicio);
      results.innerHTML = "";
      summary.textContent = `${hits.length} resultado(s) em ${ms} ms · "${termo}"`;

      if (hits.length === 0) {
        const empty = document.createElement("div");
        empty.className = "empty";
        empty.textContent = `nenhum resultado para "${termo}"`;
        results.appendChild(empty);
        return;
      }
      const maiorScore = Math.max(...hits.map((hit) => hit.score), 1);
      const termos = [...new Set(hits.flatMap((hit) => hit.evidence.flatMap((item) => item.terms)))];
      for (const hit of hits) {
        results.appendChild(resultRow(hit, ctx.root, maiorScore, termos));
      }
    } catch (error) {
      results.innerHTML = "";
      summary.textContent = "";
      const err = document.createElement("div");
      err.className = "empty";
      err.textContent = String(error);
      results.appendChild(err);
    }
  };

  /* busca enquanto digita */
  let debounce: number | undefined;
  const field = Input({
    placeholder: t("onde o sistema valida permissão de usuário"),
    size: "md",
    onInput: (value) => {
      window.clearTimeout(debounce);
      debounce = window.setTimeout(() => void run(value), 320);
    },
    onEnter: (value) => {
      window.clearTimeout(debounce);
      void run(value);
    },
  });
  page.appendChild(field.root);

  const actions = document.createElement("div");
  actions.className = styles.actions;
  actions.append(
    Button({
      label: t("Buscar"),
      iconName: "search",
      variant: "primary",
      onClick: () => void run(field.input.value),
    }),
    Button({
      label: t("Limpar"),
      variant: "ghost",
      onClick: () => {
        field.input.value = "";
        mostrarVazio();
        field.input.focus();
      },
    }),
    Toggle({
      label: t("Híbrida"),
      checked: false,
      tone: "brand",
      onChange: (ligado) => {
        hibrida = ligado;
        if (field.input.value.trim().length >= 2) void run(field.input.value);
      },
    }).root,
    summary,
  );
  page.appendChild(actions);
  page.appendChild(results);

  mostrarVazio();

  return {
    element: page,
    dispose: () => window.clearTimeout(debounce),
  };
}

/** Escapa regex para montar o destaque dos termos. */
function escaparRegex(texto: string): string {
  return texto.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/** Texto com os termos do match destacados (<mark>). */
function destacar(texto: string, termos: string[]): DocumentFragment {
  const frag = document.createDocumentFragment();
  const unicos = [...new Set(termos.filter((termo) => termo.length >= 2))].sort(
    (a, b) => b.length - a.length,
  );
  if (unicos.length === 0) {
    frag.appendChild(document.createTextNode(texto));
    return frag;
  }

  let regex: RegExp;
  try {
    regex = new RegExp(`(${unicos.map(escaparRegex).join("|")})`, "gi");
  } catch {
    frag.appendChild(document.createTextNode(texto));
    return frag;
  }

  let ultimo = 0;
  for (const match of texto.matchAll(regex)) {
    const inicio = match.index ?? 0;
    if (inicio > ultimo) {
      frag.appendChild(document.createTextNode(texto.slice(ultimo, inicio)));
    }
    const marca = document.createElement("mark");
    marca.className = styles.mark;
    marca.textContent = match[0];
    frag.appendChild(marca);
    ultimo = inicio + match[0].length;
  }
  if (ultimo < texto.length) {
    frag.appendChild(document.createTextNode(texto.slice(ultimo)));
  }
  return frag;
}

function resultRow(
  hit: Hit,
  root: string,
  maiorScore: number,
  termos: string[],
): HTMLElement {
  const row = document.createElement("div");
  row.className = styles.resultRow;

  /* cabecalho: arquivo + score */
  const head = document.createElement("div");
  head.className = styles.resultHead;
  const file = document.createElement("span");
  file.className = styles.resultFile;
  file.textContent = hit.file;
  file.title = hit.file;
  head.appendChild(file);
  head.appendChild(Pill(hit.score.toFixed(2), "accent", "xs"));
  row.appendChild(head);

  /* barra de score relativa ao melhor resultado + breakdown por campo */
  const scoreRow = document.createElement("div");
  scoreRow.className = styles.scoreRow;
  const barra = document.createElement("div");
  barra.className = styles.scoreBar;
  const preenchimento = document.createElement("div");
  preenchimento.className = styles.scoreFill;
  preenchimento.style.width = `${Math.round((hit.score / maiorScore) * 100)}%`;
  barra.appendChild(preenchimento);
  const detalhe = document.createElement("span");
  detalhe.className = styles.breakdown;
  detalhe.textContent =
    `path ${hit.path_score.toFixed(1)} · símbolo ${hit.symbol_score.toFixed(1)} · meta ${hit.semantic_score.toFixed(1)}` +
    (hit.embedding_score !== undefined
      ? ` · sem ${hit.embedding_score.toFixed(2)}`
      : "");
  scoreRow.append(barra, detalhe);
  row.appendChild(scoreRow);

  if (hit.summary) {
    const summary = document.createElement("div");
    summary.className = styles.resultSummary;
    summary.appendChild(destacar(hit.summary, termos));
    row.appendChild(summary);
  }

  /* evidencias por campo */
  const evidence = document.createElement("div");
  evidence.className = styles.evidence;
  evidence.appendChild(
    Pill(
      hit.embedding_score !== undefined
        ? "semântico"
        : hit.symbol_score > 0
          ? "símbolo"
          : "metadado",
      "neutral",
      "xs",
    ),
  );
  for (const item of hit.evidence.slice(0, 4)) {
    evidence.appendChild(Pill(`${item.field}: ${item.terms.join(", ")}`, "neutral", "xs"));
  }
  row.appendChild(evidence);

  if (hit.snippet) {
    const meta = document.createElement("div");
    meta.className = styles.resultMeta;
    meta.textContent = `linhas ${hit.snippet.line_start}-${hit.snippet.line_end} · clique para o metadado completo`;
    row.appendChild(meta);

    const pre = document.createElement("pre");
    pre.className = styles.pre;
    pre.style.maxHeight = "160px";
    pre.appendChild(destacar(hit.snippet.text, termos));
    row.appendChild(pre);
  }

  /* relacionados sem sair da busca */
  const acoes = document.createElement("div");
  acoes.className = styles.actions;
  const relacionados = document.createElement("div");

  const btnRelacionados = document.createElement("button");
  btnRelacionados.type = "button";
  btnRelacionados.textContent = t("ver relacionados");
  btnRelacionados.style.cssText =
    "height:22px;padding:0 var(--space-xs);border:0;border-radius:var(--input-radius);" +
    "background:var(--overlay-10);color:var(--text-muted);font-size:var(--text-2xs);" +
    "font-weight:700;cursor:pointer";
  btnRelacionados.addEventListener("click", async (evento) => {
    evento.stopPropagation();
    if (relacionados.childElementCount > 0) {
      relacionados.replaceChildren();
      btnRelacionados.textContent = t("ver relacionados");
      return;
    }
    try {
      const rel = await api.related(root, hit.file);
      const caixa = document.createElement("div");
      caixa.className = styles.relatedBox;
      const partes: string[] = [];
      if (rel.uses.length > 0) {
        partes.push(`usa: ${rel.uses.map((link) => link.to).join(", ")}`);
      }
      if (rel.used_by.length > 0) {
        partes.push(`usado por: ${rel.used_by.map((link) => link.to).join(", ")}`);
      }
      if (partes.length === 0) {
        partes.push("sem links estáticos");
      }
      const texto = document.createElement("span");
      texto.className = styles.breakdown;
      texto.textContent = partes.join(" · ");
      caixa.appendChild(texto);
      relacionados.appendChild(caixa);
      btnRelacionados.textContent = t("ocultar relacionados");
    } catch (erro) {
      relacionados.replaceChildren();
      const falha = document.createElement("span");
      falha.className = styles.breakdown;
      falha.textContent = String(erro);
      relacionados.appendChild(falha);
    }
  });
  acoes.appendChild(btnRelacionados);
  row.append(acoes, relacionados);

  row.addEventListener("click", () => void abrirMetadado(root, hit.file));
  return row;
}

async function abrirMetadado(root: string, file: string): Promise<void> {
  const container = document.createElement("div");
  container.style.display = "grid";
  container.style.gap = "var(--space-s)";

  try {
    const meta: MetaFile = await api.readMeta(root, file);
    container.appendChild(dialogField("arquivo", file, true));
    for (const [label, value] of [
      ["what", meta.semantic.what],
      ["why", meta.semantic.why],
      ["goal", meta.semantic.goal],
      ["summary", meta.semantic.summary],
      ["responsibility", meta.semantic.responsibility],
    ] as Array<[string, string]>) {
      container.appendChild(dialogField(label, value || "—"));
    }

    if (meta.links.length > 0) {
      const pills = dialogPills();
      for (const link of meta.links) {
        pills.appendChild(Pill(`${link.type}: ${link.to}`, "accent", "xs"));
      }
      container.appendChild(pills);
    }

    const notas = meta.agent_notes ?? [];
    if (notas.length > 0) {
      const tituloNotas = document.createElement("strong");
      tituloNotas.textContent = t("Anotações de agentes (hipóteses)");
      tituloNotas.style.marginTop = "var(--space-s)";
      container.appendChild(tituloNotas);
      for (const nota of notas) {
        const obsoleta =
          nota.hash && nota.hash !== meta.hash ? " · revisar: o arquivo mudou" : "";
        const cabecalho =
          `${nota.kind} · ${nota.by}${nota.model ? ` (${nota.model})` : ""}` + obsoleta;
        container.appendChild(dialogField(cabecalho, nota.text));
      }
    }

    const related: RelatedResult = await api.related(root, file);
    const pre = document.createElement("pre");
    pre.className = styles.pre;
    pre.textContent = [
      `usa (${related.uses.length}):`,
      ...related.uses.map((link) => `  ${link.to}  ${link.type}`),
      `usado por (${related.used_by.length}):`,
      ...related.used_by.map((link) => `  ${link.to}  ${link.type}`),
    ].join("\n");
    container.appendChild(pre);
  } catch (error) {
    container.appendChild(dialogField("erro", String(error)));
  }

  await openDialog({
    title: t("Metadado"),
    body: container,
    confirmLabel: t("Fechar"),
    cancelLabel: "",
  });
}
