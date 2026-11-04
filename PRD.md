
**Versão:** 0.1
**Status:** Proposta inicial
**Plataformas:** Windows 10/11 e Linux
**Idioma inicial:** Português e inglês
**Tecnologia principal:** Rust + Tauri + llama.cpp

---

# 1. Visão do produto

O C2 é uma ferramenta desktop e CLI para indexação, compreensão e recuperação semântica de contexto de projetos de software.

Seu objetivo é permitir que desenvolvedores e agentes de IA compreendam a arquitetura, as responsabilidades, as relações e o funcionamento de um projeto sem precisar ler todos os seus arquivos.

O C2 mantém uma representação semântica persistente do projeto, atualizada conforme os arquivos são modificados.

1. **Desktop:** configuração, monitoramento e gerenciamento visual do projeto.
2. **CLI:** inicialização, controle e consultas pelo terminal.
3. **MCP:** disponibilização do contexto para agentes de programação.

## 1.1. Problema

Agentes de programação frequentemente precisam ler grandes quantidades de código para compreender um projeto.
Isso provoca:
- Consumo excessivo de tokens.
- Repetição de leituras.
- Perda de contexto arquitetural.
- Dificuldade para localizar responsabilidades.
- Desconhecimento das relações entre arquivos.
- Necessidade de reconstruir o entendimento do projeto em cada sessão.

pro momento 2.0 O C2 pretende resolver esse problema mantendo um índice semântico persistente e consultável.

## 1.2. Proposta de valor

Transformar o código-fonte em uma base de conhecimento arquitetural que possa ser consultada por significado.

**Princípio fundamental:**
O agente não precisa conhecer todo o projeto. Precisa conseguir descobrir rapidamente quais partes do projeto são relevantes para sua tarefa.

---

# 2. Objetivos do produto

## 2.1. Objetivos principais

- Indexar automaticamente projetos de software.
- Gerar metadados semânticos utilizando modelos locais.
- Detectar alterações reais nos arquivos.
- Evitar reprocessamento desnecessário.
- Permitir pesquisas semânticas e estruturais.
- Disponibilizar contexto através de MCP.
- Funcionar independentemente do VS Code.
- Executar modelos locais sem exigir LM Studio.
- Oferecer distribuição multiplataforma.

## 2.2. Não objetivos da primeira versão

O C2 inicialmente não pretende:

- Substituir agentes de programação.
- Editar código automaticamente.

- Construir um sistema completo de RAG.
- Implementar um banco vetorial complexo.


---

# 3. Arquitetura geral

O C2 deverá possuir um núcleo independente das interfaces.

```text
                    C2 ECOSYSTEM
                          |
              +-----------+-----------+
              |           |           |
             CLI        Desktop       MCP
              |           |           |
              +-----------+-----------+
                          |
                      C2 CORE
                          |
        +-----------------+-----------------+
        |                 |                 |
     Indexer         Semantic Engine    Project Manager
        |                 |                 |
     SHA-256          Semantic Grep      Configuração
     File Watcher     Context Retrieval  Workspace
     Metadata         Search Engine      Dependencies
        |                 |
        +-----------------+
                          |
                    LLM Runtime
                          |
                      llama.cpp
                          |
                    Local GGUF Model
```

## 3.1. C2 Core

Implementado em Rust.

Responsável por:

- Gerenciar projetos.
- Percorrer diretórios.
- Calcular hashes.
- Monitorar alterações.
- Gerenciar metadados.
- Executar indexação semântica.
- Realizar pesquisas.
- Recuperar contexto.
- Gerenciar o runtime de IA.
- Expor operações para CLI, Desktop e MCP.


## 3.2. Desktop

Implementado com Tauri.

Responsável por:

- Abrir e selecionar projetos.
- Configurar modelos.
- Configurar regras de indexação.
- Controlar execução.
- Exibir estatísticas.
- Visualizar arquivos indexados.
- Consultar atividades.
- Visualizar resultados do Semantic Grep.

O desktop funcionará como painel de controle do C2 Core.

## 3.3. CLI

Executável independente.

Responsável por:

- Inicializar projetos.
- Abrir o desktop.
- Iniciar e interromper o serviço.
- Consultar informações.
- Executar pesquisas.
- Gerenciar configurações.
- Controlar o runtime.

