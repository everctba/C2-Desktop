# Briefing da landing page — C2

## 0. Slogan e copy

**Slogan:** **Comments 2.0 for AI**

**Frase-matadora (usar na landing e em posts):**
> Comente seu código de forma automatizada e semântica. Melhore a performance e
> o contexto da sua IA — e pare de gastar tokens relendo o projeto.

**Ideia em um parágrafo (base para hero e apresentações):**
O C2 comenta semanticamente cada arquivo com uma IA local: `what`, `why`,
`goal`, `summary`, `responsibility`. Um loop detecta o que mudou pelo hash e
reescreve só o comentário afetado. Sua IA paga consome esse comentário
atualizado em vez de reler o projeto inteiro — é um resumo vivo do código,
feito para consumo por IA. Comentário é código para humano; Comments 2.0 é
comentário para IA.

**Posicionamento:** **local-AI-first** como destaque — o **contexto** (índice,
busca, interpretação) roda local; o agente que escreve o código pode ser o melhor
modelo da nuvem. A nuvem — no C2 — é opção explícita (API key do usuário), nunca
escondida. **Não vender "100% offline"**: o workflow real é contexto local +
agente de código na nuvem.

**Vantagem central (usar na landing):** **consulta por intenção** — descrever o
problema em linguagem natural acha o código pelo significado (~86 ms), sem
depender do nome do arquivo nem de grep lexical; mais chance de achar o que o
agente precisa, mais rápido.

**Vantagem de hardware (usar na landing):** cada arquivo é interpretado numa
requisição isolada (sem histórico entre arquivos) — contexto pequeno, sem viés
cruzado e viável em máquina modesta. Não dizer que "não precisa de prompt
caching": cache é otimização opcional, não requisito.

**Terminologia:** no comparativo, a coluna sem o C2 chama-se **"Sem C2"** (não
"grep lexical").

**Beads:** `c2-e18` (épico), `c2-e18t1` (estrutura/copy) · **Status:** material pronto e site implementado em site/ (landing, ajuda e sobre)
**Fonte dos números:** `docs/bench-500.json`, `docs/bench-ab/resultado-piloto.md`,
`docs/bench-ab/resultado-cego.md`, `docs/mvp-gate.md`.

Este documento é a fonte da verdade para escrever a landing. Nada aqui é
estimativa: todo número tem medição registrada no repositório.

---

## 1. Posicionamento (uma frase)

> O C2 transforma seu código em **comentários semânticos** mantidos por IA
> local — para sua IA paga entender o projeto sem reler o repositório inteiro.

Público: desenvolvedores que usam agentes de IA (Claude Code, Cursor, etc.) em
projetos médios/grandes e gastam contexto relendo o mesmo código.

## 2. O problema (seção "por que existe")

- Agente de IA começa cada tarefa sem saber onde as coisas estão: gasta tokens
  redescobrindo estrutura.
- `grep` lexical falha quando o vocabulário da tarefa não é o vocabulário do
  código (medido: consulta semântica achou em 86 ms; busca sem C2, 0 resultados).
- Índices tradicionais (embeddings/banco vetorial) pedem infraestrutura e
  tiram o dado da máquina. O C2 é local, sem banco e sem embeddings.

## 3. O que é (como funciona, em 6 cartões)

1. **Metadado semântico ao lado do arquivo** — `<arquivo>.c2.json` com
   `what`, `why`, `goal`, `summary`, `responsibility` e `links[]`.
2. **Incremental por hash** — arquivo inalterado não chama o modelo; segunda
   passada em 501 arquivos custa **0,4 s** (cache por tamanho+mtime).
3. **Grafo estático** — imports/`use`/`require` viram arestas sem LLM;
   `c2 related` responde "quem usa / é usado por".
4. **Busca híbrida** — caminho + símbolo + metadado, em duas fases: só os
   candidatos têm o conteúdo lido.
5. **Runtime local embarcado** — llama.cpp oficial no pacote (Vulkan + CPU);
   modelo GGUF é referenciado por caminho, nunca copiado.
6. **Segurança por padrão** — segredos detectados antes do modelo
   (`skip`/`mask`), nada sai da máquina.

## 4. Benchmarks (seção "números")

