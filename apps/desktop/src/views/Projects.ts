import { api, hasTauri } from "../api";
import { Button } from "../ui/Button";
import { Card, cardBody, cardFooter } from "../ui/Card";
import { Pill } from "../ui/Pill";
import type { StatusReport } from "../types";
import styles from "./views.module.css";

export interface ProjectsContext {
  root: string;
  status: StatusReport | null;
  error: string | null;
  onPickProject: () => void;
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

  grid.appendChild(
    projectCard("Modelo", status.model.name, [
      ["GGUF", status.model.path ?? "não configurado"],
      ["Endpoint", `${status.model.host}:${status.model.port}`],
      ["Raciocínio", status.model.disable_thinking ? "desligado" : "ligado"],
    ]),
  );

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
      label: "Selecionar modelo",
      variant: "secondary",
      onClick: async () => {
        const path = await api.pickModel();
        if (!path) return;
        await api.configSet(ctx.root, "model.path", path);
        location.reload();
      },
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