## 3.4. MCP Server

vamos fazer ele por ultimo
a ideia inicial é gerar uma instrução quando escrever no terminal c2 init   ele abre o software desktop e o MCP entraria como uma extensão para caso a ai quiser em vez de usar a instrução pesquisar coisas direto pelo mcp que pode usar um grap, ter um outro indice de nome de arquivo e o que ele faz e em que pasta e uma frase de descrição smeanticca para grap em bash sabe, depois vc evolui essa ideia e me da mais dicas
---

## 4.1. Inicialização
O usuário abre o terminal dentro de um projeto no VS Code:
```bash
cd meu-projeto
c2 init
```

O C2 deverá:

1. Identificar o diretório atual.
2. Verificar se já existe um workspace C2.
3. Criar a estrutura `.c2/`, caso necessário.
4. Registrar o projeto.
5. Verificar as configurações do runtime.
6. Abrir o aplicativo desktop.
7. Apresentar a configuração inicial.

Se o projeto já estiver inicializado, o C2 deverá abrir seu workspace existente.

## 4.2. Configuração inicial

O desktop deverá apresentar:

**Projeto**
- Nome.
- Diretório raiz.
- Linguagens detectadas.
- Quantidade de arquivos.

**Runtime**
- Modelo selecionado.
- Caminho do modelo.
- Backend de inferência.
- Configurações de GPU.
- Contexto máximo.
- Temperatura.

**Indexação**
- Diretórios incluídos.
- Diretórios excluídos.
- Extensões permitidas.
- Arquivos ignorados.
- Tamanho máximo de arquivo.

**Privacidade**
- Exclusão de arquivos sensíveis.
- Exclusão de credenciais.
- Controle sobre envio de conteúdo ao modelo local.

## 4.3. Execução

O usuário pressiona PLAY.

O C2 inicia:

1. C2 Engine.
2. File Watcher.
3. LLM Runtime, quando necessário.
4. Semantic Indexer.
5. MCP Server, se habilitado.

O desktop passa a apresentar o estado dos serviços.

## 4.4. Execução contínua

Enquanto o usuário trabalha normalmente no VS Code:

- O C2 detecta alterações.
- Calcula hashes.
- Identifica arquivos modificados.
- Atualiza os metadados necessários.
- Mantém o índice disponível para consultas.

O usuário não precisa interagir continuamente com o desktop.

## 4.5. Encerramento

O botão STOP deverá interromper os serviços iniciados pelo workspace.

O sistema deverá permitir fechar a janela sem necessariamente interromper o C2 Engine.

A execução em segundo plano deverá ser configurável.

---
Se abrir o c2 Desktop pelo icone do desktop ele nao vai saber a pasta que esta trabalhando então ter uma seleção de projetos uma lista ou adicionar um projeto novo.




O indexador deverá:

1. Descobrir arquivos.
2. Aplicar filtros.
3. Calcular SHA-256.
4. Verificar a existência de metadados.
5. Comparar hashes.
6. Solicitar análise ao LLM quando necessário.
7. Validar a resposta.
8. Atualizar o arquivo de metadados.

## 5.2. Detecção de alterações

O SHA-256 será o mecanismo primário de detecção de alterações de conteúdo.

Regras:

- Arquivo novo: indexar.
- Hash diferente: reindexar.
- Hash igual: não reindexar.
- Arquivo removido: remover seus metadados.
- Arquivo temporariamente inacessível: preservar metadados existentes e registrar erro.

O sistema deverá utilizar escrita atômica para evitar corrupção dos arquivos JSON.

Também deverá verificar se o arquivo não mudou durante sua análise.

## 5.3. File Watcher

O sistema deverá detectar eventos do sistema de arquivos.

Eventos relevantes:

- Criação.
- Modificação.
- Remoção.
- Renomeação.

Deverá implementar debounce para evitar múltiplas análises durante gravações sucessivas.

Uma varredura periódica de reconciliação poderá complementar o File Watcher para recuperar eventos perdidos.

## 5.4. Filtros

Exclusões iniciais:

```text
.git/
node_modules/
dist/
build/
target/
__pycache__/
.venv/
vendor/
```
q
Arquivos binários e arquivos acima do limite configurado não deverão ser enviados automaticamente ao modelo.