### 4.1 Indexação de 501 arquivos (corpus sintético, Windows, RTX 4060 Ti 8 GB, Gemma 4 E2B Q4_K_M)

| Métrica | Valor |
|---|---|
| Primeira indexação | **36,7 min** (501 chamadas, 359.688 tokens, ~4,4 s/arquivo) |
| Segunda passada | **0,4 s · 0 chamadas** · 501 hashes do cache |
| `c2 grep` p50 / p95 | **60 ms / 82 ms** |
| `c2 related` | **49 ms** |
| Grafo | 501 de 501 metadados com links |
| Pacote Windows | **117,5 MB** (app 11,98 MB + CLI 4,28 MB + runtime ~101 MB) |

Evidência: `docs/bench-500.json`, `docs/mvp-gate.md` §3.

### 4.2 Verificação rápida — 100 arquivos

| Métrica | Valor |
|---|---|
| Primeira indexação | 343,6 s (101 chamadas, 65.472 tokens) |
| Segunda passada | 0,1 s · 0 chamadas |
| `c2 grep` p50 / p95 | 24 ms / 46 ms |

### 4.3 Ganhos medidos em otimizações

| Otimização | Antes | Depois |
|---|---|---|
| Cache de hash (300 arquivos, 2ª passada) | 1,17 s | **0,138 s** (8,5×) |
| Busca em duas fases (500 arquivos, p50) | 449 ms | **92 ms** (4,9×) |
| Busca em duas fases (p95) | 767 ms | **95 ms** (8,1×) |

### 4.4 Consulta semântica versus lexical (507 arquivos, 995 KB)

Consulta cujas palavras **não existem literalmente** no código
("onde impede valores acima do permitido"):

| | Com C2 | Sem C2 |
|---|---|---|
| Resultado | **achou** (`src/mod01/arquivo_001.rs`, score 10,20) | 0 resultados |
| Contexto | **1,3 KB** | 0 B (sem resposta) |
| Tempo | **86 ms** | 150 ms |
| Para responder | — | precisaria ler até **995 KB** |

### 4.5 A/B com agente de IA (transparência)

Projeto pequeno (8 arquivos, 6,5 KB), duas condições, rodada cega com agentes
sem conhecimento prévio:

| | Com C2 | Leitura direta |
|---|---|---|
| Acerto na rubrica (0–10) | 10 | 10 |
| Contexto consumido | ~16,3 KB | **~8,5 KB** |

**Segundo projeto (repositório do C2, 202 arquivos):** A/B cego deu 10/10 nas duas condições; C2 usou ~168 KB contra ~157 KB da leitura direta. Acerto empatado. Detalhe: [docs/bench-ab/resultado-cego-projeto-grande.md](../bench-ab/resultado-cego-projeto-grande.md).

**Leitura honesta:** em projeto pequeno o C2 **não compensa** — metadado + índice
custam mais que ler tudo. Ele se paga a partir de projetos médios, quando o
agente precisaria abrir 10–20% do repositório.

**Ponto de equilíbrio estimado:**

```text
≤ 20 arquivos          ler direto
100–500 arquivos       C2 ganha
milhares de arquivos   C2 ganha muito
```

### 4.6 Plataforma

- **Windows 10/11 (x64)**: validado (instalador, app, CLI, runtime).
- **Linux**: código compila e o build está automatizado
  (`scripts/build-linux.sh`), mas execução real foi adiada — não anunciar
  "suportado" até validar no Mint.
- **macOS**: fora do escopo.

## 5. Seção "quando NÃO usar" (diferencial de honestidade)

- Projeto pequeno (< ~20 arquivos): abrir os arquivos é mais barato.
- Uma tarefa pontual em um arquivo que você já sabe qual é.
- Máquina sem GPU e sem paciência para ~4 s/arquivo na primeira indexação.

Essa seção é o que separa a landing de marketing genérico — incluir de propósito.

## 6. Instalação (seção "começar")

```powershell
pwsh -File instalar.ps1                        # escolhe a pasta no disco
pwsh -File instalar.ps1 -Dest "D:\Apps\C2"     # direto
```

- instalador escolhe **pasta** (seletor gráfico), cria atalhos e põe o
  **CLI no PATH do usuário** (remove na desinstalação);
