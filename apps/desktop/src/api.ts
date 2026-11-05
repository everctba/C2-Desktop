/**
 * Camada unica de acesso ao Core.
 *
 * Dentro do Tauri usa `invoke`; no navegador (`pnpm dev` sem MSVC) usa dados
 * falsos, para permitir validar o design antes de compilar o desktop.
 */

import type {
  FileEntry,
  Hit,
  McpStatus,
  ModelInspection,
  ProviderTest,
  RuntimePlan,
  IndexReport,
  InitResult,
  MetaFile,
  ReadResult,
  RelatedResult,
  StatusReport,
} from "./types";

export const hasTauri = "__TAURI_INTERNALS__" in window;

type InvokeFn = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

let invokeFn: InvokeFn | null = null;

async function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!hasTauri) return mockInvoke<T>(command, args);
  if (!invokeFn) {
    const api = await import("@tauri-apps/api/core");
    invokeFn = api.invoke as InvokeFn;
  }
  return invokeFn<T>(command, args);
}

export function onEngineEvent(
  event: string,
  handler: (payload: unknown) => void,
): () => void {
  if (!hasTauri) return () => {};
  let unlisten: (() => void) | null = null;
  void (async () => {
    const api = await import("@tauri-apps/api/event");
    unlisten = await api.listen(event, (payload) => handler(payload.payload));
  })();
  return () => unlisten?.();
}

/** Abre uma URL no navegador padrao (fora do WebView). */
export async function openExternal(url: string): Promise<void> {
  if (hasTauri) {
    try {
      const { openUrl } = await import("@tauri-apps/plugin-opener");
      await openUrl(url);
      return;
    } catch {
      /* cai no fallback abaixo */
    }
  }
  window.open(url, "_blank", "noopener,noreferrer");
}

export const api = {
  initialRoot: () => invoke<string | null>("initial_root"),
  runtimePlan: (root: string, ctxSize?: number, gpuLayers?: number) =>
    invoke<RuntimePlan>("runtime_plan", { root, ctxSize, gpuLayers }),
  modelLoad: (root: string, ctxSize: number, gpuLayers: number) =>
    invoke<null>("model_load", { root, ctxSize, gpuLayers }),
  mcpSmoke: (root: string) => invoke<McpStatus>("mcp_smoke", { root }),
  promptDefault: () => invoke<string>("prompt_default"),
  apiKeySet: (key: string) => invoke<void>("api_key_set", { key }),
  apiKeyDelete: () => invoke<void>("api_key_delete"),
  apiKeyStatus: () => invoke<boolean>("api_key_status"),
  providerTest: (root: string) => invoke<ProviderTest>("provider_test", { root }),
  modelInspect: (path: string) => invoke<ModelInspection>("model_inspect", { path }),
  modelSet: (root: string, path: string) =>
    invoke<ModelInspection>("model_set", { root, path }),
  status: (root: string) => invoke<StatusReport | null>("workspace_status", { root }),
  init: (root: string) => invoke<InitResult>("workspace_init", { root }),
  reindex: (root: string, full = false, limit?: number) =>
    invoke<IndexReport>("workspace_reindex", { root, full, limit }),
  engineStart: (root: string) => invoke<null>("engine_start", { root }),
  engineStop: (root: string) => invoke<null>("engine_stop", { root }),
  engineRunning: () => invoke<boolean>("engine_running"),
  grep: (root: string, query: string, limit = 10) =>
    invoke<Hit[]>("search_grep", { root, query, limit }),
  read: (root: string, file: string, start?: number, end?: number) =>
    invoke<ReadResult>("file_read", { root, file, start, end }),
  related: (root: string, file: string) => invoke<RelatedResult>("file_related", { root, file }),
  listFiles: (root: string) => invoke<FileEntry[]>("list_files", { root }),
  readMeta: (root: string, file: string) => invoke<MetaFile>("read_meta", { root, file }),
  configGet: (root: string) => invoke<Record<string, unknown>>("config_get", { root }),
  configSet: (root: string, key: string, value: string) =>
    invoke<string>("config_set", { root, key, value }),
  pickFolder: () => invoke<string | null>("pick_folder"),
  pickModel: () => invoke<string | null>("pick_model"),
};

