# Protocolo de contexto para agentes — C2

**Versão:** 1.0 · **Status:** vigente · **Substitui:** `Base/instruções de uso.txt`
**Fonte:** este arquivo · **Versão curta para colar nas regras:** [agent-snippet.md](agent-snippet.md) · **Versão completa e autocontida:** [agent-instructions.md](agent-instructions.md)

---

## 1. Escopo

Vale para qualquer agente de programação (Claude Code, Cursor, Copilot, Codex,
agente próprio) trabalhando em um projeto **indexado pelo C2** — inclusive neste
repositório, que é indexado por ele mesmo.

O C2 mantém, ao lado de cada arquivo, um metadado `<arquivo>.c2.json` com a
semântica do arquivo. O protocolo define **quando e em que ordem** o agente usa
esse metadado, o grafo de links e a leitura de código.

**Regra de ouro:** o agente não lê código para descobrir onde algo está. Ele usa
o índice para decidir *se* vale ler o código — e lê o mínimo.

---

## 2. Ordem canônica (do mais barato ao mais caro)

| # | Passo | Comando | Custo típico | Critério para parar |
|---|---|---|---|---|
| 0 | Estado do índice | `c2 status` | ~100 tokens | engine ativo e `pending = 0` |
| 1 | Localizar | `c2 grep "<tarefa>" --limit 5` | ~300 tokens | 1–5 candidatos plausíveis |
| 2 | Triagem | ler `<arquivo>.c2.json` | ~300 tokens/arquivo | sabe qual arquivo é o dono da responsabilidade |
| 3 | Vizinhos | `c2 related <arquivo>` | ~200 tokens | entende o que depende e do que depende |
| 4 | Trecho | `c2 read <arquivo> --lines A:B` | ~200–800 tokens | tem o comportamento concreto |
| 5 | Arquivo completo | leitura normal | milhares | só com motivo explícito |

**Nunca inverta a ordem.** Ler o código antes do metadado transforma o índice em
enfeite; ler só o metadado transforma o agente em gerador de alucinação.

---

## 3. O metadado (`<arquivo>.c2.json`)

Schema v1 (bead `c2-dec1`):

```json
{
  "schema_version": 1,
  "file": "src/auth/session.rs",
  "hash": "sha256:9f2c...",
  "size": 4821,
  "mtime": 1790000000,
  "semantic": {
    "what": "Gerenciamento de sessão do usuário e renovação de token.",
    "why": "Centralizar ciclo de vida de sessão para que autorização não dependa de estado espalhado.",
    "goal": "Garantir que toda requisição autenticada tenha identidade válida e rastreável.",
    "summary": "Sessão e token: criação, renovação e invalidação.",
    "responsibility": "Dono da validade da sessão."
  },
  "links": [
    { "to": "src/auth/token.rs", "type": "imports" },
    { "to": "src/db/users.rs", "type": "imports" }
  ]
}
```

Como usar cada campo:

| Campo | Pergunta que responde | Uso na prática |
|---|---|---|
| `what` | Do que se trata? | decidir se é o arquivo certo antes de abrir |
| `why` | Por que existe? | entender a motivação e o que não pode ser quebrado |
| `goal` | Que objetivo garante? | avaliar se a tarefa pede mudança nele |
| `summary` | Responsabilidade em uma frase | citar em plano/resumo sem ler código |
| `responsibility` | O que é de sua propriedade | diferenciar de arquivos vizinhos parecidos |
| `links[]` | Quem se relaciona, e como | navegar o grafo em vez de varrer o repo |
| `hash`/`mtime`/`size` | O metadado está fresco? | detectar metadado velho antes de confiar |
| `schema_version` | Compatibilidade | `1` = schema atual |

Tipos de link: `imports` (Rust `use`/`mod`, JS `import`, Python `import`),
`includes` (C/C++), `require` (CommonJS), `extends` (herança de arquivo/base).

---

## 4. Frescor e confiança

1. **Metadado velho é mentira silenciosa.** O C2 reindexa por polling (mtime +
   tamanho a cada 5 s), mas janela de trabalho, branch trocada e arquivo em
   escrita podem deixar defasagem. `c2 status` mostra pendentes antes de você
   confiar.
2. **Evidência antes de inferência** (PRD §20.5). O metadado serve para
   *localizar e priorizar*. Toda afirmação sobre comportamento precisa de
   `arquivo:linha`.
3. **Divergência é dado, não erro do agente.** Se o código contradiz o
   metadado: relate a divergência, rode `c2 reindex --path <arquivo>` e siga pelo
   código. Se persistir após reindexação, é bug do C2 → abra um bead.
