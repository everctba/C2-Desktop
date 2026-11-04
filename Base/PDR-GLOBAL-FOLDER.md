PDR 1 — c2GLOBAL System (Project Semantic Brain)
📌 Nome

c2GLOBAL — Hierarchical Semantic Project Memory

🎯 Objetivo

Criar um sistema de memória semântica global que consolida todos os .c2.json individuais em uma visão arquitetural unificada do projeto, permitindo que LLMs entendam o sistema inteiro sem precisar reprocessar todos os arquivos.

🧠 Problema que resolve
LLM perde contexto em codebases grandes
.c2.json é nível local demais (arquivo por arquivo)
não existe visão de arquitetura global
repetição de análise sem memória consolidada
🏗️ Arquitetura
Camadas do sistema:
1. File Layer (local)
file → .c2.json

Contém:

why
when
goal
summary
hash
mtime
2. Folder Layer (intermediário)
folder → folder_summary.json

Responsável por:

agrupar arquivos da pasta
sintetizar propósito da pasta
identificar responsabilidades do módulo
mapear dependências internas
3. Global Layer
c2GLOBAL.json

Contém:

visão geral do sistema
arquitetura de alto nível
módulos principais
fluxos críticos
dependências entre pastas
propósito do sistema completo
🔄 Pipeline de geração
Ler todos .c2.json
Agrupar por pasta
Gerar folder_summary.json
Agregar todos os folders
Gerar c2GLOBAL.json
🧾 Estrutura do c2GLOBAL.json
{
  "project": "name",

  "what_system_is": "",
  "why_it_exists": "",
  "goal": "",

  "architecture": {
    "frontend": {
      "purpose": "",
      "summary": ""
    },
    "backend": {
      "purpose": "",
      "summary": ""
    }
  },

  "modules": [
    {
      "name": "",
      "role": "",
      "importance": "core | support | utility"
    }
  ],

  "critical_flows": [],
  "dependencies": [],
  "notes": ""
}
⚙️ Requisitos técnicos
Bash + jq
LM Studio (API OpenAI compatible)
execução incremental
evitar reprocessamento desnecessário
🚫 Não-objetivos
não substituir Git
não versionar código
não armazenar estado de runtime
não ser dependente de contexto da LLM
🧠 Critério de sucesso
LLM consegue entender o projeto inteiro só lendo c2GLOBAL.json
redução de contexto necessário em ≥80%
zero reprocessamento global desnecessário
🧠 PDR 2 — Folder Summarizer Script
📌 Nome

c2_folder_summarizer.sh — Semantic Aggregation Engine

🎯 Objetivo

Criar um script que percorre o projeto, agrupa arquivos .c2.json por pasta e gera um resumo semântico de cada módulo do sistema.

🧠 Problema que resolve
.c2.json isolado não representa arquitetura
não existe “visão de módulo”
LLM perde hierarquia do sistema
impossível entender responsabilidades por pasta
🏗️ Entrada
./**/*.c2.json
⚙️ Processo
Step 1 — scan
encontra todos .c2.json
Step 2 — grouping
agrupa por diretório pai

Ex:

backend/
frontend/
tools/
Step 3 — merge context

Para cada pasta:

lê todos .c2.json
extrai:
goals
why
summary
consolida visão única
Step 4 — LLM call

Prompt:

Você é um system architect.

Resuma o propósito desta pasta como um módulo do sistema.

Inclua:
- role do módulo
- responsabilidade
- relação com o sistema
- dependências prováveis

Saída: JSON puro
📤 Output
{
  "folder": "backend/",
  "role": "",
  "responsibility": "",
  "summary": "",
  "importance": "core | support | utility",
  "related_folders": []
}
💾 Output file
backend/.c2.folder.json
frontend/.c2.folder.json
tools/.c2.folder.json
🔁 Execução
./c2_folder_summarizer.sh
🚫 Regras
não processar .c2.json diretamente no global
não gerar loop infinito
não incluir node_modules, .git
ignorar arquivos ocultos
não duplicar pastas
🧠 Critério de sucesso
cada pasta tem um “papel claro”
sistema pode ser entendido em 1 nível acima dos arquivos
redução de complexidade cognitiva
🚀 visão final do sistema

Você está montando isso:

FILES → SEMANTIC FILE MEMORY
       ↓
FOLDERS → MODULE MEMORY
       ↓
GLOBAL → SYSTEM BRAIN
💡 observação importante (nível arquitetura real)

Esse design que você está criando é basicamente:

🧠 “Hierarchical Cognitive Code Graph”

ou em termos mais simples:

um “cérebro de código local com memória persisten

⚡ como isso funciona na prática (sem embeddings)

Você faz um ranking heurístico simples:

1. normalização da query
"onde sistema processa autenticação"

vira tokens:

["sistema", "processa", "autenticação"]


2. matching por campos
score básico:
goal      → maior peso (3x)
why       → médio (2x)
summary   → médio (2x)
file path → baixo (1x)