/* ───────────────────────── mock para o navegador ───────────────────────── */

const delay = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

const MOCK_ROOT = "C:/exemplo/loja-virtual";

const mockFiles: FileEntry[] = [
  { file: "src/auth/session.rs", what: "Sessão do usuário", summary: "Cria, renova e invalida sessões.", links: 2 },
  { file: "src/estoque/reserva.rs", what: "Reserva de estoque", summary: "Garante que a reserva não exceda a capacidade.", links: 3 },
  { file: "src/faturamento/nota.rs", what: "Emissão de nota", summary: "Fecha o pedido e gera a nota.", links: 4 },
  { file: "src/busca/mod.rs", what: "Busca de produtos", summary: "Consulta o catálogo por termo.", links: 1 },
  { file: "src/lib.rs", what: "Raiz do crate", summary: "Declara os módulos do domínio.", links: 4 },
];

const mockStatus: StatusReport = {
  root: MOCK_ROOT,
  initialized: true,
  config_path: `${MOCK_ROOT}/.c2/config.json`,
  model: {
    name: "gemma-4-e2b",
    path: "H:/modelos/gemma-4-E2B-it-Q4_K_M.gguf",
    host: "127.0.0.1",
    port: 8099,
    ctx_size: 8192,
    disable_thinking: true,
    provider: "local",
    base_url: null,
    api_key: false,
  },
  index: { max_file_bytes: 262144, extensions: 64, secret_policy: "skip", max_total_bytes_per_run: 67108864 },
  engine: { poll_interval_secs: 5 },
  runtime: {
    healthy: false,
    pid: null,
    binary: "C:/runtime/llama-server.exe",
    source: "workspace .c2/runtime",
    version: "version: 0.0.0-dev",
    url: "http://127.0.0.1:8099/health",
  },
  files: mockFiles.length,
  metadata: mockFiles.length,
  skipped: { binary: 0, too_large: 0, unsupported_extension: 0, sensitive: 1, other: 0 },
  state: {
    last_scan: Math.floor(Date.now() / 1000) - 42,
    indexed: mockFiles.length,
    unchanged: 0,
    removed: 0,
    errors: 0,
    llm_calls: mockFiles.length,
    prompt_tokens: 1840,
    completion_tokens: 410,
    latency_ms: 7320,
    last_error: null,
  },
};

