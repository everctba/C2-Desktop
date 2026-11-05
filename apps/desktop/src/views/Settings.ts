import { api } from "../api";
import type { StatusReport } from "../types";
import { Button } from "../ui/Button";
import { Card, cardBody } from "../ui/Card";
import { dialogField, openDialog } from "../ui/Dialog";
import { Input } from "../ui/Input";
import { Pill } from "../ui/Pill";
import { Select } from "../ui/Select";
import { Toggle } from "../ui/Toggle";
import type { View } from "./view";
import styles from "./views.module.css";

export interface SettingsContext {
  root: string;
  status: StatusReport | null;
  onSaved: () => void;
  onPickProject: () => void;
}

type Kind = "text" | "number" | "toggle" | "select" | "list" | "kib" | "mib";

interface FieldSpec {
  key: string;
  label: string;
  kind: Kind;
  options?: string[];
  hint?: string;
  readOnly?: boolean;
}

const WORKSPACE_FIELDS: FieldSpec[] = [
  {
    key: "index.extensions",
    label: "extensões indexadas",
    kind: "list",
    hint: "separadas por vírgula (rs, ts, py, md…)",
  },
  {
    key: "index.excluded_dirs",
    label: "diretórios excluídos",
    kind: "list",
    hint: "separados por vírgula (node_modules, dist…)",
  },
  { key: "index.max_file_bytes", label: "máx. por arquivo (KiB)", kind: "kib" },
  { key: "index.max_total_bytes_per_run", label: "máx. por execução (MiB)", kind: "mib" },
  { key: "index.max_links_per_file", label: "máx. links por arquivo", kind: "number" },
  { key: "index.respect_gitignore", label: "respeitar .gitignore", kind: "toggle" },
  {
    key: "index.secret_policy",
    label: "política de segredos",
    kind: "select",
    options: ["skip", "mask"],
    hint: "skip pula o arquivo; mask envia mascarado",
  },
];

const RUNTIME_FIELDS: FieldSpec[] = [
  {
    key: "model.path",
    label: "modelo .gguf",
    kind: "text",
    readOnly: true,
    hint: "o workspace guarda só o caminho",
  },
  { key: "model.host", label: "host", kind: "text" },
  { key: "model.port", label: "porta", kind: "number" },
  { key: "model.ctx_size", label: "contexto (tokens)", kind: "number" },
  { key: "model.gpu_layers", label: "camadas na GPU", kind: "number" },
  { key: "model.threads", label: "threads (CPU)", kind: "number" },
  { key: "model.temperature", label: "temperatura", kind: "number" },
  { key: "model.max_tokens", label: "max_tokens", kind: "number" },
  { key: "model.timeout_secs", label: "timeout (s)", kind: "number" },
  { key: "model.max_retries", label: "tentativas", kind: "number" },
  { key: "model.disable_thinking", label: "desligar raciocínio", kind: "toggle" },
  { key: "model.allow_lmstudio", label: "LM Studio como fallback", kind: "toggle" },
  { key: "engine.poll_interval_secs", label: "polling do engine (s)", kind: "number" },
  { key: "engine.batch_size", label: "arquivos por ciclo", kind: "number" },
];

interface Controle {
  kind: Kind;
  get: () => string;
  set?: (valor: string) => void;
}

function lerConfig(config: Record<string, unknown>, key: string): unknown {
  let atual: unknown = config;
  for (const parte of key.split(".")) {
    if (atual === null || typeof atual !== "object") return undefined;
    atual = (atual as Record<string, unknown>)[parte];
  }
  return atual;
}

function valorInicial(config: Record<string, unknown>, spec: FieldSpec): string {
  const valor = lerConfig(config, spec.key);
  if (valor === undefined || valor === null) return "";
  if (Array.isArray(valor)) return valor.join(", ");
  if (spec.kind === "kib") return String(Math.round(Number(valor) / 1024));
  if (spec.kind === "mib") return String(Math.round(Number(valor) / 1024 / 1024));
  return String(valor);
}