- avisa se já existe outro `c2` no PATH;
- não toca em nenhum projeto; modelo `.gguf` fica onde o usuário quiser.

Primeiro uso: abrir projeto → selecionar `.gguf` (valida nome/quantização/VRAM)
→ carregar na GPU → indexar → buscar.

## 7. Testemunhos (seção "o que as IAs dizem")

**Pendente:** o usuário vai fornecer prints de feedback de modelos de IA
(ex.: *"nossa, realmente esse sistema de comentário me ajudou a ser muito mais
efetivo"*).

Formato de cada item:

| Campo | O que é |
|---|---|
| `arquivo` | print otimizado em AVIF/WebP (tool `avifnim`) |
| `modelo` | qual IA (ex.: Claude, GPT, Gemini) |
| `tarefa` | o que ela estava fazendo (ex.: "refatorar módulo X") |
| `data` | quando aconteceu |
| `citacao` | a frase em texto (para SEO e leitor de tela) |

Regra: print **com** atribuição (modelo/tarefa/data) e a mesma frase em texto.
Sem atribuição, vira depoimento anônimo e perde força.

## 8. Estrutura da página

1. **Hero** — frase de posicionamento + print da tela Engine (log ao vivo).
2. **Problema** — tokens redescobrindo estrutura; Sem C2, só grep falha.
3. **Como funciona** — os 6 cartões da §3.
4. **Demonstração** — GIF/AVIF do fluxo: abrir → carregar modelo na GPU →
   indexar → buscar → metadado.
5. **Números** — §4 (com link para os relatórios do repositório).
6. **Quando não usar** — §5.
7. **Instalação** — §6 + comandos copiáveis.
8. **Privacidade** — local-first, segredos, sem telemetria.
9. **Testemunhos** — §7 (placeholders até os prints chegarem).
10. **FAQ** — ver §9.
11. **Rodapé** — licença, repositório, releases.

## 9. FAQ (respostas curtas)

- **Precisa de GPU?** Não; CPU funciona (mais lento). GPU via Vulkan acelera.
- **Precisa de internet?** Só para baixar o app e o modelo. O resto é local.
- **Meu código vai para algum servidor?** Não. Modelo e índice ficam na máquina.
- **Preciso deletar meus `.c2.json` para parar?** Não; são arquivos ao lado do
  código, e `c2 cleaner` remove.
- **Funciona em monorepo?** Sim; respeita `.gitignore` e tem filtros por pasta.
- **Quanto custa indexar?** ~4 s/arquivo na primeira vez (GPU 8 GB); depois é
  incremental e quase instantâneo.

## 10. Ativos que já existem

| Ativo | Onde |
|---|---|
| Captura do painel MCP | `docs/img/mcp-painel.png` |
| Relatório de benchmark 501 | `docs/bench-500.json` |
| Piloto A/B | `docs/bench-ab/resultado-piloto.md` |
| Rodada cega A/B | `docs/bench-ab/resultado-cego.md` |
| Protocolo/rubrica do A/B | `docs/bench-ab/protocolo.md`, `rubrica.md` |
| Gate do MVP | `docs/mvp-gate.md` |

**A capturar:** telas Projetos/Engine/Busca, GIF do fluxo completo, print de
`c2 status` no terminal, e os prints de testemunho.

## 11. Design

Usar o Design System "Terminal Editorial" (`Design System/design-system-tokens.css`):
tokens apenas, sem Tailwind, dark por padrão, cards sem borda com gap de 1 px,
tipografia 10–13 px. Pasta `site/` com build estático.

**Tipografia:** UI em sans-serif do sistema (`Segoe UI`/`system-ui`) e **IBM Plex
Mono** para código, comandos e números de benchmark (woff2 embutido, sem CDN —
funciona offline). Licença SIL OFL, ver `apps/desktop/src/styles/fonts/OFL.txt`.

## 12. O que NÃO prometer

- Linux "funciona" (ainda não validado).
- MCP "integrado com qualquer cliente" (o painel mostra a config; validação em
  clientes externos ainda não foi feita).
- "Substitui embeddings/bancos vetoriais" para busca em escala gigante — o C2 é
  híbrido lexical + metadado, sem embeddings, por decisão (`c2-dec6`).
- Números que não estejam neste briefing.