async function mockInvoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  await delay(120);

  switch (command) {
    case "workspace_status":
      return mockStatus as T;
    case "workspace_init":
      return { created: true, files: 12 } as T;
    case "workspace_reindex":
      await delay(1200);
      return {
        indexed: 3,
        unchanged: 2,
        removed: 0,
        errors: 0,
        deferred: 0,
        relinked: 0,
        blocked: 0,
        masked: 0,
        calls: 3,
        prompt_tokens: 1200,
        completion_tokens: 300,
        latency_ms: 5400,
        last_error: null,
      } satisfies IndexReport as T;
    case "engine_start":
    case "engine_stop":
      return null as T;
    case "engine_running":
      return false as T;
    case "list_files":
      return mockFiles as T;
    case "search_grep": {
      const query = String(args?.query ?? "");
      const hits: Hit[] = mockFiles.slice(0, Math.min(3, mockFiles.length)).map((entry, index) => ({
        file: entry.file,
        score: Number((4.5 - index).toFixed(2)),
        path_score: 1,
        symbol_score: 0.8,
        semantic_score: 2.7,
        evidence: [
          { field: "semantic.what", terms: query.split(" ").filter(Boolean).slice(0, 2) },
          { field: "path", terms: ["src"] },
        ],
        summary: entry.summary,
        snippet: {
          line_start: 12,
          line_end: 16,
          text: "// trecho de exemplo no modo preview\npub fn exemplo() {\n    // o chamado real vem do Core\n}",
        },
      }));
      return hits as T;
    }
    case "file_read":
      return {
        file: String(args?.file ?? ""),
        line_start: 1,
        line_end: 3,
        lines: ["// preview", "// rode pnpm tauri dev para ler o arquivo real", ""],
      } as T;
    case "file_related":
      return {
        file: String(args?.file ?? ""),
        uses: [{ to: "src/auth/token.rs", type: "imports" }],
        used_by: [{ to: "src/lib.rs", type: "mod" }],
      } as T;
    case "read_meta":
      return {
        schema_version: 1,
        file: String(args?.file ?? ""),
        hash: "sha256:preview",
        size: 1200,
        mtime: Math.floor(Date.now() / 1000),
        semantic: {
          what: "Prévia no navegador",
          why: "Permitir validar o design sem compilar o desktop",
          goal: "Mostrar a interface com dados plausíveis",
          summary: "Dados falsos do modo preview",
          responsibility: "Demonstração",
        },
        links: [{ to: "src/auth/token.rs", type: "imports" }],
      } as T;
    case "config_get":
      return {} as T;
    case "config_set":
      return `${String(args?.key)} = ${String(args?.value)}` as T;
    case "pick_folder":
      return MOCK_ROOT as T;
    case "pick_model":
      return "H:/modelos/gemma-4-E2B-it-Q4_K_M.gguf" as T;
    case "api_key_set":
      return undefined as T;
    case "api_key_delete":
      return undefined as T;
    case "api_key_status":
      return false as T;
    case "provider_test":
      return { ok: true, provider: "openai_compat", model: "gpt-4o-mini", latency_ms: 820, sample: "mock" } as T;
    case "mcp_smoke":
      return {
        available: true,
        binary: "C:/C2/cli/c2.exe",
        protocol_version: "2024-11-05",
        server_name: "c2",
        tools: [
          { name: "c2_grep", description: "Busca semantica nos metadados do projeto." },
          { name: "c2_read", description: "Le um arquivo com numero de linha." },
          { name: "c2_related", description: "Dependencias pelo grafo estatico." },
          { name: "c2_map", description: "Arvore plana do projeto." },
        ],
        error: null,
      } as T;
    case "initial_root":
      return null as T;
    case "runtime_plan":
      return {
        gguf: {
          path: String(args?.root ?? "") + "/modelo.gguf",
          size_bytes: 3412060300,
          version: 3,
          tensor_count: 601,
          architecture: "gemma4",
          name: "Gemma 4 E2B",
          quantization: "Q4_K_M",
          file_type: null,
        },
        gpu: { name: "NVIDIA GeForce RTX 4060 Ti", total_mb: 8188, used_mb: 320, free_mb: 7868 },
        ctx_size: Number(args?.ctxSize ?? 8192),
        gpu_layers: Number(args?.gpuLayers ?? 99),
        estimate: {
          model_mb: 3256,
          kv_mb: Number(args?.ctxSize ?? 8192) / 8192 * 716,
          overhead_mb: 162,
          total_mb: 3256 + 162 + 256 + (Number(args?.ctxSize ?? 8192) / 8192) * 716,
          available_mb: 7868,
          fit: "fits",
        },
        loaded: false,
      } as T;
    case "model_load":
      await delay(1500);
      return null as T;
    case "model_inspect":
    case "model_set":
      return {
        gguf: {
          path: String(args?.path ?? "H:/modelos/gemma-4-E2B-it-Q4_K_M.gguf"),
          size_bytes: 3412060300,
          version: 3,
          tensor_count: 307,
          architecture: "gemma",
          name: "gemma-4-E2B-it",
          quantization: "Q4_K_M",
          file_type: 15,
        },
        gpu: { name: "NVIDIA GeForce RTX 4060 Ti", total_mb: 8188, used_mb: 320, free_mb: 7868 },
        fit: "fits",
      } as T;
    default:
      throw new Error(`comando sem mock: ${command}`);
  }
}