Arquivos contendo credenciais, tokens e senhas deverão ser excluídos por padrão ou passar por uma política explícita de sanitização.

---

# 6. Sistema de metadados C2

## 6.1. Formato inicial

Manter arquivos JSON independentes ao lado dos arquivos originais.

Exemplo:

```text
src/main.rs
src/main.rs.c2.json
```

## 6.2. Schema inicial

```json
{
  "schema_version": 1,
  "file": "src/main.rs",
  "hash": "sha256...",
  "mtime": 1790000000,
  "semantic": {
    "why": "...",
    "when": "...",
    "goal": "...",
    "summary": "..."
  }
}
```

Os campos iniciais:

| Campo | Descrição |
|---|---|
| why | Por que o arquivo existe. |
| when | Quando ou em quais condições é utilizado. |
| goal | Qual objetivo ele busca alcançar. |
| summary | Resumo de sua responsabilidade. |

## 6.3. Evolução semântica

O schema deverá permitir expansão futura.

Campos planejados:

- `responsibility`
- `concepts`
- `inputs`
- `outputs`
- `dependencies`
- `interfaces`
- `constraints`
- `decisions`
- `related_files`

A expansão não deverá exigir que todos os campos estejam presentes.

Os metadados deverão ser versionados para permitir migrações futuras.

---

# 7. Semantic Grep

## 7.1. Definição

O Semantic Grep é o mecanismo central de recuperação de contexto do C2.

Ele permite pesquisar informações pelo significado, e não somente por correspondência textual.

Exemplo:

```bash
c2 grep "Onde o sistema controla o risco financeiro?"
```

Mesmo que os arquivos utilizem nomes como:

```text
exposure_manager.py
position_sizing.py
capital_guard.py
```

O C2 deverá identificar sua possível relação com a pergunta.

## 7.2. Objetivos

- Encontrar arquivos relevantes.
- Encontrar responsabilidades.
- Identificar conceitos relacionados.
- Recuperar trechos de código.
- Apresentar justificativas para os resultados.
- Reduzir a quantidade de arquivos que o agente precisa ler.

## 7.3. Estratégia de busca

A primeira versão deverá implementar busca híbrida, sem exigir banco vetorial.

### Camada 1 — Busca lexical

Pesquisa por:

- Nomes de arquivos.
- Funções.
- Classes.
- Variáveis.
- Palavras-chave.
- Símbolos.

### Camada 2 — Busca semântica

Pesquisa nos metadados:

- why.
- when.
- goal.
- summary.
- responsibility.
- concepts.

A recuperação inicial poderá usar pontuação textual ponderada e expansão de termos.

### Camada 3 — Análise contextual

Quando necessário, o Gemma poderá interpretar a pergunta e ajudar a classificar os resultados encontrados.

Essa etapa deverá ser opcional para evitar chamadas desnecessárias ao LLM.

### Camada 4 — Recuperação do código

Após identificar os arquivos relevantes, o C2 deverá conseguir retornar:

- Caminho do arquivo.
- Trecho relevante.
- Número das linhas.
- Referência aos metadados utilizados.


## 7.4. Exemplo de resposta

Pergunta:

"Como o sistema controla as permissões de usuários?"

Resultado esperado:

```text
1. auth/permission_manager.py

Responsabilidade:
Controle de permissões.

Relevância:
Alta.

Motivo:
Define as regras de autorização e verifica
se o usuário pode executar determinadas operações.

2. auth/session.py

Responsabilidade:
Gerenciamento de sessões.

Relevância:
Média.

Motivo:
Fornece a identidade do usuário utilizada
durante as verificações de autorização.
```

Os resultados deverão indicar quando a informação é inferida e quando foi confirmada diretamente no código.

## 7.5. Comandos CLI

```bash
c2 grep "pergunta semântica"

c2 grep "autenticação" --limit 10

c2 grep "controle de permissões" --files-only

c2 grep "gerenciamento de sessões" --explain
```

---

# 8. Recuperação de contexto

O C2 deverá permitir que um agente obtenha somente as informações necessárias para uma tarefa.

Exemplo:

O agente está trabalhando em um módulo de autenticação.