function valorParaSalvar(spec: FieldSpec, bruto: string): string {
  if (spec.kind === "kib") return String(Math.round(Number(bruto) * 1024));
  if (spec.kind === "mib") return String(Math.round(Number(bruto) * 1024 * 1024));
  return bruto;
}

/** Configurações do workspace e do runtime (c2-e16t2 / c2-e16t3). */
export async function renderSettings(ctx: SettingsContext): Promise<View> {
  const page = document.createElement("div");
  page.className = styles.page;

  const header = document.createElement("div");
  header.className = styles.header;
  const title = document.createElement("h1");
  title.className = "pageTitle";
  title.textContent = "Configurações";
  const hint = document.createElement("p");
  hint.className = "pageHint";
  hint.textContent =
    "Grava direto em .c2/config.json. Mudanças de contexto/camadas valem no próximo carregamento do modelo.";
  header.append(title, hint);
  page.appendChild(header);

  if (!ctx.root) {
    const vazio = document.createElement("div");
    vazio.className = "empty";
    vazio.textContent = "abra um projeto para configurar o workspace";
    vazio.appendChild(
      Button({ label: "Abrir projeto", variant: "primary", onClick: ctx.onPickProject }),
    );
    page.appendChild(vazio);
    return { element: page };
  }

  let config: Record<string, unknown>;
  try {
    config = await api.configGet(ctx.root);
  } catch (error) {
    const erro = document.createElement("div");
    erro.className = "empty";
    erro.textContent = String(error);
    page.appendChild(erro);
    return { element: page };
  }

  const controles = new Map<string, Controle>();
  const iniciais = new Map<string, string>();
  const status = document.createElement("div");
  status.className = styles.resultMeta;
  status.style.minHeight = "16px";

  const montar = (spec: FieldSpec, container: HTMLElement) => {
    const inicial = valorInicial(config, spec);
    iniciais.set(spec.key, inicial);

    if (spec.kind === "toggle") {
      const toggle = Toggle({ label: spec.label, checked: inicial === "true" });
      controles.set(spec.key, {
        kind: spec.kind,
        get: () => (toggle.control.getAttribute("aria-checked") === "true" ? "true" : "false"),
      });
      container.appendChild(toggle.root);
      return;
    }

    if (spec.kind === "select") {
      const select = Select({
        label: spec.label,
        value: inicial,
        options: spec.options ?? [],
      });
      controles.set(spec.key, { kind: spec.kind, get: () => select.select.value });
      container.appendChild(select.root);
      return;
    }

    const input = Input({
      label: spec.label,
      value: inicial,
      size: "sm",
      mono: spec.kind !== "text",
      readOnly: spec.readOnly,
      hint: spec.hint,
    });
    controles.set(spec.key, { kind: spec.kind, get: () => input.input.value });
    container.appendChild(input.root);

    if (spec.key === "model.path") {
      const linha = document.createElement("div");
      linha.className = styles.actions;
      linha.appendChild(
        Button({
          label: "Selecionar .gguf…",
          size: "xs",
          variant: "secondary",
          onClick: async () => {
            const escolhido = await api.pickModel();
            if (!escolhido) return;
            try {
              await api.modelInspect(escolhido);
              await api.modelSet(ctx.root, escolhido);
              input.input.value = escolhido;
              status.textContent = "modelo atualizado";
            } catch (error) {
              await openDialog({
                title: "Modelo inválido",
                body: dialogField("Erro", String(error), true),
                confirmLabel: "Entendi",
                cancelLabel: "",
              });
            }
          },
        }),
      );
      container.appendChild(linha);
    }
  };

  const montarCard = (titulo: string, accent: number, fields: FieldSpec[]) => {
    const card = Card({ title: titulo, accent });
    const body = cardBody(card);
    const grid = document.createElement("div");
    grid.style.display = "grid";
    grid.style.gridTemplateColumns = "repeat(2, minmax(0, 1fr))";
    grid.style.gap = "var(--space-m)";
    for (const spec of fields) montar(spec, grid);
    body.appendChild(grid);
    page.appendChild(card);
  };

  montarCard("Workspace", 2, WORKSPACE_FIELDS);
  montarCard("Runtime", 4, RUNTIME_FIELDS);

  // ---- Prompt de indexacao (personalizavel) ----
  const promptInicial = String(lerConfig(config, "index.prompt") ?? "");
  iniciais.set("index.prompt", promptInicial);

  const promptCard = Card({ title: "Prompt de indexação", accent: 6 });
  const promptBody = cardBody(promptCard);

  const fluxo = document.createElement("p");
  fluxo.className = styles.promptFluxo;
  fluxo.textContent = "prompt = instruções + conteúdo do arquivo 1 → resposta";
  promptBody.appendChild(fluxo);

  const explicacao = document.createElement("p");
  explicacao.className = "pageHint";
  explicacao.style.maxWidth = "none";
  explicacao.textContent =
    "Estas instruções são enviadas ao modelo junto com o conteúdo de cada arquivo. " +
    "A linha que trata o arquivo como DADO (e não instrução) é fixa e não pode ser removida.";
  promptBody.appendChild(explicacao);

  const area = document.createElement("textarea");
  area.className = styles.promptArea;
  area.rows = 12;
  area.spellcheck = false;
  area.value = promptInicial;
  promptBody.appendChild(area);

  const promptAcoes = document.createElement("div");
  promptAcoes.className = styles.actions;
  let padrao = "";
  try {
    padrao = await api.promptDefault();
  } catch {
    padrao = "";
  }
  if (padrao) {
    promptAcoes.appendChild(
      Button({
        label: "Restaurar padrão",
        size: "xs",
        variant: "secondary",
        onClick: () => {
          area.value = padrao;
          status.textContent = "prompt restaurado (salve para aplicar)";
        },
      }),
    );
  }
  promptBody.appendChild(promptAcoes);

  controles.set("index.prompt", { kind: "text", get: () => area.value });
  page.appendChild(promptCard);

  const rodape = document.createElement("div");
  rodape.className = styles.actions;
  const salvar = Button({
    label: "Salvar alterações",
    variant: "primary",
    iconName: "settings",
    onClick: async () => {
      salvar.disabled = true;
      salvar.textContent = "Salvando…";
      const erros: string[] = [];
      let alterados = 0;

      for (const [chave, controle] of controles) {
        const atual = controle.get().trim();
        const inicial = iniciais.get(chave) ?? "";
        if (atual === inicial) continue;

        if (controle.kind === "number" || controle.kind === "kib" || controle.kind === "mib") {
          if (atual === "" || Number.isNaN(Number(atual))) {
            erros.push(`${chave}: valor numérico inválido`);
            continue;
          }
        }

        try {
          await api.configSet(ctx.root, chave, valorParaSalvar({ key: chave, label: chave, kind: controle.kind }, atual));
          alterados += 1;
        } catch (error) {
          erros.push(`${chave}: ${String(error)}`);
        }
      }

      salvar.disabled = false;
      salvar.textContent = "Salvar alterações";

      if (erros.length > 0) {
        status.textContent = "";
        status.style.color = "var(--danger)";
        status.textContent = erros.join(" · ");
        return;
      }
      if (alterados === 0) {
        status.textContent = "nada para salvar";
        return;
      }
      status.style.color = "var(--success)";
      status.textContent = `${alterados} campo(s) salvo(s)`;
      ctx.onSaved();
    },
  });
  rodape.append(
    salvar,
    Button({
      label: "Recarregar",
      variant: "ghost",
      onClick: ctx.onSaved,
    }),
    status,
    Pill(
      ctx.status?.runtime.healthy ? "runtime no ar" : "runtime parado",
      ctx.status?.runtime.healthy ? "success" : "neutral",
      "xs",
    ),
  );
  page.appendChild(rodape);

  return { element: page };
}
