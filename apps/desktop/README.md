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

> Nesta máquina o MSVC já está instalado (VS 2019 Build Tools em
> `E:\VSBuildTools2019` + Windows SDK 10.0.19041 em `E:\Windows Kits\10`,
> com `rustup default stable-x86_64-pc-windows-msvc`). Numa máquina nova,
> instale o Build Tools antes — o frontend, porém, roda no navegador com dados
> falsos (`pnpm dev`) mesmo sem ele.

## Desenvolvimento

```bash
cd apps/desktop
pnpm install

pnpm dev            # só o frontend, em http://127.0.0.1:1420 (modo preview)
pnpm tauri dev      # janela real com hot reload (sobe o Vite junto)
pnpm build          # typecheck + build do frontend
pnpm tauri build --debug --no-bundle   # binário standalone (assets embutidos)
pnpm tauri build    # instalador (release)
```

### Modos de execução (importante)

| Comando | O que acontece |
|---|---|
| `pnpm tauri dev` | Janela aponta para o Vite (`devUrl`) e recarrega ao editar |
| `pnpm tauri build --debug --no-bundle` | `src-tauri/target/debug/c2-desktop.exe` **standalone**: frontend embutido, não precisa do Vite |
| `cargo build` (debug, direto) | Usa `devUrl`; **sem o Vite rodando** a janela mostra `ERR_CONNECTION_REFUSED` em `127.0.0.1:1420` |
| `pnpm tauri build` | Release + instalador (msi/nsis) |

Para testar rápido sem servidor: `pnpm tauri build --debug --no-bundle` e abra o
`.exe` gerado. Para desenvolver: `pnpm tauri dev`.

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

## MCP

A aba **MCP** faz o handshake real com `c2 mcp` (`initialize` + `tools/list`)
ao abrir, mostra protocolo, servidor, lista de ferramentas e a configuração
para colar em um cliente MCP. Se a CLI não estiver ao lado do app, o painel
mostra "indisponível" com o motivo.

## Instalar em outra pasta

O instalador abre o **seletor gráfico de pasta** do Windows (padrão
`%LOCALAPPDATA%\C2`). Também aceita caminho direto, sem interface:

```powershell
pwsh -File instalar.ps1                       # abre o seletor de pasta
pwsh -File instalar.ps1 -Dest "D:\Apps\C2"   # instala direto
pwsh -File instalar.ps1 -NoGui                # prompt de texto
```

O instalador também coloca `<dest>\cli` no **PATH do usuário**, então em um
terminal novo `c2 --version` funciona de qualquer pasta (desinstalar remove a
entrada). Use `-NoPath` para não alterar o PATH.

**Conflito com outro `c2`:** o Windows procura primeiro no PATH do *sistema*, e
só depois no do usuário. Se já existir outro `c2.exe` (outro produto, script ou
versão antiga), o instalador **detecta e avisa** com o caminho encontrado; nesse
caso `c2` continuará chamando o outro. Opções: usar o caminho completo
(`<dest>\cli\c2.exe`), remover o outro do PATH, ou `-PrependPath` (coloca o C2
no início do PATH do usuário — resolve só se o outro também estiver no PATH do
usuário).

## Configurações

A rota **Config** (engrenagem no rail) edita o `.c2/config.json` do projeto:

- **Workspace**: extensões indexadas, diretórios excluídos, tamanho máximo por
  arquivo e por execução, links por arquivo, `.gitignore`, política de segredos
  (`skip`/`mask`).
- **Runtime**: caminho do `.gguf` (com seletor), host/porta, contexto, camadas na
  GPU, threads, temperatura, `max_tokens`, timeout, tentativas, raciocínio,
  fallback LM Studio, polling e arquivos por ciclo do engine.

Cada campo alterado é gravado pela mesma API do Core (`config set`), com
validação; o rodapé mostra quantos campos foram salvos e o runtime só usa
contexto/camadas novos no próximo carregamento do modelo.
## Selecionar modelo (`.gguf`)

Na tela **Projetos**, o card *Modelo* tem o botão **Selecionar .gguf…**:

1. abre o seletor de arquivos (filtrado por `.gguf`);
2. o Core lê **apenas o cabeçalho** do arquivo (`c2_core::gguf`) e mostra nome,
   arquitetura, tamanho, número de tensores, versão GGUF e quantização
   (`general.file_type`, senão `general.size_label`, senão o nome do arquivo);
3. consulta a GPU via `nvidia-smi` e mostra se cabe na VRAM livre
   (estimativa: tamanho + 10% + 512 MiB de KV cache);
4. só ao confirmar grava `model.path` e o nome em `.c2/config.json`.

O arquivo **nunca é copiado** — o workspace guarda apenas o caminho. Um `.gguf`
inválido é recusado antes de qualquer escrita.

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