Ele solicita:

1. Onde está implementada a autenticação?
2. Quais arquivos estão relacionados?
3. Quais funções realizam a validação?
4. Quais arquivos dependem desse módulo?
5. Quais regras arquiteturais precisam ser respeitadas?

O C2 deverá montar uma resposta contextual com referências aos arquivos e trechos correspondentes.

A recuperação deverá ser limitada por quantidade de resultados e tamanho de contexto.

O sistema não deverá enviar automaticamente o projeto inteiro ao agente.

---

# 9. MCP

## 9.1. Ferramentas iniciais

| Ferramenta | Responsabilidade |
|---|---|
| `c2_grep` | Pesquisa semântica. |
| `c2_search` | Pesquisa lexical. |
| `c2_read` | Leitura de arquivos e trechos. |
| `c2_related` | Recuperação de arquivos relacionados. |
| `c2_context` | Montagem de contexto para uma tarefa. |
| `c2_status` | Estado do workspace e indexação. |
| `c2_reindex` | Solicitação de reindexação. |

## 9.2. Exemplo

O agente solicita:

```text
c2_grep(
  query: "Onde o sistema valida as permissões?"
  limit: 5
)
```

O C2 retorna resultados estruturados.

O agente poderá então solicitar `c2_read` para examinar os trechos relevantes.

## 9.3. Requisitos

- Transporte STDIO inicialmente.
- Comunicação compatível com o protocolo MCP.
- Logs enviados para stderr.
- Respostas estruturadas.
- Controle de acesso aos diretórios do workspace.
- Limites de leitura e tamanho de resposta.

As ferramentas MCP deverão consultar o C2 Core, sem duplicar sua lógica.

---

# 10. Runtime de IA

## 10.1. Tecnologia

Motor de inferência: llama.cpp.

Modelo inicial: família Gemma 4 em formato GGUF, desde que disponível em uma variante compatível.

O modelo deverá ser configurável.

## 10.2. Gerenciamento

O C2 Runtime será responsável por:

- Localizar o executável do llama.cpp.
- Localizar o modelo.
- Iniciar o processo.
- Configurar o backend.
- Configurar a GPU.
- Monitorar o processo.
- Verificar disponibilidade.
- Encerrar o processo quando necessário.
- Reiniciar em caso de falha, respeitando limites de tentativas.

## 10.3. Configurações

- Modelo.
- Quantização.
- Contexto.
- Temperatura.
- GPU offload.
- Threads da CPU.
- Porta HTTP local.
- Timeout.
- Limite de tokens.

## 10.4. Independência

O C2 não deverá depender do LM Studio.

O runtime deverá funcionar diretamente com llama.cpp.

A API HTTP será uma implementação interna de comunicação entre o C2 Core e o motor auxiliar.

Uma futura versão poderá integrar o llama.cpp diretamente como biblioteca nativa.

---

# 11. Desktop

## 11.1. Tecnologia

- Tauri.
- Rust.
- HTML.
- CSS.
- TypeScript.

## 11.2. Telas

### Dashboard

Apresentar:

- Nome do projeto.
- Diretório.
- Estado do Engine.
- Estado do LLM.
- Quantidade de arquivos indexados.
- Arquivos pendentes.
- Últimas atividades.

### Indexação

Apresentar:

- Arquivos processados.
- Arquivos ignorados.
- Erros.
- Hashes.
- Histórico de processamento.

### Semantic Grep

Interface para realizar pesquisas semânticas e visualizar:

- Resultados.
- Relevância.
- Responsabilidades.
- Trechos de código.
- Relações entre arquivos.

### Runtime

Configuração do modelo, GPU, contexto e parâmetros de inferência.

### Workspace

Configuração dos diretórios, exclusões e regras do projeto.

### MCP

Visualização das ferramentas disponíveis e estado do servidor.

## 11.3. Controles principais

- PLAY.
- STOP.
- REINDEX.
- OPEN PROJECT.
- OPEN TERMINAL.
- REMOVER TODOS C2 json

O estado dos controles deverá refletir o estado real do Engine.

---

# 12. CLI

Comandos iniciais propostos:

