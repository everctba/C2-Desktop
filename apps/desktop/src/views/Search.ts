import { api } from "../api";
import type { Hit } from "../types";
import { Button } from "../ui/Button";
import { Input } from "../ui/Input";
import { Pill } from "../ui/Pill";
import type { View } from "./view";
import styles from "./views.module.css";

export interface SearchContext {
  root: string;
}

/** Semantic Grep: pergunta em linguagem natural, com motivo e trecho. */
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
    "Descreva a responsabilidade que procura. O resultado mostra o motivo (campo e termos) e um trecho do arquivo.";
  header.append(title, hint);
  page.appendChild(header);

  const results = document.createElement("div");
  results.className = "list";

  const run = async (query: string) => {
    results.innerHTML = "";
    if (!query.trim()) return;

    const loading = document.createElement("div");
    loading.className = "empty";
    loading.textContent = "buscando...";
    results.appendChild(loading);

    try {
      const hits = await api.grep(ctx.root, query, 10);
      results.innerHTML = "";
      if (hits.length === 0) {
        const empty = document.createElement("div");
        empty.className = "empty";
        empty.textContent = `nenhum resultado para "${query}"`;
        results.appendChild(empty);
        return;
      }
      for (const hit of hits) results.appendChild(resultRow(hit));
    } catch (error) {
      results.innerHTML = "";
      const err = document.createElement("div");
      err.className = "empty";
      err.textContent = String(error);
      results.appendChild(err);
    }
  };

  const field = Input({
    placeholder: "onde o sistema valida permissão de usuário",
    size: "md",
    mono: false,
    onEnter: (value) => void run(value),
  });
  page.appendChild(field.root);

  const actions = document.createElement("div");
  actions.className = styles.actions;
  actions.appendChild(
    Button({
      label: "Buscar",
      iconName: "search",
      variant: "primary",
      onClick: () => void run(field.input.value),
    }),
  );
  actions.appendChild(
    Button({
      label: "Limpar",
      variant: "ghost",
      onClick: () => {
        field.input.value = "";
        results.innerHTML = "";
      },
    }),
  );
  page.appendChild(actions);

  page.appendChild(results);
  return { element: page };
}

function resultRow(hit: Hit): HTMLElement {
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
  evidence.appendChild(Pill(`${hit.symbol_score > 0 ? "símbolo" : "metadado"}`, "neutral", "xs"));
  for (const item of hit.evidence.slice(0, 4)) {
    evidence.appendChild(Pill(`${item.field}: ${item.terms.join(", ")}`, "neutral", "xs"));
  }
  row.appendChild(evidence);

  if (hit.snippet) {
    const pre = document.createElement("pre");
    pre.className = styles.pre;
    pre.style.display = "none";
    pre.textContent = `${hit.file} (linhas ${hit.snippet.line_start}-${hit.snippet.line_end})\n${hit.snippet.text}`;
    row.appendChild(pre);

    const meta = document.createElement("div");
    meta.className = styles.resultMeta;
    meta.textContent = `linhas ${hit.snippet.line_start}-${hit.snippet.line_end}`;
    row.appendChild(meta);

    row.addEventListener("click", () => {
      pre.style.display = pre.style.display === "none" ? "block" : "none";
    });
  }

  return row;
}
