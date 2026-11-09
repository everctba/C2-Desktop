# Como executar o C2

Guia passo a passo para instalar o C2, abrir o primeiro projeto, carregar o modelo e usar com agentes.

> O README é a apresentação do produto. Este arquivo é o manual de execução.

## Requisitos

| Item | Recomendado |
|---|---|
| Windows | 10 ou 11, x64 |
| GPU | 8 GB de VRAM para o modelo local (ex.: Gemma 4 E2B Q4) |
| Modelo | um arquivo `.gguf` local |
| Sem GPU | dá para usar um provedor OpenAI-compatible em Config → Modo de uso |
| Desenvolvimento | Rust stable, Node/pnpm, MSVC Build Tools |

O C2 não copia o `.gguf`: ele guarda apenas o caminho.

## 1. Instalar

### Instalador Windows

Baixe o instalador na página de releases:

```text
https://github.com/everctba/C2-Desktop/releases
```

Ou, se estiver com o repositório e o pacote gerado localmente:

```powershell
dist\installer\C2-Setup-0.1.0.exe
```

O instalador:

- instala o desktop, a CLI e o runtime embarcado;
- cria atalho no Menu Iniciar e na Área de Trabalho;
- adiciona `cli` ao `PATH` do sistema;
- pergunta, ao desinstalar, se deve limpar os dados do usuário.

### Pacote portátil

Na pasta do pacote:

```powershell
pwsh -File instalar.ps1
pwsh -File instalar.ps1 -Dest "D:\Apps\C2"
pwsh -File instalar.ps1 -Lang pt-BR
```

O portátil coloca a CLI no `PATH` do usuário. Para remover:

```powershell
pwsh -File desinstalar.ps1
```

### Compilar do código-fonte

```powershell
git clone https://github.com/everctba/C2-Desktop
cd C2-Desktop

cargo build --workspace
pnpm --dir apps/desktop install
```

Rodar em modo desenvolvimento:

```powershell
pwsh -File scripts/dev.ps1        # janela Tauri com hot reload
pwsh -File scripts/dev.ps1 -UiOnly # só frontend no navegador, com mocks
```

Gerar instalador:

```powershell
pwsh -File scripts/build-installer.ps1
# dist\installer\C2-Setup-<versao>.exe
```

## 2. Abrir o primeiro projeto

```powershell
cd C:\meu-projeto
c2 init
```

O `c2 init`:

1. cria `.c2/config.json`;
2. imprime as instruções para o agente;
3. **abre o C2 Desktop** já apontando para o projeto.

Na janela:

1. escolha o modelo `.gguf`;
2. confirme contexto e camadas na GPU;
3. aguarde o card **GPU E MODELO** mostrar `modelo carregado`;
4. o engine começa a indexar automaticamente.

A partir daí, fechar a janela no **X** mantém o C2 na bandeja e o engine rodando. Para encerrar de verdade, use o menu do tray → **Sair**.

### Modelo local

No primeiro uso, a tela **Config → Modelo local** permite selecionar o `.gguf`. O app valida cabeçalho, VRAM e contexto antes de carregar.

### Provedor em nuvem

Em **Config → Modo de uso → API Key / nuvem**, escolha o provedor e informe a API key. Nesse modo, o conteúdo dos arquivos é enviado ao provedor configurado — o app avisa antes.

## 3. Usar com um agente

Para gravar as regras no projeto:

```powershell
c2 init --agent
```

Isso grava:

```text
AGENTS.md
.mcp.json
.cursor/mcp.json
```

Para gerar só os arquivos MCP:

```powershell
c2 init --mcp
```

Cole o bloco `AGENTS.md` no seu cliente (Claude Code, Cursor, etc.) e configure o MCP com o JSON mostrado em **Config → MCP**.

O agente deve começar por:

```text
c2 status
c2 grep "<tarefa>" --limit 5
```

Depois lê o `.c2.json` do candidato e só então o trecho de código.

## 4. CLI essencial

```bash
c2 status                           # índice, fila, runtime, log
c2 reindex [--full] [--limit N]     # uma passada
c2 start / c2 stop                  # engine contínuo / parar
c2 grep "por significado" --limit 5 # busca nos metadados
c2 related src/auth/session.rs      # usa / usado por
c2 read src/auth/session.rs --lines 40:95
c2 note add src/auth/session.rs --text "..."
c2 migrate --yes                    # metadado antigo → schema v1
c2 logs --lines 20
```

Idioma do CLI:

```bash
c2 lang          # mostra o atual
c2 lang en
c2 lang pt-BR
c2 --lang en status
```

O instalador grava o idioma escolhido em `%APPDATA%\dev.c2.desktop\language.json`; CLI e desktop leem o mesmo arquivo.

## 5. MCP

```powershell
c2 --root . mcp
```

O servidor é **STDIO** e deve ser iniciado pelo cliente MCP, não digitado no terminal.

Ferramentas:

| Tool | Uso |
|---|---|
| `c2_grep` | busca pelos metadados |
| `c2_read` | lê um arquivo/faixa |
| `c2_related` | dependências do grafo |
| `c2_map` | árvore plana do projeto |
| `c2_annotate` | anotação de agente no `.c2.json` |

## 6. Onde ficam os dados

```text
<projeto>/.c2/config.json       configuração do projeto
<projeto>/.c2/logs/             logs
<arquivo>.c2.json               metadado ao lado do código
%APPDATA%\dev.c2.desktop\       preferências, idioma, modelo global
%LOCALAPPDATA%\dev.c2.desktop\  WebView2/cache, onboarded, recentes
```

O desinstalador **não** apaga os `.c2.json` dos projetos nem os `.gguf`.

## 7. Problemas comuns

### A janela abriu em branco ou “travada”

Use o build de console para ver logs:

```powershell
pwsh -File scripts/build-console.ps1
dist\desktop\C2-console.exe
```

### `c2` não é reconhecido

Feche e abra o terminal depois do instalador, ou use o caminho completo:

```text
<instalação>\cli\c2.exe
```

### O modelo não carrega

Confira:

- caminho do `.gguf` em Config → Modelo local;
- VRAM livre no card **GPU E MODELO**;
- contexto/camadas; reduza o contexto ou camadas se faltar VRAM;
- logs em `<projeto>/.c2/logs/`.

### “Model on GPU” não aparece

Ele só aparece quando o runtime responde `/health` e o modelo está de fato na VRAM. Enquanto carrega, o card mostra `carregando modelo… N%`.

### `c2 mcp` responde erro `-32700`

Você digitou um comando dentro do servidor MCP. Ele espera JSON-RPC no stdin. Abra o C2 Desktop (ou `c2 start` em outro terminal) e deixe o MCP ser iniciado pelo cliente.

## 8. Privacidade

- modelo e índice rodam localmente;
- segredos são detectados antes do modelo;
- sem telemetria;
- no modo nuvem, o conteúdo é enviado apenas ao provedor configurado, com aviso e estimativa de custo.