| Comando | Função |
|---|---|
| `c2 init` | Inicializa workspace e abre desktop. |
| `c2 start` | Inicia o Engine. |
| `c2 stop` | Interrompe o Engine. |
| `c2 status` | Consulta o estado. |
| `c2 grep` | Pesquisa semântica. |
| `c2 search` | Pesquisa lexical. |
| `c2 read` | Lê arquivo ou trecho. |
| `c2 reindex` | Reindexa o projeto. |
| `c2 mcp` | Inicia servidor MCP. |
| `c2 config` | Gerencia configurações. |
| `c2 cleaner` | Remover todos os .c2.json |

A CLI deverá funcionar sem interface gráfica.

---

# 13. Organização de arquivos

Estrutura sugerida:

```text
meu-projeto/
│
├── .c2/
│   ├── config.json
│   ├── context.json
│   │
│   ├── index/
│   │   ├── files/
│   │   └── global/
│   │
│   ├── logs/
│   │
│   └── cache/
│
├── src/
├── tests/
└── package.json
```

Os arquivos originais deverão permanecer intactos.

A pasta `.c2/` armazenará configurações, índices, caches e informações específicas do workspace.

Os metadados individuais `.c2.json` poderão permanecer ao lado dos arquivos, conforme a estratégia atual, com possibilidade de configuração para armazenamento centralizado no futuro.

---

# 14. Segurança e privacidade

Requisitos obrigatórios:

- Execução local por padrão.
- Nenhum envio automático para serviços externos.
- Exclusão de segredos e credenciais.
- Validação dos caminhos acessados.
- Proteção contra leitura fora do workspace.
- Limites de tamanho de arquivos.
- Controle de processos iniciados pelo C2.
- Comunicação local restrita ao necessário.
- Validação de respostas do modelo.
- Tratamento de conteúdo de arquivos como dados não confiáveis, nunca como instruções de controle do sistema.

O C2 deverá informar claramente quando uma funcionalidade exigir comunicação externa.

---

# 15. Compatibilidade

## Windows

- Windows 10 e 11.
- Executável nativo.
- Suporte a CUDA quando disponível.
- Interface Tauri.
- CLI independente do Git Bash.

## Linux

- Distribuições modernas.
- Executável nativo.
- Suporte a CUDA e outros backends compatíveis.
- Interface Tauri com dependências de WebView do sistema.

## Requisitos de distribuição

O instalador deverá gerenciar:

- Aplicativo C2.
- Runtime llama.cpp.
- Modelo GGUF.
- Configurações.
- Atualizações.

O modelo deverá poder ser instalado separadamente para evitar instaladores excessivamente grandes.

---

# 16. Requisitos não funcionais

| Requisito | Objetivo |
|---|---|
| Portabilidade | Windows e Linux. |
| Persistência | Metadados preservados entre execuções. |
| Incrementalidade | Não reprocessar arquivos sem alterações. |
| Confiabilidade | Não perder metadados em falhas de processamento. |
| Modularidade | Núcleo independente das interfaces. |
| Escalabilidade | Suportar projetos com milhares de arquivos. |
| Privacidade | Processamento local por padrão. |
| Eficiência | Evitar chamadas desnecessárias ao LLM. |
| Observabilidade | Logs e histórico de operações. |

---

# 17. Roadmap de desenvolvimento

## Fase 1 — C2 Core

Objetivo: reproduzir o comportamento atual do Bash em Rust.

Entregas:

- Leitura de arquivos.
- SHA-256.
- Filtros.
- Metadados JSON.
- Integração HTTP com llama.cpp.
- Validação de respostas.
- Indexação incremental.
- Limpeza de órfãos.

Critério de conclusão: o C2 Rust consegue indexar um projeto e produzir metadados equivalentes aos do indexador Bash.

## Fase 2 — CLI e Runtime

Entregas:

- CLI funcional.
- Gerenciamento do llama-server.
- Configuração do modelo.
- Inicialização do Engine.
- Monitoramento de processos.
- Windows e Linux.

Critério de conclusão: o C2 funciona sem Git Bash e sem LM Studio.

## Fase 3 — Semantic Grep

Entregas:

- Busca lexical.
- Busca em metadados.
- Ranking de relevância.
- Leitura seletiva.
- Recuperação de trechos.
- Explicação dos resultados.