4. **Nunca edite `.c2.json` à mão.** O C2 sobrescreve na próxima indexação.
   Para forçar atualização, use `c2 reindex`.

---

## 5. Orçamento de tokens

- Comece com `--limit 5`. Só aumente com motivo declarado (ex.: "nenhum
  resultado acima de 0.4").
- **Metadado primeiro, sempre**: ~300 tokens contra milhares.
- Trecho com `--lines`, não arquivo inteiro.
- Não leia `.c2.json` de arquivos que o `grep` não trouxe — é ruído.
- Pare quando tiver o suficiente: 1–3 arquivos resolvem a maioria das tarefas.
- Se a tarefa for de arquitetura, use `c2 grep --explain` antes de abrir qualquer
  arquivo.

---

## 6. Fluxos por tipo de tarefa

**Localizar antes de corrigir bug**
`c2 grep "onde valida permissão" --limit 5` → metadados → `c2 related` no arquivo
mais relevante → `c2 read` do trecho da validação → só então editar.

**Entender impacto de uma mudança**
`c2 related <arquivo>` → para cada vizinho, ler só o metadado → decidir quais
precisam de leitura real.

**Onboarding em módulo desconhecido**
`c2 grep "<domínio>" --files-only --limit 10` → agrupar por pasta → ler metadados
dos 2–3 arquivos centrais → montar o mapa mental antes de qualquer código.

**Revisão de PR**
`c2 status` (índice fresco?) → diff → para cada arquivo alterado, metadado →
`c2 read` do trecho alterado → checar `links[]` para ver o que pode quebrar.

---

## 7. Fallback

| Situação | O que fazer |
|---|---|
| CLI do C2 não instalada | `Base/c2_context.sh <símbolo> [profundidade]` — protótipo em bash que devolve `{root, depth, nodes, links}` |
| Projeto sem `.c2.json` | avisar o usuário; sugerir `c2 init && c2 start`; seguir leitura normal |
| LLM local indisponível | C2 não indexa; leitura normal, sem tentar forçar |
| Servidor MCP (pós-MVP) | mesmas ferramentas via MCP; a ordem do protocolo não muda |

O protótipo bash é tolerante a schema, mas carrega defeitos conhecidos
(resposta vazia gravada, `set -e` derrubando o loop, hash pós-análise, raiz
duplicada na travessia) — todos rastreados em `c2-e11t3`. Não trate sua saída
como definitiva.

---

## 8. Anti-padrões

| Anti-padrão | Por que é ruim |
|---|---|
| Ler todos os `.c2.json` do repositório | caro e ruidoso; o grep já filtrou |
| Confiar no metadado contra o código | alucinação com aparência de evidência |
| Editar `.c2.json` à mão | será sobrescrito; corrompe o índice |
| Ler arquivo inteiro "para ter contexto" | derrota o propósito do C2 |
| Usar C2 para gerenciar tarefas | isso é `bd`; não misture os dois |
| Ignorar `hash`/`mtime` desatualizados | decisão baseada em metadado velho |
| Pedir "um resumo do projeto" ao modelo | use `c2 grep --files-only` e leia metadados |

---

## 9. Exemplo de sessão

```text
Tarefa: "o login está aceitando token expirado"

1. c2 status
   → engine: idle | indexados: 412 | pendentes: 0                (índice fresco)

2. c2 grep "validação de token e expiração" --limit 5
   → src/auth/token.rs      0.91
   → src/auth/session.rs    0.84
   → src/auth/middleware.rs 0.77

3. ler src/auth/token.rs.c2.json
   → what: "Validação e decodificação de JWT."
     goal: "Recusar token inválido, expirado ou de emissor desconhecido."
   → é o dono da regra de expiração.

4. c2 related src/auth/token.rs
   → session.rs (imports), middleware.rs (imports)

5. c2 read src/auth/token.rs --lines 40:95
   → encontra a checagem comparando `exp` em segundos com `now` em milissegundos.

Total: 3 metadados + 1 trecho. Nenhum arquivo lido por inteiro.
```

---

## 10. Manutenção

- Este documento é a fonte da verdade; o snippet é a versão curta para colar.
- Mudou o schema (`schema_version`)? Atualize a seção 3 e o snippet na mesma PR.
- Comandos citados pertencem aos beads `c2-e7` (CLI) e `c2-e8` (Semantic Grep);
  `c2 map` e as ferramentas MCP entram com `c2-e13` e `c2-e16`, sem alterar a
  ordem do protocolo.
- `Base/instruções de uso.txt` fica como registro histórico e não deve ser usado.
