import { api } from "../api";
import { t } from "../i18n";
import type { ForestData } from "../types";
import { Button } from "../ui/Button";
import { Card, cardBody } from "../ui/Card";
import { Pill } from "../ui/Pill";
import type { View } from "./view";
import styles from "./views.module.css";

export interface ForestContext {
  root: string;
}

/** Visao da floresta: teleologia, dominios, riscos e divergencias. */
export async function renderForest(ctx: ForestContext): Promise<View> {
  const page = document.createElement("div");
  page.className = styles.page;

  const desenhar = async (forest: ForestData | null) => {
    page.innerHTML = "";

    const header = document.createElement("div");
    header.className = styles.header;
    const title = document.createElement("h1");
    title.className = "pageTitle";
    title.textContent = t("Floresta");
    const hint = document.createElement("p");
    hint.className = "pageHint";
    hint.textContent = t(
      "A visão do projeto inteiro: propósito, domínios, decisões, riscos e divergências entre docs e código.",
    );
    header.append(title, hint);
    page.appendChild(header);

    const acoes = document.createElement("div");
    acoes.className = styles.actions;
    acoes.style.marginBottom = "var(--space-m)";
    const status = document.createElement("span");
    status.className = "pageHint";
    status.style.maxWidth = "none";
    acoes.append(
      Button({
        label: forest ? t("Atualizar floresta") : t("Gerar floresta"),
        variant: "primary",
        onClick: () => {
          void (async () => {
            status.textContent = t("gerando floresta…");
            try {
              const report = await api.forestBuild(ctx.root);
              status.textContent = `${report.folders} folders · ${report.files} arquivos · ${report.notes} notas`;
              const atual = await api.forestOverview(ctx.root).catch(() => null);
              await desenhar(atual);
            } catch (erro) {
              status.textContent = `erro: ${String(erro)}`;
            }
          })();
        },
      }),
      status,
    );
    page.appendChild(acoes);

    if (!forest) {
      const vazio = Card({ title: t("Floresta ainda não gerada") });
      const body = cardBody(vazio);
      const texto = document.createElement("p");
      texto.className = "pageHint";
      texto.style.maxWidth = "none";
      texto.textContent = t(
        "Gere a floresta para sintetizar os domínios a partir dos metadados e docs.",
      );
      body.appendChild(texto);
      page.appendChild(vazio);
      return;
    }

    const visao = Card({ title: t("Visão e propósito"), accent: 1 });
    const visaoBody = cardBody(visao);
    const what = document.createElement("h2");
    what.style.margin = "0";
    what.textContent = forest.teleology.what;
    const why = document.createElement("p");
    why.className = "pageHint";
    why.style.maxWidth = "none";
    why.textContent = forest.teleology.why;
    const goal = document.createElement("div");
    goal.className = styles.breakdownWrap;
    goal.textContent = `${t("objetivo")}: ${forest.teleology.goal}`;
    visaoBody.append(what, why, goal);
    page.appendChild(visao);

    const arquitetura = Card({ title: t("Arquitetura"), accent: 2 });
    const arquiteturaBody = cardBody(arquitetura);
    const resumo = document.createElement("p");
    resumo.className = "pageHint";
    resumo.style.maxWidth = "none";
    resumo.textContent = forest.architecture.summary;
    arquiteturaBody.append(resumo);
    for (const layer of forest.architecture.layers.slice(0, 12)) {
      const linha = document.createElement("div");
      linha.className = styles.field;
      const nome = document.createElement("span");
      nome.className = styles.mono;
      nome.textContent = layer;
      const entrada = document.createElement("span");
      entrada.className = styles.breakdownWrap;
      entrada.textContent = t("domínio");
      linha.append(nome, entrada);
      arquiteturaBody.appendChild(linha);
    }
    page.appendChild(arquitetura);

    const dominios = Card({ title: t("Domínios"), accent: 3 });
    const dominiosBody = cardBody(dominios);
    for (const dominio of forest.domains) {
      const row = document.createElement("div");
      row.className = styles.resultRow;
      const head = document.createElement("div");
      head.className = styles.resultHead;
      const file = document.createElement("span");
      file.className = styles.resultFile;
      file.textContent = dominio.path;
      head.append(file, Pill(`${dominio.key_files.length} ${t("arquivos")}`, "neutral", "xs"));
      const texto = document.createElement("div");
      texto.className = styles.resultSummary;
      texto.textContent = dominio.what;
      row.append(head, texto);
      if (dominio.why) {
        const why = document.createElement("div");
        why.className = styles.breakdownWrap;
        why.textContent = dominio.why;
        row.appendChild(why);
      }
      dominiosBody.appendChild(row);
    }
    page.appendChild(dominios);

    if (forest.decisions.length > 0 || forest.risks.length > 0) {
      const decisoes = Card({ title: t("Decisões e riscos"), accent: 5 });
      const decisoesBody = cardBody(decisoes);
      for (const decisao of forest.decisions.slice(0, 10)) {
        const linha = document.createElement("div");
        linha.className = styles.field;
        const nome = document.createElement("span");
        nome.className = styles.mono;
        nome.textContent = decisao.title;
        const texto = document.createElement("span");
        texto.className = styles.breakdownWrap;
        texto.textContent = decisao.summary;
        linha.append(nome, texto);
        decisoesBody.appendChild(linha);
      }
      for (const risco of forest.risks.slice(0, 10)) {
        const linha = document.createElement("div");
        linha.className = styles.field;
        const nome = document.createElement("span");
        nome.className = styles.mono;
        nome.textContent = risco.title;
        const texto = document.createElement("span");
        texto.className = styles.breakdownWrap;
        texto.textContent = risco.summary;
        linha.append(nome, texto);
        decisoesBody.appendChild(linha);
      }
      page.appendChild(decisoes);
    }

    if (forest.divergences.length > 0) {
      const div = Card({ title: t("Divergências"), accent: 7 });
      const divBody = cardBody(div);
      for (const item of forest.divergences.slice(0, 20)) {
        const linha = document.createElement("div");
        linha.className = styles.field;
        const nome = document.createElement("span");
        nome.className = styles.mono;
        nome.textContent = item.kind;
        const texto = document.createElement("span");
        texto.className = styles.breakdownWrap;
        texto.textContent = `${item.claim} · docs: ${item.docs_ref} · código: ${item.code_ref}`;
        linha.append(nome, texto);
        divBody.appendChild(linha);
      }
      page.appendChild(div);
    }
  };

  const inicial = await api.forestOverview(ctx.root).catch(() => null);
  await desenhar(inicial);
  return { element: page };
}