Critério de conclusão: uma pergunta em linguagem natural consegue localizar arquivos relevantes sem precisar ler o projeto inteiro.

## Fase 4 — MCP

Entregas:

- Servidor STDIO.
- Ferramentas de consulta.
- Leitura de contexto.
- Recuperação de dependências disponíveis.
- Integração com agentes.

Critério de conclusão: um agente externo consegue consultar o projeto pelo C2.

## Fase 5 — Desktop

Entregas:

- Interface Tauri.
- Dashboard.
- Configurações.
- PLAY/STOP.
- Visualização da indexação.
- Semantic Grep visual.
- Gerenciamento do runtime.

Critério de conclusão: o usuário consegue inicializar, configurar e controlar o C2 sem utilizar comandos manuais.

## Fase 6 — Contexto arquitetural avançado

Entregas futuras:

- Contexto global.
- Grafo de dependências.
- Relações entre responsabilidades.
- Histórico de decisões.
- Busca vetorial.
- Embeddings.
- Análise de impacto de alterações.
- Recuperação contextual avançada.

---

# 18. Critérios de aceitação do MVP

O MVP será considerado funcional quando:

1. O usuário instalar o C2 em Windows ou Linux.
2. Executar `c2 init` dentro de um projeto.
3. O desktop reconhecer o workspace.
4. O usuário configurar um modelo local.
5. O usuário pressionar PLAY.
6. O C2 iniciar o runtime e indexar os arquivos.
7. Alterações nos arquivos provocarem atualização dos metadados.
8. Arquivos inalterados não provocarem novas chamadas ao LLM.
9. Arquivos excluídos terem seus metadados órfãos removidos.
10. O usuário conseguir realizar pesquisas pelo Semantic Grep.
11. Um agente MCP conseguir consultar os metadados e recuperar arquivos.
12. O projeto continuar funcionando sem LM Studio ou Git Bash.

---

# 19. Decisões arquiteturais consolidadas

| Decisão | Escolha |
|---|---|
| Linguagem principal | Rust |
| Desktop | Tauri |
| CLI | Rust |
| MCP | Rust |
| Motor de inferência | llama.cpp |
| Modelo inicial | Gemma 4 GGUF compatível |
| Comunicação com LLM | HTTP local via llama-server |
| Metadados | JSON |
| Hash | SHA-256 |
| Monitoramento | File Watcher |
| Busca inicial | Híbrida, lexical + metadados |
| Transporte MCP | STDIO |
| Banco de dados inicial | Sem SQLite obrigatório |
| Sistemas operacionais | Windows e Linux |

---

# 20. Princípios de engenharia

1. **Um núcleo, múltiplas interfaces:** Desktop, CLI e MCP compartilham a mesma implementação.
2. **Local-first:** o processamento acontece na máquina do usuário.
3. **Incrementalidade:** somente arquivos relevantes e alterados devem ser reprocessados.
4. **Contexto antes de código:** o agente consulta os metadados antes de solicitar a leitura de arquivos.
5. **Evidência antes de inferência:** resultados semânticos devem apontar para fontes verificáveis.
6. **Modularidade:** novos mecanismos de busca e modelos podem ser adicionados sem reescrever o núcleo.
7. **Simplicidade inicial:** não introduzir bancos de dados e infraestrutura complexa sem necessidade comprovada.
8. **Independência:** o C2 deve funcionar sem depender de IDE, Git Bash ou LM Studio.

---

# 21. Definição final do produto

O C2 é um motor de inteligência contextual para projetos de software.

Ele observa o projeto, mantém uma representação semântica atualizada e oferece mecanismos para recuperar informações relevantes sob demanda.

Seu principal diferencial não é simplesmente indexar código, mas permitir que agentes de IA descubram:

- Onde uma responsabilidade está implementada.
- Por que determinado componente existe.
- Quais arquivos estão relacionados.
- Onde determinado comportamento acontece.
- Quais partes do projeto precisam ser compreendidas para executar uma tarefa.

**Visão de longo prazo:**

O C2 deverá permitir que qualquer agente de programação obtenha uma compreensão arquitetural útil de um projeto através de consultas estruturadas, sem precisar reconstruir esse conhecimento a cada interação.
