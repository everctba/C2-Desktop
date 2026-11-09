/**
 * i18n do desktop: pt-BR (texto fonte) e en.
 *
 * `t("texto em português")` devolve o inglês quando o idioma é en; se faltar
 * tradução, mantém o português (fallback seguro). A escolha fica em
 * localStorage e em `%APPDATA%/dev.c2.desktop/language.json` (gravada pelo
 * instalador e pelo seletor do Config).
 */

import { api } from "./api";

export type Lang = "en" | "pt-BR";

const CHAVE = "c2.lang";

/** Inglês é o padrão (mesma decisão do instalador). */
let atual: Lang = "en";

const EN: Record<string, string> = {
  // ---- rail ----
  Projetos: "Projects",
  Engine: "Engine",
  Busca: "Search",
  Config: "Settings",
  Sobre: "About",
  "Navegação principal": "Main navigation",

  // ---- configurações ----
  Configurações: "Settings",
  "Grava direto em .c2/config.json. Mudanças de contexto/camadas valem no próximo carregamento do modelo.":
    "Writes directly to .c2/config.json. Context/layer changes apply on the next model load.",
  "Modo de uso": "Usage mode",
  "Escolha onde a IA roda. O restante da tela se adapta ao modo; o que não se aplica fica oculto (abra 'avançado' para ver tudo).":
    "Choose where the AI runs. The rest of the screen adapts; what does not apply stays hidden (open \"advanced\" to see everything).",
  "IA local (padrão)": "Local AI (default)",
  "Modelo GGUF na sua GPU. Privado, sem custo por token. Precisa de GPU de 8 GB.":
    "GGUF model on your GPU. Private, no per-token cost. Needs an 8 GB GPU.",
  "API Key / nuvem": "API Key / cloud",
  "OpenAI, Groq, DeepSeek, OpenRouter ou um servidor local (LM Studio/Ollama/vLLM).":
    "OpenAI, Groq, DeepSeek, OpenRouter or a local server (LM Studio/Ollama/vLLM).",
  ativo: "active",
  escolher: "choose",
  "avançado": "advanced",
  "modo local ativado": "local mode enabled",
  "modo API/nuvem ativado": "API/cloud mode enabled",
  "nao foi possivel trocar o modo: ": "could not change mode: ",

  Workspace: "Workspace",
  Runtime: "Runtime",
  "Modelo local": "Local model",
  "nenhum modelo": "no model",
  "não configurado": "not configured",
  "Selecionar .gguf…": "Select .gguf…",
  "modelo atualizado": "model updated",
  "Modelo inválido": "Invalid model",
  Erro: "Error",
  Entendi: "Got it",
  "O modelo é uma preferência do app: escolha uma vez e ele vale para todos os projetos (novos projetos herdam este caminho).":
    "The model is an app preference: choose once and it applies to every project (new projects inherit this path).",

  "Provedor de IA": "AI provider",
  "Local é o padrão: o modelo roda na sua máquina e nada sai dela. ":
    "Local is the default: the model runs on your machine and nothing leaves it. ",
  "Ao usar um provedor de nuvem, o conteúdo dos arquivos é enviado ao provedor ":
    "When using a cloud provider, file contents are sent to the provider ",
  "configurado e os tokens são cobrados na sua conta — só ative se for aceitável. ":
    "and tokens are billed to your account — only enable it if that is acceptable. ",
  "No modo API/BYO, os indicadores locais (runtime, VRAM, .gguf) não se aplicam: ":
    "In API/BYO mode, local indicators (runtime, VRAM, .gguf) do not apply: ",
  "quem gerencia o servidor é o provedor ou software externo.":
    "the provider or external software manages the server.",
  provedor: "provider",
  preset: "preset",
  personalizado: "custom",
  "base URL (nuvem)": "base URL (cloud)",
  "ex.: https://api.openai.com/v1": "e.g. https://api.openai.com/v1",
  modelo: "model",
  "nome enviado no campo model": "name sent in the model field",
  "preço entrada / 1M tokens": "input price / 1M tokens",
  "preço saída / 1M tokens": "output price / 1M tokens",
  "modo nuvem; 0 = não calcular": "cloud mode; 0 = do not calculate",
  "teto por execução (US$)": "per-run cap (USD)",
  "0 = sem teto; interrompe o ciclo ao atingir": "0 = no cap; stops the cycle when reached",
  "API key (cofre do sistema)": "API key (system vault)",
  "cole a chave do provedor": "paste the provider key",
  "nunca é gravada no .c2/config.json": "never written to .c2/config.json",
  "Salvar chave": "Save key",
  "Esquecer chave": "Forget key",
  "Testar conexão": "Test connection",
  "sem chave": "no key",
  "chave salva": "key saved",
  "API key salva no cofre do sistema": "API key saved in the system vault",
  "chave esquecida (removida do cofre local; revogue no provedor se necessario)":
    "key forgotten (removed from the local vault; revoke it at the provider if needed)",
  "testando provedor…": "testing provider…",
  "Prompt de indexação": "Indexing prompt",
  "prompt = instruções + conteúdo do arquivo 1 → resposta":
    "prompt = instructions + file 1 content → answer",
  "Estas instruções são enviadas ao modelo junto com o conteúdo de cada arquivo. A linha que trata o arquivo como DADO (e não instrução) é fixa e não pode ser removida.":
    "These instructions are sent to the model along with each file's content. The line that treats the file as DATA (not instruction) is fixed and cannot be removed.",
  "Restaurar padrão": "Restore default",
  "Salvar alterações": "Save changes",
  "Salvando…": "Saving…",
  Recarregar: "Reload",
  "nada para salvar": "nothing to save",
  "runtime no ar": "runtime up",
  "runtime parado": "runtime stopped",
  "runtime externo": "external runtime",
  " (salve para aplicar)": " (save to apply)",


  "extensões indexadas": "indexed extensions",
  "separadas por vírgula (rs, ts, py, md…)": "comma-separated (rs, ts, py, md…)",
  "diretórios excluídos": "excluded directories",
  "separados por vírgula (node_modules, dist…)": "comma-separated (node_modules, dist…)",
  "máx. por arquivo (KiB)": "max per file (KiB)",
  "máx. por execução (MiB)": "max per run (MiB)",
  "máx. links por arquivo": "max links per file",
  "respeitar .gitignore": "respect .gitignore",
  "política de segredos": "secret policy",
  "skip pula o arquivo; mask envia mascarado": "skip blocks the file; mask sends it masked",
  "modelo .gguf": ".gguf model",
  "o workspace guarda só o caminho": "the workspace stores only the path",
  host: "host",
  porta: "port",
  "contexto (tokens)": "context (tokens)",
  "camadas na GPU": "GPU layers",
  "threads (CPU)": "threads (CPU)",
  temperatura: "temperature",
  max_tokens: "max_tokens",
  "timeout (s)": "timeout (s)",
  tentativas: "retries",
  "desligar raciocínio": "disable reasoning",
  "LM Studio como fallback": "LM Studio as fallback",
  "polling do engine (s)": "engine polling (s)",
  "arquivos por ciclo": "files per cycle",
  "abra um projeto para configurar o workspace": "open a project to configure the workspace",
  "Abrir projeto": "Open project",
  "prompt restaurado (salve para aplicar)": "prompt restored (save to apply)",
  testando: "testing",
  Salvar: "Save",
  GGUF: "GGUF",

  "Abra a pasta de um projeto para o C2 indexar: o índice fica em .c2/ e os metadados ao lado de cada arquivo.":
    "Open a project folder so C2 can index it: the index lives in .c2/ and metadata sits next to each file.",
  "Projeto ainda não inicializado. Clique em Inicializar para criar .c2/config.json.":
    "Project not initialized yet. Click Initialize to create .c2/config.json.",
  "Nenhum projeto aberto.": "No project open.",
  Inicializar: "Initialize",
  "Trocar projeto": "Change project",
  "Primeiros passos": "Getting started",
  "Modo preview no navegador: dados falsos. Rode `pnpm tauri dev` para usar o Core real.":
    "Preview mode in the browser: fake data. Run `pnpm tauri dev` to use the real Core.",
  "Provedor": "Provider",
  "Teste do provedor": "Provider test",
  Fechar: "Close",
  "conexão ok": "connection ok",
  falhou: "failed",
  "Não foi possível alterar o raciocínio": "Could not change reasoning",
  "Usar este modelo?": "Use this model?",
  "Usar modelo": "Use model",
  "Falha ao salvar": "Failed to save",
  "cabe na VRAM": "fits in VRAM",
  "no limite da VRAM": "at the VRAM limit",
  "não cabe na VRAM livre": "does not fit in free VRAM",
  "sem GPU detectada": "no GPU detected",
  "recomendado · gemma 4 E2B": "recommended · gemma 4 E2B",
  "Copiar link": "Copy link",
  "Como baixar um modelo": "How to download a model",
  "Projetos recentes": "Recent projects",
  "nenhum modelo selecionado": "no model selected",
  "selecione um .gguf para poder iniciar": "select a .gguf to start",
  Endpoint: "Endpoint",
  raciocínio: "reasoning",
  "Quem faz o C2, versão do programa e como apoiar o projeto.":
    "Who makes C2, the program version and how to support the project.",
  "Quem faz": "Who makes it",
  "Criador do C2 — Comments 2.0 for AI. Desenvolvedor, designer e entusiasta de IA local.":
    "Creator of C2 — Comments 2.0 for AI. Developer, designer and local-AI enthusiast.",
  "Repositório do C2": "C2 repository",
  "Apoie o projeto": "Support the project",
  "O C2 é gratuito e continua gratuito: nenhuma funcionalidade fica atrás de doação. Se ele te economizou tempo, qualquer valor ajuda a manter o desenvolvimento. Doação opcional.":
    "C2 is free and stays free: no feature is behind a donation. If it saved you time, any amount helps keep development going. Donation is optional.",
  PayPal: "PayPal",
  internacional: "international",
  "Cartão ou saldo PayPal, em dólar. Funciona fora do Brasil.":
    "Card or PayPal balance, in USD. Works outside Brazil.",
  PIX: "PIX",
  "Pagamento instantâneo em reais, direto do seu banco.":
    "Instant payment in BRL, straight from your bank.",
  "escaneie com o app do PayPal": "scan with the PayPal app",
  Programa: "Program",
  Versão: "Version",
  Tecnologia: "Technology",
  "software gratuito e open source": "free and open-source software",
  "como configurar": "how to configure",

  atividade: "activity",
  ocioso: "idle",
  "aguardando próximo ciclo": "waiting for next cycle",
  parado: "stopped",
  "Reindexar (10)": "Reindex (10)",
  Log: "Log",
  "Arquivos indexados": "Indexed files",
  "nenhum metadado ainda — rode REINDEXAR": "no metadata yet — run REINDEX",
  links: "links",
  // search
  "digite pelo menos 2 caracteres — exemplos:": "type at least 2 characters — examples:",
  buscando: "searching",
  "onde o sistema valida permissão de usuário": "where the system validates user permission",
  Buscar: "Search",
  Limpar: "Clear",
  "ver relacionados": "show related",
  "ocultar relacionados": "hide related",
  "Anotações de agentes (hipóteses)": "Agent notes (hypotheses)",
  Metadado: "Metadata",
  // mcp
  "testando…": "testing…",
  Servidor: "Server",
  "Ferramentas expostas": "Exposed tools",
  "Conectar um agente": "Connect an agent",
  "nenhuma ferramenta retornada pelo servidor": "no tool returned by the server",
  "configuração do cliente MCP": "MCP client configuration",
  "Copiar MCP": "Copy MCP",
  Copiado: "Copied",
  // welcome
  "Conecte seu provedor (API Key)": "Connect your provider (API Key)",
  "Abrir Configuracoes": "Open Settings",
  "Testar conexao": "Test connection",
  "Abra a pasta do projeto primeiro (proximo passo).":
    "Open the project folder first (next step).",
  "Baixe um modelo local (GGUF)": "Download a local model (GGUF)",
  "Configurar nuvem": "Configure cloud",
  "Escolha a pasta do projeto": "Choose the project folder",
  "Trocar pasta": "Change folder",
  "Abrir projeto…": "Open project…",
  "Selecione o arquivo .gguf": "Select the .gguf file",
  "Escolha a pasta do projeto antes de selecionar o modelo.":
    "Choose the project folder before selecting the model.",
  Voltar: "Back",
  Pular: "Skip",
  Avançar: "Next",
  "Ir para a Engine": "Go to Engine",

  Iniciar: "Start",
  Parar: "Stop",
  Reindexar: "Reindex",
  "engine rodando": "engine running",
  "engine parado": "engine stopped",
  "Como você quer rodar a IA?": "How do you want to run the AI?",
  "Provedor de IA falhou": "AI provider failed",
  "Erro do provedor": "Provider error",
  "Custo estimado": "Estimated cost",
  Indexar: "Index",
  "Selecione um modelo para iniciar": "Select a model to start",
  "Não foi possível planejar a carga": "Could not plan the load",
  "Carregar modelo na GPU": "Load model on GPU",
  "Carregar na GPU": "Load on GPU",
  "Carregando…": "Loading…",
  "Contexto inválido (mínimo 512 tokens).": "Invalid context (minimum 512 tokens).",
  "Camadas na GPU inválidas.": "Invalid GPU layers.",
  "Estimativa": "Estimate",

  Projeto: "Project",
  "Arquivos descobertos": "Discovered files",
  Metadados: "Metadata",
  "Sensíveis pulados": "Sensitive skipped",
  "Máx. por arquivo": "Max per file",
  Extensões: "Extensions",
  "Teto por ciclo": "Per-cycle cap",
  Segurança: "Security",
  política: "policy",
  Binário: "Binary",
  Origem: "Origin",
  "não localizado": "not found",
  "runtime embarcado": "embedded runtime",
  atual: "current",
  Modelo: "Model",
  "workspace": "workspace",
  "Força reindexar mesmo sem mudanças; consome o modelo em todos os arquivos.":
    "Forces reindexing even without changes; consumes the model on every file.",
  "Reindexar tudo força reindexar mesmo os arquivos cujo hash não mudou. Use quando: trocou o modelo/prompt de indexação; quer regenerar todos os metadados; suspeita que algum metadado está ruim. É caro: chama o modelo para todos os arquivos (na nuvem, custa tokens).":
    "Reindex all forces reindexing even files whose hash did not change. Use it when: you changed the indexing model/prompt; you want to regenerate all metadata; you suspect some metadata is bad. It is expensive: it calls the model for every file (in the cloud, it costs tokens).",
  "nenhum metadado ainda — dê play para começar.": "no metadata yet — press play to start.",
  "prompt trocado (salve para aplicar)": "prompt changed (save to apply)",
  "falha ao carregar prompt": "failed to load prompt",
  "O idioma dos metadados é definido por este prompt, não pelo idioma da interface. Use English/Português acima ou edite livremente.":
    "The metadata language is defined by this prompt, not by the interface language. Use English/Portuguese above or edit freely.",
  "Trocar o idioma do prompt não traduz metadados antigos. Para regenerar tudo no novo idioma, rode Reindexar tudo na Engine.":
    "Changing the prompt language does not translate old metadata. To regenerate everything in the new language, run Reindex all in the Engine.",
  "Abrir pasta no Explorer": "Open folder in Explorer",
  "Parar engine": "Stop engine",
  "Iniciar engine": "Start engine",
  Manutenção: "Maintenance",
  "Limpar metadados…": "Clear metadata…",
  "Remove todos os metadados .c2.json do projeto. O código e o .c2/config.json não são alterados; para recriar o contexto, rode Reindexar tudo depois. Anotações de agentes serão perdidas.":
    "Removes all .c2.json metadata from the project. Code and .c2/config.json are untouched; to rebuild context, run Reindex all afterwards. Agent notes will be lost.",
  "Limpar metadados?": "Clear metadata?",
  "metadados serão removidos": "metadata entries will be removed",
  "Digite DELETE (maiúsculo) para confirmar": "Type DELETE (uppercase) to confirm",
  "Remover metadados": "Remove metadata",
  "removendo metadados…": "removing metadata…",
  "nenhum metadado para limpar": "no metadata to clear",
  "metadados removidos": "metadata entries removed",
  falhas: "failures",
  "falha ao limpar": "failed to clear",
  Cancelar: "Cancel",
  "Registrar cliques e latência (diagnóstico)": "Log clicks and latency (diagnostics)",
  reindexando: "reindexing",
  "Abortar reindexação": "Abort reindex",
  de: "of",
  "Carregando modelo…": "Loading model…",
  "Modelo carregado": "Model loaded",

  "PLAY indexa continuamente e supervisiona o llama-server. REINDEX faz uma passada só e para o runtime.":
    "PLAY continuously indexes and supervises llama-server. REINDEX runs one pass and stops the runtime.",
  Arquivos: "Files",
  descobertos: "discovered",
  indexados: "indexed",
  Pendentes: "Pending",
  "sem metadado": "without metadata",
  "Chamadas LLM": "LLM calls",
  "nesta máquina": "on this machine",
  "parado — clique em Iniciar para indexar": "stopped — click Start to index",
  "runtime saudável": "runtime healthy",
  "engine iniciado, aguardando atividade": "engine started, waiting for activity",
  "engine encerrado": "engine stopped",
  "Reindexar tudo": "Reindex all",
  "Descreva a responsabilidade que procura. O resultado mostra o motivo (campo e termos) e um trecho; clique para abrir o metadado completo.":
    "Describe the responsibility you are looking for. Results show the reason (field and terms) and a snippet; click to open the full metadata.",
  "onde calcula o hash dos arquivos": "where it computes file hashes",
  "cliente que fala com o modelo local": "client that talks to the local model",
  "busca semântica por significado": "semantic search by meaning",
  "buscando...": "searching...",
  "Servidor MCP por STDIO para agentes de IA. Somente leitura: grep, read, related e map sobre o Core, com resposta limitada.":
    "MCP server over STDIO for AI agents. Read-only: grep, read, related and map over the Core, with bounded responses.",
  "servidor pronto": "server ready",
  indisponível: "unavailable",
  protocolo: "protocol",
  servidor: "server",
  binário: "binary",
  "não encontrado": "not found",
  erro: "error",
  "Busca semantica nos metadados do projeto. Retorna arquivo, score, resumo e trecho.":
    "Semantic search over project metadata. Returns file, score, summary and snippet.",
  "Le um arquivo do projeto com numero de linha (faixa opcional).":
    "Reads a project file with line numbers (optional range).",
  "Dependencias de um arquivo pelo grafo estatico (usa / usado por).":
    "File dependencies from the static graph (uses / used by).",
  "Arvore plana do projeto: uma linha por arquivo indexavel.":
    "Flat project tree: one line per indexable file.",
  "Anexa uma anotacao de agente ao .c2.json do arquivo (campo agent_notes; nao altera codigo, semantic nem links). Use SOMENTE apos pensamento teleologico: por que existe, o que quebra se remover, o que o codigo nao revela — e apenas se isso nao for derivavel do codigo. Max 1-2 por tarefa; nunca anote o obvio.":
    "Appends an agent note to the file's .c2.json (agent_notes; does not change code, semantic fields or links). Use ONLY after teleological thinking: why it exists, what breaks if removed, what the code does not reveal — and only if that is not derivable from the code. Max 1-2 per task; never note the obvious.",
  "Português (Brasil)": "Portuguese (Brazil)",
  Português: "Portuguese",
  Brasil: "Brazil",
  Licença: "License",
  "workspace carregado": "workspace loaded",
  arquivos: "files",
  metadados: "metadata items",
  "Cole no seu cliente MCP (Claude Desktop, Cursor, etc.). O servidor é somente leitura, exceto `c2_annotate` (anotações no .c2.json do arquivo). Nada é enviado para fora da máquina.":
    "Paste into your MCP client (Claude Desktop, Cursor, etc.). The server is read-only except `c2_annotate` (notes in the file's .c2.json). Nothing leaves your machine.",
  "O software é gratuito. A ajuda é opcional.": "The software is free. Help is optional.",
  "O C2 é open source e continuará gratuito: nenhuma funcionalidade fica bloqueada por trás de doação. Se ele te economizou tempo, você pode contribuir com qualquer valor para manter o desenvolvimento.":
    "C2 is open source and will stay free: no feature is locked behind a donation. If it saved you time, you can contribute any amount to keep development going.",
  "um café": "a coffee",
  "uma hora de trabalho": "an hour of work",
  "ajuda de verdade": "real help",
  Doar: "Donate",
  "com PayPal": "with PayPal",
  "Você será levado ao PayPal; informe o valor de US$":
    "You will be taken to PayPal; enter the amount of US$",
  Original: "Original",
  Personalizar: "Custom",
  "Editar prompt?": "Edit prompt?",
  "Para editar, ative a opção Personalizar. No modo Original o prompt acompanha o idioma da interface.":
    "To edit, enable Custom. In Original mode the prompt follows the interface language.",
  "prompt personalizado: edite e salve para aplicar":
    "custom prompt: edit and save to apply",
  "prompt original restaurado (salve para aplicar)":
    "original prompt restored (save to apply)",
  "O prompt Original acompanha o idioma da interface. Personalizar permite editar e não muda quando o idioma da interface muda.":
    "The Original prompt follows the interface language. Custom lets you edit it and does not change when the interface language changes.",
  "Trocar o idioma não traduz metadados antigos. Para regenerar tudo no novo idioma, rode Reindexar tudo na Engine.":
    "Changing the language does not translate old metadata. To regenerate everything in the new language, run Reindex all in the Engine.",
  "Outro software do autor": "Another app by the author",
  "Conheça o AVIFnim": "Meet AVIFnim",
  "Crie animações AVIF e WebP profissionais no desktop — leve, local e feito para designers.":
    "Create professional AVIF and WebP animations on the desktop — lightweight, local and made for designers.",
  Conhecer: "Learn more",
  "Execução automática": "Automatic indexing",
  "Mantenha o C2 Desktop aberto ou na bandeja: ele indexa em segundo plano automaticamente. A CLI e o MCP apenas leem; não mantêm o índice atualizado sozinhos.":
    "Keep C2 Desktop open or in the tray: it indexes in the background automatically. The CLI and MCP only read; they do not keep the index updated on their own.",
  "Iniciar engine automaticamente ao abrir o projeto":
    "Start engine automatically when opening the project",
  "engine será iniciado automaticamente ao abrir o projeto":
    "engine will start automatically when opening the project",
  "engine automático desligado": "automatic engine disabled",
  "GPU e modelo": "GPU and model",
  "modelo carregado": "model loaded",
  "modelo não carregado": "model not loaded",
  Contexto: "Context",
  "Camadas GPU": "GPU layers",
  "não detectada": "not detected",
  "consultando…": "checking…",
  "0 (CPU)": "0 (CPU)",
  "PLAY indexa continuamente e supervisiona o llama-server.":
    "PLAY continuously indexes and supervises llama-server.",
  "O botão Reindexar tudo faz uma passada só e para o runtime.":
    "The Reindex all button runs one pass and stops the runtime.",
  "Modelo carregado na VRAM": "Model loaded in VRAM",
  "Model on GPU": "Model on GPU",
  "Descarregar modelo": "Unload model",
  "carregando modelo…": "loading model…",
  Idioma: "Language",
  "Idioma da interface": "Interface language",
  English: "English",
  "Escolha o idioma da interface. A CLI e as ferramentas MCP continuam em inglês.":
    "Choose the interface language. The CLI and MCP tools stay in English.",
  "idioma atualizado": "language updated",
  "Busca semântica (embeddings)": "Semantic search (embeddings)",
  "Busca por significado usando embeddings locais dos resumos .c2.json. O grep lexical continua; embeddings são um sinal extra de recall.":
    "Search by meaning using local embeddings of the .c2.json summaries. Lexical grep stays; embeddings add an extra recall signal.",
  "Ativar busca semântica": "Enable semantic search",
  "carregando runtime…": "loading runtime…",
  "Configurar modelo…": "Configure model…",
  "Abrir pasta de modelos": "Open models folder",
  "nenhum modelo configurado": "no model configured",
  "device de embeddings atualizado": "embeddings device updated",
  "Configurar busca semântica": "Configure semantic search",
  Arquivo: "File",
  "Pasta de modelos": "Models folder",
  "escolha uma pasta fora do C:": "choose a folder outside C:",
  "Escolher pasta…": "Choose folder…",
  "Abrir pasta": "Open folder",
  "Baixar do Hugging Face": "Download from Hugging Face",
  "modelo de embeddings configurado": "embedding model configured",
  "Indexar embeddings": "Index embeddings",
  Híbrida: "Hybrid",
  semântico: "semantic",
  semelhantes: "similar",
  "ocultar semelhantes": "hide similar",
  "sem similares (indexe os embeddings)": "no similar files (index embeddings first)",
  "indexando embeddings…": "indexing embeddings…",
};

export function idiomaAtual(): Lang {
  return atual;
}

export function t(texto: string): string {
  if (atual === "pt-BR") return texto;
  return EN[texto] ?? texto;
}

export function setLang(lang: Lang): void {
  atual = lang;
  try {
    localStorage.setItem(CHAVE, lang);
  } catch {
    /* storage indisponivel */
  }
  void api.languageSave(lang).catch(() => {
    /* melhor esforco */
  });
  // Re-renderiza a UI imediatamente (main.ts escuta este evento).
  window.dispatchEvent(new CustomEvent("c2:lang", { detail: lang }));
}

/** Carrega a preferencia (language.json do instalador > localStorage > en). */
export async function initLang(): Promise<void> {
  let lang: Lang = "en";
  try {
    const doArquivo = await api.languageLoad();
    if (doArquivo === "en" || doArquivo === "pt-BR") lang = doArquivo;
  } catch {
    /* segue */
  }
  if (lang === "en") {
    try {
      const salvo = localStorage.getItem(CHAVE);
      if (salvo === "pt-BR" || salvo === "en") lang = salvo;
    } catch {
      /* storage indisponivel */
    }
  }
  atual = lang;
}
