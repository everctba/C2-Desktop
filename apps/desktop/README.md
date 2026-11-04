# C2 Desktop (Tauri)

Janela mínima do C2: abre um projeto, mostra o que está indexado, deixa
buscar por significado e controla o engine (`PLAY` / `STOP` / `REINDEX`).
Toda a lógica vem de `c2-core` — aqui não há regra de indexação duplicada.

## Pré-requisitos

| Item | Como |
|---|---|
| Node 22+ e pnpm | `node --version`, `pnpm --version` |
| Rust com **MSVC** | instalar Visual Studio Build Tools (bead `c2-e14.1`) e rodar `rustup default stable-x86_64-pc-windows-msvc` |
| WebView2 | já vem no Windows 10/11 |

> No Windows sem MSVC, o desktop **não compila** (Tauri depende do `link.exe`).
> O frontend, porém, roda no navegador com dados falsos (`pnpm dev`), o que
> permite validar o design antes de instalar o Build Tools.

## Desenvolvimento

```bash
cd apps/desktop
pnpm install

pnpm dev            # só o frontend, em http://127.0.0.1:1420 (modo preview)
pnpm tauri dev      # janela real, falando com o Core
pnpm build          # typecheck + build do frontend
pnpm tauri build    # instalador (depois do MSVC)
```

O modo preview detecta a ausência do Tauri (`__TAURI_INTERNALS__`) e usa
`src/api.ts` → mock. Nenhuma tela quebra sem o Core.

## Estrutura

```text
apps/desktop/
├── index.html
├── src/
│   ├── main.ts          shell: rail + toolbar + rota
│   ├── api.ts           única porta para o Core (invoke) + mock do navegador
│   ├── types.ts         espelho dos tipos serializados pelo c2-core
│   ├── styles/
│   │   ├── tokens.css   copiado de Design System/design-system-tokens.css
│   │   └── global.css   reset, scrollbar, shell, grid de cards
│   ├── ui/              primitivos (CSS Modules por componente)
│   │   ├── Button, Card, Pill, Input, TreeRow, Rail, Toolbar
│   └── views/
│       ├── Projects.ts  abrir/inicializar projeto, modelo, runtime
│       ├── Engine.ts    contadores, PLAY/STOP/REINDEX, arquivos + metadado
│       └── Search.ts    Semantic Grep com motivo e trecho
└── src-tauri/
    ├── src/lib.rs       comandos que chamam c2-core
    └── tauri.conf.json
```

## Design System

O frontend segue o **"Terminal Editorial"** (`Design System/DESIGN-SYSTEM.md`):

- `src/styles/tokens.css` é cópia do design system — **nenhum hex/rgb/hsl** em
  CSS de componente, só `var(--token)`;
- CSS Modules por componente (sem Tailwind, sem biblioteca de UI);
- densidade de ferramenta: fonte 10–13px, botões 26px, tree rows 28px;
- cor só com significado (accent determinístico por nome, estados semânticos);
- card sem borda sobre `--bg-surface` com gap de 1px (a divisória é o gap);
- `data-theme="light"` troca só tokens.

## Comandos expostos ao frontend

| Comando | O que faz |
|---|---|
| `workspace_status(root)` | `StatusReport` (arquivos, metadados, runtime, contadores) |
| `workspace_init(root)` | cria `.c2/config.json` |
| `workspace_reindex(root, full, limit)` | uma passada: sobe runtime, indexa, para o runtime |
| `engine_start(root)` / `engine_stop(root)` | laço contínuo com watchdog do runtime |
| `engine_running()` | estado do laço |
| `search_grep(root, query, limit)` | Semantic Grep |
| `file_read(root, file, start, end)` | trecho com número de linha |
| `file_related(root, file)` | usa / usado por |
| `list_files(root)` | arquivos indexados com resumo |
| `read_meta(root, file)` | metadado completo (`.c2.json`) |
| `config_get` / `config_set` | configuração efetiva |
| `pick_folder` / `pick_model` | seleção de pasta e de `.gguf` |

Eventos do engine para a UI: `engine://progress`, `engine://cycle`,
`engine://error`, `engine://stopped`.

## Limitações desta versão

- uma janela; sem múltiplos projetos abertos ao mesmo tempo;
- `PLAY` usa uma thread no processo da janela (fechar a janela encerra o engine);
- instalador ainda não embarca o runtime do llama.cpp (`c2-e12.1`);
- sem telas de configuração completa de modelo/GPU (bead `c2-e16`).
