import { api } from "../api";
import type { StatusReport } from "../types";
import { Button } from "../ui/Button";
import { Card, cardBody } from "../ui/Card";
import { createDialog, dialogField, openDialog } from "../ui/Dialog";
import { Input } from "../ui/Input";
import { Pill } from "../ui/Pill";
import { flagBR, flagUS } from "../ui/Flags";
import { idiomaAtual, setLang, t } from "../i18n";
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
  title.textContent = t("Configurações");
  const hint = document.createElement("p");
  hint.className = "pageHint";
  hint.textContent = t(
    "Grava direto em .c2/config.json. Mudanças de contexto/camadas valem no próximo carregamento do modelo.",
  );
  header.append(title, hint);
  page.appendChild(header);

  if (!ctx.root) {
    const vazio = document.createElement("div");
    vazio.className = "empty";
    vazio.textContent = t("abra um projeto para configurar o workspace");
    vazio.appendChild(
      Button({ label: t("Abrir projeto"), variant: "primary", onClick: ctx.onPickProject }),
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
      const toggle = Toggle({ label: t(spec.label), checked: inicial === "true" });
      controles.set(spec.key, {
        kind: spec.kind,
        get: () => (toggle.control.getAttribute("aria-checked") === "true" ? "true" : "false"),
      });
      container.appendChild(toggle.root);
      return;
    }

    if (spec.kind === "select") {
      const select = Select({
        label: t(spec.label),
        value: inicial,
        options: spec.options ?? [],
      });
      controles.set(spec.key, { kind: spec.kind, get: () => select.select.value });
      container.appendChild(select.root);
      return;
    }

    const input = Input({
      label: t(spec.label),
      value: inicial,
      size: "sm",
      mono: spec.kind !== "text",
      readOnly: spec.readOnly,
      hint: spec.hint ? t(spec.hint) : undefined,
    });
    controles.set(spec.key, { kind: spec.kind, get: () => input.input.value });
    container.appendChild(input.root);

    if (spec.key === "model.path") {
      const linha = document.createElement("div");
      linha.className = styles.actions;
      linha.appendChild(
        Button({
          label: t("Selecionar .gguf…"),
          size: "xs",
          variant: "secondary",
          onClick: async () => {
            const escolhido = await api.pickModel();
            if (!escolhido) return;
            try {
              await api.modelInspect(escolhido);
              await api.modelSet(ctx.root, escolhido);
              input.input.value = escolhido;
              status.textContent = t("modelo atualizado");
            } catch (error) {
              await openDialog({
                title: t("Modelo inválido"),
                body: dialogField("Erro", String(error), true),
                confirmLabel: t("Entendi"),
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
    const card = Card({ title: t(titulo), accent });
    const body = cardBody(card);
    const grid = document.createElement("div");
    grid.style.display = "grid";
    grid.style.gridTemplateColumns = "repeat(2, minmax(0, 1fr))";
    grid.style.gap = "var(--space-m)";
    for (const spec of fields) montar(spec, grid);
    body.appendChild(grid);
    page.appendChild(card);
    return card;
  };

  // ---- Idioma da interface (PT-BR x EN) ----
  const idiomaCard = Card({ title: t("Idioma"), accent: 2 });
  const idiomaBody = cardBody(idiomaCard);
  const explicaIdioma = document.createElement("p");
  explicaIdioma.className = "pageHint";
  explicaIdioma.style.maxWidth = "none";
  explicaIdioma.textContent = t(
    "Escolha o idioma da interface. A CLI e as ferramentas MCP continuam em inglês.",
  );
  idiomaBody.appendChild(explicaIdioma);

  const gradeIdioma = document.createElement("div");
  gradeIdioma.style.display = "flex";
  gradeIdioma.style.gap = "var(--space-s)";
  gradeIdioma.style.marginTop = "var(--space-m)";

  const opcaoIdioma = (lang: "en" | "pt-BR", rotulo: string, bandeira: string) => {
    const ativo = idiomaAtual() === lang;
    const botao = document.createElement("button");
    botao.type = "button";
    botao.style.cssText =
      "display: flex; align-items: center; gap: var(--space-s); padding: var(--space-s) var(--space-m); " +
      "border-radius: var(--radius); cursor: pointer; background: var(--bg-surface); " +
      `border: 1px solid ${ativo ? "var(--accent-1)" : "var(--border-muted)"}; color: var(--text);`;
    botao.innerHTML = bandeira;
    const textoBotao = document.createElement("span");
    textoBotao.textContent = rotulo;
    botao.appendChild(textoBotao);
    if (ativo) botao.appendChild(Pill(t("ativo"), "success", "xs"));
    botao.addEventListener("click", () => {
      if (ativo) return;
      setLang(lang);
      status.textContent = t("idioma atualizado");
      ctx.onSaved();
    });
    return botao;
  };

  gradeIdioma.append(
    opcaoIdioma("en", "English", flagUS(22)),
    opcaoIdioma("pt-BR", "Português (Brasil)", flagBR(22)),
  );
  idiomaBody.appendChild(gradeIdioma);
  page.appendChild(idiomaCard);

  // ---- Modo de uso (IA local x API Key) + simples/avancado ----
  const avancado = localStorage.getItem("c2.config.avancado") === "1";
  const provedorAtual = String(lerConfig(config, "model.provider") ?? "local");

  const modoCard = Card({ title: t("Modo de uso"), accent: 1 });
  const modoBody = cardBody(modoCard);
  const explicaModo = document.createElement("p");
  explicaModo.className = "pageHint";
  explicaModo.style.maxWidth = "none";
  explicaModo.textContent = t(
    "Escolha onde a IA roda. O restante da tela se adapta ao modo; " +
      "o que não se aplica fica oculto (abra 'avançado' para ver tudo).",
  );
  modoBody.appendChild(explicaModo);

  const gradeModo = document.createElement("div");
  gradeModo.style.display = "grid";
  gradeModo.style.gridTemplateColumns = "repeat(2, minmax(0, 1fr))";
  gradeModo.style.gap = "var(--space-m)";
  gradeModo.style.marginTop = "var(--space-m)";

  const opcaoModo = (
    titulo: string,
    descricao: string,
    valor: "local" | "openai_compat",
  ): HTMLButtonElement => {
    const ativo = provedorAtual === valor;
    const box = document.createElement("button");
    box.type = "button";
    box.style.cssText =
      "display: grid; gap: var(--space-2xs); text-align: left; padding: var(--space-m); " +
      "border-radius: var(--radius); cursor: pointer; background: var(--bg-surface); " +
      `border: 1px solid ${ativo ? "var(--accent-1)" : "var(--border-muted)"};`;
    const tituloEl = document.createElement("strong");
    tituloEl.textContent = t(titulo);
    tituloEl.style.color = "var(--text-bright)";
    const descEl = document.createElement("span");
    descEl.textContent = t(descricao);
    descEl.style.fontSize = "var(--text-s)";
    descEl.style.color = "var(--text-muted)";
    descEl.style.lineHeight = "1.4";
    box.append(tituloEl, descEl);
    box.appendChild(Pill(t(ativo ? "ativo" : "escolher"), ativo ? "success" : "neutral", "xs"));
    box.addEventListener("click", async () => {
      if (ativo) return;
      try {
        await api.configSet(ctx.root, "model.provider", valor);
        localStorage.setItem("c2.mode", valor);
        status.textContent = valor === "local" ? "modo local ativado" : "modo API/nuvem ativado";
        ctx.onSaved();
      } catch (erro) {
        status.textContent = `nao foi possivel trocar o modo: ${erro}`;
      }
    });
    return box;
  };

  gradeModo.append(
    opcaoModo(
      "IA local (padrão)",
      "Modelo GGUF na sua GPU. Privado, sem custo por token. Precisa de GPU de 8 GB.",
      "local",
    ),
    opcaoModo(
      "API Key / nuvem",
      "OpenAI, Groq, DeepSeek, OpenRouter ou um servidor local (LM Studio/Ollama/vLLM).",
      "openai_compat",
    ),
  );
  modoBody.appendChild(gradeModo);

  const linhaAvancado = document.createElement("div");
  linhaAvancado.className = styles.actions;
  linhaAvancado.style.marginTop = "var(--space-m)";
  linhaAvancado.appendChild(
    Toggle({
      label: t("avançado"),
      checked: avancado,
      onChange: (marcado: boolean) => {
        localStorage.setItem("c2.config.avancado", marcado ? "1" : "0");
        ctx.onSaved();
      },
    }).root,
  );
  modoBody.appendChild(linhaAvancado);
  page.appendChild(modoCard);

  const workspaceCard = montarCard("Workspace", 2, WORKSPACE_FIELDS);
  const runtimeCard = montarCard("Runtime", 4, RUNTIME_FIELDS);

  // ---- Provedor de IA (local x nuvem) ----
  const PROVEDORES: Record<
    string,
    { base: string; modelo: string; precoIn: number; precoOut: number }
  > = {
    "OpenAI": { base: "https://api.openai.com/v1", modelo: "gpt-4o-mini", precoIn: 0.15, precoOut: 0.6 },
    "OpenRouter": { base: "https://openrouter.ai/api/v1", modelo: "openai/gpt-4o-mini", precoIn: 0.15, precoOut: 0.6 },
    "Groq": { base: "https://api.groq.com/openai/v1", modelo: "llama-3.3-70b-versatile", precoIn: 0.59, precoOut: 0.79 },
    "DeepSeek": { base: "https://api.deepseek.com/v1", modelo: "deepseek-chat", precoIn: 0.27, precoOut: 1.1 },
    "Gemini (compat)": {
      base: "https://generativelanguage.googleapis.com/v1beta/openai",
      modelo: "gemini-2.0-flash",
      precoIn: 0.1,
      precoOut: 0.4,
    },
    "Ollama (local)": { base: "http://127.0.0.1:11434/v1", modelo: "llama3.2", precoIn: 0, precoOut: 0 },
    "LM Studio (local)": { base: "http://127.0.0.1:1234/v1", modelo: "local-model", precoIn: 0, precoOut: 0 },
  };

  const provedorCard = Card({ title: t("Provedor de IA"), accent: 3 });
  const provedorBody = cardBody(provedorCard);

  const avisoProvedor = document.createElement("p");
  avisoProvedor.className = "pageHint";
  avisoProvedor.style.maxWidth = "none";
  avisoProvedor.textContent = t(
    "Local é o padrão: o modelo roda na sua máquina e nada sai dela. " +
      "Ao usar um provedor de nuvem, o conteúdo dos arquivos é enviado ao provedor " +
      "configurado e os tokens são cobrados na sua conta — só ative se for aceitável. " +
      "No modo API/BYO, os indicadores locais (runtime, VRAM, .gguf) não se aplicam: " +
      "quem gerencia o servidor é o provedor ou software externo.",
  );
  provedorBody.appendChild(avisoProvedor);

  const gradeProvedor = document.createElement("div");
  gradeProvedor.style.display = "grid";
  gradeProvedor.style.gridTemplateColumns = "repeat(2, minmax(0, 1fr))";
  gradeProvedor.style.gap = "var(--space-m)";
  gradeProvedor.style.marginTop = "var(--space-m)";

  const seletorProvedor = Select({
    label: t("provedor"),
    value: String(lerConfig(config, "model.provider") ?? "local"),
    options: ["local", "openai_compat"],
  });
  controles.set("model.provider", { kind: "select", get: () => seletorProvedor.select.value });
  gradeProvedor.appendChild(seletorProvedor.root);

  const preset = Select({
    label: t("preset"),
    value: "personalizado",
    options: ["personalizado", ...Object.keys(PROVEDORES)],
  });
  gradeProvedor.appendChild(preset.root);

  const campoBase = Input({
    label: t("base URL (nuvem)"),
    value: String(lerConfig(config, "model.base_url") ?? ""),
    mono: true,
    hint: t("ex.: https://api.openai.com/v1"),
  });
  controles.set("model.base_url", { kind: "text", get: () => campoBase.input.value });
  gradeProvedor.appendChild(campoBase.root);

  const campoModelo = Input({
    label: t("modelo"),
    value: String(lerConfig(config, "model.name") ?? ""),
    mono: true,
    hint: t("nome enviado no campo model"),
  });
  controles.set("model.name", { kind: "text", get: () => campoModelo.input.value });
  gradeProvedor.appendChild(campoModelo.root);

  const campoPrecoIn = Input({
    label: t("preço entrada / 1M tokens"),
    value: String(lerConfig(config, "model.cost_input_per_mtok") ?? 0),
    hint: t("modo nuvem; 0 = não calcular"),
  });
  controles.set("model.cost_input_per_mtok", { kind: "text", get: () => campoPrecoIn.input.value });
  gradeProvedor.appendChild(campoPrecoIn.root);

  const campoPrecoOut = Input({
    label: t("preço saída / 1M tokens"),
    value: String(lerConfig(config, "model.cost_output_per_mtok") ?? 0),
    hint: t("modo nuvem; 0 = não calcular"),
  });
  controles.set("model.cost_output_per_mtok", { kind: "text", get: () => campoPrecoOut.input.value });
  gradeProvedor.appendChild(campoPrecoOut.root);

  const campoCap = Input({
    label: t("teto por execução (US$)"),
    value: String(lerConfig(config, "model.max_cost_usd_per_run") ?? 0),
    hint: t("0 = sem teto; interrompe o ciclo ao atingir"),
  });
  controles.set("model.max_cost_usd_per_run", { kind: "text", get: () => campoCap.input.value });
  gradeProvedor.appendChild(campoCap.root);

  preset.select.addEventListener("change", () => {
    const escolhido = PROVEDORES[preset.select.value];
    if (!escolhido) return;
    campoBase.input.value = escolhido.base;
    campoModelo.input.value = escolhido.modelo;
    campoPrecoIn.input.value = String(escolhido.precoIn);
    campoPrecoOut.input.value = String(escolhido.precoOut);
    seletorProvedor.select.value = "openai_compat";
    status.textContent = `preset ${preset.select.value} aplicado (salve para gravar)`;
  });

  provedorBody.appendChild(gradeProvedor);

  const linha = document.createElement("div");
  linha.className = styles.actions;
  linha.style.marginTop = "var(--space-m)";

  const campoChave = Input({
    label: t("API key (cofre do sistema)"),
    placeholder: t("cole a chave do provedor"),
    mono: true,
    hint: t("nunca é gravada no .c2/config.json"),
  });
  campoChave.input.type = "password";
  provedorBody.appendChild(campoChave.root);

  let temChave = false;
  try {
    temChave = await api.apiKeyStatus();
  } catch {
    /* modo preview */
  }
  const pillChave = Pill(temChave ? "chave salva" : "sem chave", temChave ? "success" : "neutral", "xs");

  linha.appendChild(
    Button({
      label: t("Salvar chave"),
      size: "xs",
      variant: "secondary",
      onClick: async () => {
        try {
          await api.apiKeySet(campoChave.input.value);
          campoChave.input.value = "";
          status.textContent = t("API key salva no cofre do sistema");
        } catch (erro) {
          status.textContent = `nao foi possivel salvar a chave: ${erro}`;
        }
      },
    }),
  );
  linha.appendChild(
    Button({
      label: t("Esquecer chave"),
      size: "xs",
      variant: "ghost",
      onClick: async () => {
        await api.apiKeyDelete();
        status.textContent =
          "chave esquecida (removida do cofre local; revogue no provedor se necessario)";
      },
    }),
  );
  linha.appendChild(
    Button({
      label: t("Testar conexão"),
      size: "xs",
      variant: "primary",
      onClick: async () => {
        status.textContent = t("testando provedor…");
        const resultado = await api.providerTest(ctx.root);
        status.textContent = resultado.ok
          ? `provedor ok (${resultado.provider} · ${resultado.model}) em ${resultado.latency_ms} ms`
          : `falha no provedor: ${resultado.error ?? "erro desconhecido"}`;
      },
    }),
  );
  linha.appendChild(pillChave);
  provedorBody.appendChild(linha);

  page.appendChild(provedorCard);

  // ---- Prompt de indexacao (personalizavel) ----
  const promptInicial = String(lerConfig(config, "index.prompt") ?? "");
  iniciais.set("index.prompt", promptInicial);

  const promptCard = Card({ title: t("Prompt de indexação"), accent: 6 });
  const promptBody = cardBody(promptCard);

  const fluxo = document.createElement("p");
  fluxo.className = styles.promptFluxo;
  fluxo.textContent = t("prompt = instruções + conteúdo do arquivo 1 → resposta");
  promptBody.appendChild(fluxo);

  const explicacao = document.createElement("p");
  explicacao.className = "pageHint";
  explicacao.style.maxWidth = "none";
  explicacao.textContent = t(
    "Estas instruções são enviadas ao modelo junto com o conteúdo de cada arquivo. " +
      "A linha que trata o arquivo como DADO (e não instrução) é fixa e não pode ser removida.",
  );
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
    padrao = await api.promptDefault(idiomaAtual());
  } catch {
    padrao = "";
  }
  if (padrao) {
    promptAcoes.appendChild(
      Button({
        label: t("Restaurar padrão"),
        size: "xs",
        variant: "secondary",
        onClick: () => {
          area.value = padrao;
          status.textContent = t("prompt restaurado (salve para aplicar)");
        },
      }),
    );
  }
  // Troca explicita do idioma dos metadados (independente da UI).
  const trocaIdiomaPrompt = document.createElement("div");
  trocaIdiomaPrompt.style.display = "flex";
  trocaIdiomaPrompt.style.gap = "var(--space-s)";
  trocaIdiomaPrompt.style.marginTop = "var(--space-s)";
  for (const [rotulo, lang] of [
    ["English", "en"],
    ["Português", "pt-BR"],
  ] as Array<[string, string]>) {
    trocaIdiomaPrompt.appendChild(
      Button({
        label: rotulo,
        size: "xs",
        variant: "ghost",
        onClick: () => {
          void (async () => {
            try {
              area.value = await api.promptDefault(lang);
              status.textContent = t("prompt trocado (salve para aplicar)");
            } catch {
              status.textContent = t("falha ao carregar prompt");
            }
          })();
        },
      }),
    );
  }
  promptBody.appendChild(trocaIdiomaPrompt);

  const notaPrompt = document.createElement("p");
  notaPrompt.className = "pageHint";
  notaPrompt.style.maxWidth = "none";
  notaPrompt.textContent = t(
    "O idioma dos metadados é definido por este prompt, não pelo idioma da interface. " +
      "Use English/Português acima ou edite livremente.",
  );
  promptBody.appendChild(notaPrompt);

  const avisoTraducao = document.createElement("p");
  avisoTraducao.className = "pageHint";
  avisoTraducao.style.maxWidth = "none";
  avisoTraducao.style.color = "var(--warning)";
  avisoTraducao.textContent = t(
    "Trocar o idioma do prompt não traduz metadados antigos. " +
      "Para regenerar tudo no novo idioma, rode Reindexar tudo na Engine.",
  );
  promptBody.appendChild(avisoTraducao);

  controles.set("index.prompt", { kind: "text", get: () => area.value });
  page.appendChild(promptCard);

  // ---- Modelo local no modo simples (o card Runtime completo fica em avançado) ----
  const modeloSimples = Card({ title: t("Modelo local"), accent: 4 });
  const modeloSimplesBody = cardBody(modeloSimples);
  const modeloNome = document.createElement("div");
  modeloNome.className = styles.fieldValue;
  modeloNome.textContent = String(lerConfig(config, "model.name") ?? "nenhum modelo");
  modeloSimplesBody.appendChild(modeloNome);
  const modeloCaminho = document.createElement("div");
  modeloCaminho.className = styles.field;
  const modeloCaminhoLabel = document.createElement("span");
  modeloCaminhoLabel.className = styles.fieldLabel;
  modeloCaminhoLabel.textContent = t("GGUF");
  const modeloCaminhoValor = document.createElement("span");
  modeloCaminhoValor.className = styles.mono;
  const caminhoInicial = String(lerConfig(config, "model.path") ?? "");
  modeloCaminhoValor.textContent = caminhoInicial || "não configurado";
  modeloCaminhoValor.title = caminhoInicial;
  modeloCaminho.append(modeloCaminhoLabel, modeloCaminhoValor);
  modeloSimplesBody.appendChild(modeloCaminho);
  const modeloAcoes = document.createElement("div");
  modeloAcoes.className = styles.actions;
  modeloAcoes.appendChild(
    Button({
      label: t("Selecionar .gguf…"),
      iconName: "projects",
      size: "sm",
      variant: "secondary",
      onClick: async () => {
        const escolhido = await api.pickModel();
        if (!escolhido) return;
        try {
          await api.modelSet(ctx.root, escolhido);
          status.textContent = t("modelo atualizado");
          ctx.onSaved();
        } catch (erro) {
          await openDialog({
            title: t("Modelo inválido"),
            body: dialogField("Erro", String(erro), true),
            confirmLabel: t("Entendi"),
            cancelLabel: "",
          });
        }
      },
    }),
  );
  const dicaModelo = document.createElement("p");
  dicaModelo.className = "pageHint";
  dicaModelo.style.maxWidth = "none";
  dicaModelo.textContent = t(
    "O modelo é uma preferência do app: escolha uma vez e ele vale para todos os projetos " +
      "(novos projetos herdam este caminho).",
  );
  modeloSimplesBody.appendChild(dicaModelo);
  modeloSimplesBody.appendChild(modeloAcoes);
  page.appendChild(modeloSimples);

  // ---- Manutencao: limpar metadados (acao destrutiva, dupla confirmacao) ----
  const manutencaoCard = Card({ title: t("Manutenção"), accent: 10 });
  const manutencaoBody = cardBody(manutencaoCard);
  const textoManutencao = document.createElement("p");
  textoManutencao.className = "pageHint";
  textoManutencao.style.maxWidth = "none";
  textoManutencao.textContent = t(
    "Remove todos os metadados .c2.json do projeto. O código e o .c2/config.json não são alterados; " +
      "para recriar o contexto, rode Reindexar tudo depois. Anotações de agentes serão perdidas.",
  );
  manutencaoBody.appendChild(textoManutencao);

  const limpar = Button({
    label: t("Limpar metadados…"),
    variant: "destructive",
    size: "xs",
    disabled: !ctx.root,
    onClick: () => {
      void (async () => {
        if (!ctx.root) return;
        try {
          const plano = await api.cleanerPlan(ctx.root);
          if (plano.found === 0) {
            status.textContent = t("nenhum metadado para limpar");
            return;
          }
          const handle = createDialog({ title: t("Limpar metadados?") });
          const corpo = document.createElement("div");
          corpo.style.display = "grid";
          corpo.style.gap = "var(--space-s)";

          const contagem = document.createElement("p");
          contagem.style.margin = "0";
          contagem.style.color = "var(--warning)";
          contagem.textContent = `${plano.found} ${t("metadados serão removidos")}`;
          corpo.appendChild(contagem);

          const aviso = document.createElement("p");
          aviso.className = "pageHint";
          aviso.style.margin = "0";
          aviso.style.maxWidth = "none";
          aviso.textContent = t(
            "O código e o .c2/config.json não são alterados. Esta ação não pode ser desfeita; reindexe depois.",
          );
          corpo.appendChild(aviso);

          const confirmacao = Input({
            label: t("Digite DELETE (maiúsculo) para confirmar"),
            placeholder: "DELETE",
            mono: true,
          });
          corpo.appendChild(confirmacao.root);

          const remover = Button({
            label: t("Remover metadados"),
            variant: "destructive",
            disabled: true,
            onClick: async () => {
              if (confirmacao.input.value !== "DELETE") return;
              handle.close();
              status.textContent = t("removendo metadados…");
              try {
                const resultado = await api.cleanerRun(ctx.root);
                status.textContent = `${resultado.removed} ${t("metadados removidos")}`;
                if (resultado.failed.length > 0) {
                  status.textContent += ` · ${resultado.failed.length} ${t("falhas")}`;
                }
                ctx.onSaved();
              } catch (erro) {
                status.textContent = `${t("falha ao limpar")}: ${String(erro)}`;
              }
            },
          });
          confirmacao.input.addEventListener("input", () => {
            remover.disabled = confirmacao.input.value !== "DELETE";
          });
          confirmacao.input.addEventListener("keydown", (evento) => {
            if (evento.key === "Enter" && confirmacao.input.value === "DELETE") {
              remover.click();
            }
          });

          handle.body.appendChild(corpo);
          handle.footer.append(
            Button({
              label: t("Cancelar"),
              variant: "ghost",
              onClick: () => handle.close(),
            }),
            remover,
          );
        } catch (erro) {
          status.textContent = `${t("falha ao limpar")}: ${String(erro)}`;
        }
      })();
    },
  });
  manutencaoBody.appendChild(limpar);
  page.appendChild(manutencaoCard);

  // ---- Visibilidade condicional: modo x simples/avancado ----
  const aplicarVisibilidade = () => {
    const cloud = seletorProvedor.select.value === "openai_compat";
    const adv = localStorage.getItem("c2.config.avancado") === "1";
    workspaceCard.style.display = adv ? "" : "none";
    runtimeCard.style.display = adv ? "" : "none";
    promptCard.style.display = adv ? "" : "none";
    manutencaoCard.style.display = adv ? "" : "none";
    provedorCard.style.display = cloud || adv ? "" : "none";
    modeloSimples.style.display = !cloud && !adv ? "" : "none";
    seletorProvedor.root.style.display = adv ? "" : "none";
    campoBase.root.style.display = adv ? "" : "none";
    campoModelo.root.style.display = adv ? "" : "none";
    preset.root.style.display = cloud ? "" : "none";
    campoPrecoIn.root.style.display = cloud && adv ? "" : "none";
    campoPrecoOut.root.style.display = cloud && adv ? "" : "none";
    campoCap.root.style.display = cloud && adv ? "" : "none";
    campoChave.root.style.display = cloud ? "" : "none";
    linha.style.display = cloud ? "" : "none";
  };
  aplicarVisibilidade();

  const rodape = document.createElement("div");
  rodape.className = styles.actions;
  const salvar = Button({
    label: t("Salvar alterações"),
    variant: "primary",
    iconName: "settings",
    onClick: async () => {
      salvar.disabled = true;
      salvar.textContent = t("Salvando…");
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
      salvar.textContent = t("Salvar alterações");

      if (erros.length > 0) {
        status.textContent = "";
        status.style.color = "var(--danger)";
        status.textContent = erros.join(" · ");
        return;
      }
      if (alterados === 0) {
        status.textContent = t("nada para salvar");
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
      label: t("Recarregar"),
      variant: "ghost",
      onClick: ctx.onSaved,
    }),
    status,
    Pill(
      ctx.status?.model.provider === "openai_compat"
        ? t("runtime externo")
        : ctx.status?.runtime.healthy
          ? t("runtime no ar")
          : t("runtime parado"),
      ctx.status?.runtime.healthy && ctx.status?.model.provider !== "openai_compat"
        ? "success"
        : "neutral",
      "xs",
    ),
  );
  page.appendChild(rodape);

  return { element: page };
}
