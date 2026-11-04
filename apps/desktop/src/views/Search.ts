import { api } from "../api";
import type { Hit, MetaFile, RelatedResult } from "../types";
import { Button } from "../ui/Button";
import { dialogField, dialogPills, openDialog } from "../ui/Dialog";
import { Input } from "../ui/Input";
import { Pill } from "../ui/Pill";
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
  title.textContent = "Busca";
  const hint = document.createElement("p");
  hint.className = "pageHint";
  hint.textContent =
    "Descreva a responsabilidade que procura. O resultado mostra o motivo (campo e termos) e um trecho; clique para abrir o metadado completo.";
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
    empty.textContent = "digite pelo menos 2 caracteres — exemplos:";
    const exemplos = document.createElement("div");
    exemplos.className = styles.actions;
    exemplos.style.justifyContent = "center";
    exemplos.style.marginTop = "var(--space-m)";
    for (const exemplo of EXEMPLOS) {
      exemplos.appendChild(
        Button({
          label: exemplo,
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

  const run = async (query: string) => {
    const termo = query.trim();
    if (termo.length < 2) {
      mostrarVazio();
      return;
    }

    const inicio = performance.now();
    results.innerHTML = "";
    summary.textContent = "buscando...";
    try {
      const hits = await api.grep(ctx.root, termo, 10);
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
      for (const hit of hits) results.appendChild(resultRow(hit, ctx.root));
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
    placeholder: "onde o sistema valida permissão de usuário",
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
      label: "Buscar",
      iconName: "search",
      variant: "primary",
      onClick: () => void run(field.input.value),
    }),
    Button({
      label: "Limpar",
      variant: "ghost",
      onClick: () => {
        field.input.value = "";
        mostrarVazio();
        field.input.focus();
      },
    }),
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

function resultRow(hit: Hit, root: string): HTMLElement {
  const row = document.createElement("div");
  row.className = styles.resultRow;

  const head = document.createElement("div");
  head.className = styles.resultHead;
  const file = document.createElement("span");
  file.className = styles.resultFile;
  file.textContent = hit.file;
  file.title = hit.file;
  head.appendChild(file);
  head.appendChild(Pill(hit.score.toFixed(2), "accent", "xs"));
  row.appendChild(head);

  if (hit.summary) {
    const summary = document.createElement("div");
    summary.className = styles.resultSummary;
    summary.textContent = hit.summary;
    row.appendChild(summary);
  }

  const evidence = document.createElement("div");
  evidence.className = styles.evidence;
  evidence.appendChild(Pill(hit.symbol_score > 0 ? "símbolo" : "metadado", "neutral", "xs"));
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
    pre.textContent = hit.snippet.text;
    row.appendChild(pre);
  }

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
    title: "Metadado",
    body: container,
    confirmLabel: "Fechar",
    cancelLabel: "",
  });
}
