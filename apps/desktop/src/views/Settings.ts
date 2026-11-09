import { api, openExternal } from "../api";
import type { EmbeddingStatus, StatusReport } from "../types";
import { Button } from "../ui/Button";
import { Card, cardBody } from "../ui/Card";
import { createDialog, dialogField, openDialog } from "../ui/Dialog";
import { limparMetadados } from "./Cleanup";
import { iniciarProbeUI } from "../perf";
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
    opcaoIdioma("pt-BR", t("Português (Brasil)"), flagBR(22)),
  );
  idiomaBody.appendChild(gradeIdioma);

  // Idioma e Execucao automatica lado a lado no topo.
  const topoConfig = document.createElement("div");
  topoConfig.className = styles.duasColunas;
  topoConfig.appendChild(idiomaCard);
  page.appendChild(topoConfig);

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
  avisoProvedor.textContent =
    t("Local é o padrão: o modelo roda na sua máquina e nada sai dela. ") +
    t("Ao usar um provedor de nuvem, o conteúdo dos arquivos é enviado ao provedor ") +
    t("configurado e os tokens são cobrados na sua conta — só ative se for aceitável. ") +
    t("No modo API/BYO, os indicadores locais (runtime, VRAM, .gguf) não se aplicam: ") +
    t("quem gerencia o servidor é o provedor ou software externo.");
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

  // ---- Prompt de indexacao (Original ou Custom) ----
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

  const padraoPt = await api.promptDefault("pt-BR").catch(() => "");
  const padraoEn = await api.promptDefault("en").catch(() => "");
  const padraoAtual = await api.promptDefault(idiomaAtual()).catch(() => "");
  const ehPadrao = (texto: string) => {
    const atual = texto.trim();
    return [padraoPt, padraoEn].some((padrao) => {
      const p = padrao.trim();
      return p.length > 0 && (atual === p || (atual.length > 300 && p.startsWith(atual)));
    });
  };
  let personalizado = promptInicial.trim().length > 0 && !ehPadrao(promptInicial);

  const area = document.createElement("textarea");
  area.className = styles.promptArea;
  area.rows = 12;
  area.spellcheck = false;
  area.value = personalizado ? promptInicial : padraoAtual || padraoEn || padraoPt;
  area.readOnly = !personalizado;
  area.style.opacity = personalizado ? "1" : "0.78";
  promptBody.appendChild(area);

  const promptAcoes = document.createElement("div");
  promptAcoes.className = styles.actions;

  const btnOriginal = Button({
    label: t("Original"),
    size: "xs",
    variant: "secondary",
    onClick: () => {
      void (async () => {
        try {
          const padrao = await api.promptDefault(idiomaAtual());
          area.value = padrao;
          personalizado = false;
          area.readOnly = true;
          area.style.opacity = "0.78";
          atualizarModoPrompt();
          status.textContent = t("prompt original restaurado (salve para aplicar)");
        } catch {
          status.textContent = t("falha ao carregar prompt");
        }
      })();
    },
  });

  const btnCustom = Button({
    label: t("Personalizar"),
    size: "xs",
    variant: "secondary",
    onClick: () => ativarCustom(),
  });

  promptAcoes.append(btnOriginal, btnCustom);
  promptBody.appendChild(promptAcoes);

  const atualizarModoPrompt = () => {
    btnOriginal.style.borderColor = personalizado ? "" : "var(--accent-1)";
    btnOriginal.style.background = personalizado ? "" : "var(--accent-1-10)";
    btnCustom.style.borderColor = personalizado ? "var(--accent-1)" : "";
    btnCustom.style.background = personalizado ? "var(--accent-1-10)" : "";
    area.readOnly = !personalizado;
    area.style.opacity = personalizado ? "1" : "0.78";
  };

  const ativarCustom = () => {
    personalizado = true;
    area.readOnly = false;
    area.focus();
    atualizarModoPrompt();
    status.textContent = t("prompt personalizado: edite e salve para aplicar");
  };

  // No modo Original, clicar no texto explica como ativar o Custom.
  area.addEventListener("click", () => {
    if (personalizado) return;
    void (async () => {
      const corpo = document.createElement("p");
      corpo.className = "pageHint";
      corpo.style.maxWidth = "none";
      corpo.textContent = t(
        "Para editar, ative a opção Personalizar. No modo Original o prompt acompanha o idioma da interface.",
      );
      const editar = await openDialog({
        title: t("Editar prompt?"),
        body: corpo,
        confirmLabel: t("Personalizar"),
        cancelLabel: t("Cancelar"),
      });
      if (editar) ativarCustom();
    })();
  });
  atualizarModoPrompt();

  const notaPrompt = document.createElement("p");
  notaPrompt.className = "pageHint";
  notaPrompt.style.maxWidth = "none";
  notaPrompt.textContent = t(
    "O prompt Original acompanha o idioma da interface. Personalizar permite editar " +
      "e não muda quando o idioma da interface muda.",
  );
  promptBody.appendChild(notaPrompt);

  const avisoTraducao = document.createElement("p");
  avisoTraducao.className = "pageHint";
  avisoTraducao.style.maxWidth = "none";
  avisoTraducao.style.color = "var(--warning)";
  avisoTraducao.textContent = t(
    "Trocar o idioma não traduz metadados antigos. Para regenerar tudo no novo idioma, " +
      "rode Reindexar tudo na Engine.",
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

  // ---- Execucao automatica (desktop e o modo normal) ----
  const execucaoCard = Card({ title: t("Execução automática"), accent: 2 });
  const execucaoBody = cardBody(execucaoCard);
  const execucaoTexto = document.createElement("p");
  execucaoTexto.className = "pageHint";
  execucaoTexto.style.maxWidth = "none";
  execucaoTexto.textContent = t(
    "Mantenha o C2 Desktop aberto ou na bandeja: ele indexa em segundo plano automaticamente. " +
      "A CLI e o MCP apenas leem; não mantêm o índice atualizado sozinhos.",
  );
  execucaoBody.appendChild(execucaoTexto);
  execucaoBody.appendChild(
    Toggle({
      label: t("Iniciar engine automaticamente ao abrir o projeto"),
      checked: localStorage.getItem("c2.autoEngine") !== "0",
      tone: "brand",
      onChange: (ligado) => {
        if (ligado) localStorage.removeItem("c2.autoEngine");
        else localStorage.setItem("c2.autoEngine", "0");
        status.textContent = ligado
          ? t("engine será iniciado automaticamente ao abrir o projeto")
          : t("engine automático desligado");
      },
    }).root,
  );
  topoConfig.appendChild(execucaoCard);

  // ---- Busca semantica (embeddings bge-m3, opcional) ----
  const embeddingCard = Card({ title: t("Busca semântica (embeddings)"), accent: 3 });
  const embeddingBody = cardBody(embeddingCard);
  const embedStatusInicial = ctx.root
    ? await api.embeddingStatus(ctx.root).catch(() => null)
    : null;
  let embedStatus: EmbeddingStatus | null = embedStatusInicial;
  let embedDevice = embedStatus?.device === "gpu" ? "gpu" : "cpu";

  const embedTexto = document.createElement("p");
  embedTexto.className = "pageHint";
  embedTexto.style.maxWidth = "none";
  embedTexto.textContent = t(
    "Busca por significado usando embeddings locais dos resumos .c2.json. " +
      "O grep lexical continua; embeddings são um sinal extra de recall.",
  );
  embeddingBody.appendChild(embedTexto);

  const embedInfo = document.createElement("div");
  embedInfo.className = styles.field;
  const embedInfoLabel = document.createElement("span");
  embedInfoLabel.className = styles.fieldLabel;
  embedInfoLabel.textContent = t("Modelo");
  const embedInfoValor = document.createElement("span");
  embedInfoValor.className = styles.mono;
  embedInfo.append(embedInfoLabel, embedInfoValor);
  embeddingBody.appendChild(embedInfo);

  const embedAcoes = document.createElement("div");
  embedAcoes.className = styles.actions;
  embedAcoes.style.marginTop = "var(--space-s)";
  const embedToggle = Toggle({
    label: t("Ativar busca semântica"),
    checked: Boolean(embedStatus?.enabled),
    tone: "brand",
    onChange: (ligado) => {
      void (async () => {
        if (ligado && !embedStatus?.model_path) {
          await abrirModalEmbedding();
          embedToggle.control.setAttribute("aria-checked", "false");
          embedToggle.control.className = embedToggle.control.className.replace(/\bon\b/, "");
          return;
        }
        try {
          embedStatus = await api.embeddingSave(
            ctx.root,
            undefined,
            ligado,
            embedDevice,
          );
          if (ligado) {
            embedInfoValor.textContent = t("carregando runtime…");
            embedStatus = await api.embeddingStart(ctx.root);
          } else {
            await api.embeddingStop(ctx.root);
          }
          atualizarEmbedding(embedStatus);
        } catch (erro) {
          embedInfoValor.textContent = `erro: ${String(erro)}`;
        }
      })();
    },
  });
  embedAcoes.appendChild(embedToggle.root);

  const btnCpu = Button({
    label: "CPU",
    size: "xs",
    variant: "secondary",
    onClick: () => void trocarDevice("cpu"),
  });
  const btnGpu = Button({
    label: "GPU",
    size: "xs",
    variant: "secondary",
    onClick: () => void trocarDevice("gpu"),
  });
  embedAcoes.append(btnCpu, btnGpu);
  embeddingBody.appendChild(embedAcoes);

  const embedConfigAcoes = document.createElement("div");
  embedConfigAcoes.className = styles.actions;
  embedConfigAcoes.style.marginTop = "var(--space-s)";
  embedConfigAcoes.append(
    Button({
      label: t("Configurar modelo…"),
      size: "xs",
      variant: "secondary",
      onClick: () => void abrirModalEmbedding(),
    }),
    Button({
      label: t("Abrir pasta de modelos"),
      size: "xs",
      variant: "ghost",
      onClick: () => {
        const pasta = embedStatus?.model_path?.replace(/[\\/][^\\/]+$/, "");
        if (pasta) void api.openProjectFolder(pasta);
      },
    }),
  );
  embeddingBody.appendChild(embedConfigAcoes);

  const atualizarEmbedding = (status: EmbeddingStatus | null) => {
    embedStatus = status;
    embedDevice = status?.device === "gpu" ? "gpu" : "cpu";
    embedInfoValor.textContent = status?.model_path
      ? status.model_path
      : t("nenhum modelo configurado");
    btnCpu.style.borderColor = embedDevice === "cpu" ? "var(--accent-1)" : "";
    btnCpu.style.background = embedDevice === "cpu" ? "var(--accent-1-10)" : "";
    btnGpu.style.borderColor = embedDevice === "gpu" ? "var(--accent-1)" : "";
    btnGpu.style.background = embedDevice === "gpu" ? "var(--accent-1-10)" : "";
  };

  const trocarDevice = async (device: string) => {
    try {
      embedStatus = await api.embeddingSave(
        ctx.root,
        undefined,
        undefined,
        device,
      );
      if (embedStatus.enabled && embedStatus.model_path) {
        await api.embeddingStop(ctx.root);
        embedInfoValor.textContent = t("carregando runtime…");
        embedStatus = await api.embeddingStart(ctx.root);
      }
      atualizarEmbedding(embedStatus);
      status.textContent = t("device de embeddings atualizado");
    } catch (erro) {
      embedInfoValor.textContent = `erro: ${String(erro)}`;
    }
  };

  const abrirModalEmbedding = async () => {
    const handle = createDialog({ title: t("Configurar busca semântica") });
    const corpo = document.createElement("div");
    corpo.style.display = "grid";
    corpo.style.gap = "var(--space-s)";
    const texto = document.createElement("p");
    texto.className = "pageHint";
    texto.style.maxWidth = "none";
    texto.textContent = t(
      "Baixe o bge-m3-Q8_0.gguf (~600 MB) em uma pasta fora do C: " +
        "(ex.: D:\C2\data\models\embeddings) e selecione o arquivo. " +
        "Nada é baixado automaticamente.",
    );
    corpo.appendChild(texto);
    const info = dialogField(t("Arquivo"), "bge-m3-Q8_0.gguf · ~600 MB · bge-m3 Q8_0");
    corpo.appendChild(info);

    let pastaModelos = embedStatus?.model_path?.replace(/[\\/][^\\/]+$/, "") ?? "";
    const pastaInfo = document.createElement("div");
    pastaInfo.className = styles.field;
    const pastaLabel = document.createElement("span");
    pastaLabel.className = styles.fieldLabel;
    pastaLabel.textContent = t("Pasta de modelos");
    const pastaValor = document.createElement("span");
    pastaValor.className = styles.mono;
    pastaValor.textContent = pastaModelos || t("escolha uma pasta fora do C:");
    pastaInfo.append(pastaLabel, pastaValor);
    corpo.appendChild(pastaInfo);

    const pastaAcoes = document.createElement("div");
    pastaAcoes.className = styles.actions;
    pastaAcoes.append(
      Button({
        label: t("Escolher pasta…"),
        size: "xs",
        variant: "secondary",
        onClick: () => {
          void (async () => {
            const escolhida = await api.pickFolder();
            if (!escolhida) return;
            pastaModelos = escolhida.replace(/\\/g, "/");
            pastaValor.textContent = pastaModelos;
          })();
        },
      }),
      Button({
        label: t("Abrir pasta"),
        size: "xs",
        variant: "ghost",
        onClick: () => {
          if (pastaModelos) void api.openProjectFolder(pastaModelos);
        },
      }),
    );
    corpo.appendChild(pastaAcoes);

    const acoes = document.createElement("div");
    acoes.className = styles.actions;
    acoes.append(
      Button({
        label: t("Baixar do Hugging Face"),
        size: "xs",
        variant: "secondary",
        onClick: () =>
          void openExternal(
            "https://huggingface.co/lm-kit/bge-m3-gguf/resolve/main/bge-m3-Q8_0.gguf",
          ),
      }),
      Button({
        label: t("Selecionar .gguf…"),
        size: "xs",
        variant: "primary",
        onClick: () => {
          void (async () => {
            const escolhido = await api.embeddingPickModel();
            if (!escolhido) return;
            try {
              embedInfoValor.textContent = t("carregando runtime…");
              embedStatus = await api.embeddingSave(
                ctx.root,
                escolhido,
                true,
                embedDevice,
              );
              embedStatus = await api.embeddingStart(ctx.root);
              atualizarEmbedding(embedStatus);
              status.textContent = t("modelo de embeddings configurado");
              handle.close();
            } catch (erro) {
              embedInfoValor.textContent = `erro: ${String(erro)}`;
            }
          })();
        },
      }),
    );
    corpo.appendChild(acoes);
    handle.body.appendChild(corpo);
    handle.footer.appendChild(
      Button({ label: t("Fechar"), variant: "ghost", onClick: () => handle.close() }),
    );
  };

  atualizarEmbedding(embedStatus);
  page.appendChild(embeddingCard);

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
    label: t("Remove all c2.json files"),
    variant: "destructive",
    size: "xs",
    disabled: !ctx.root || (ctx.status?.metadata ?? 0) === 0,
    onClick: () => {
      if (!ctx.root) return;
      void limparMetadados(ctx.root, {
        setStatus: (texto) => {
          status.textContent = texto;
        },
        onDone: () => ctx.onSaved(),
      });
    },
  });
  manutencaoBody.appendChild(limpar);

  const diagToggle = Toggle({
    label: t("Registrar cliques e latência (diagnóstico)"),
    checked: localStorage.getItem("c2.perf") === "1",
    tone: "brand",
    onChange: (ligado) => {
      if (ligado) {
        localStorage.setItem("c2.perf", "1");
        iniciarProbeUI();
      } else {
        localStorage.removeItem("c2.perf");
      }
    },
  });
  manutencaoBody.appendChild(diagToggle.root);

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
