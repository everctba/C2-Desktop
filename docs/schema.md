# Schema v1 do metadado C2

**Versão:** 1 · **Bead:** `c2-dec1`, `c2-e1t1` · **Implementação:** `crates/c2-core/src/schema.rs`

---

## 1. Onde fica

Ao lado de cada arquivo indexado, com o sufixo `.c2.json`:

```text
src/auth/session.rs
src/auth/session.rs.c2.json
```

O nome do símbolo no grafo é o caminho do arquivo, e o leitor tolerante
(`c2_context.sh`, protótipo) também aceita o basename sem o sufixo.

## 2. Formato

```json
{
  "schema_version": 1,
  "file": "src/auth/session.rs",
  "hash": "sha256:9f2c...",
  "size": 4821,
  "mtime": 1790000000,
  "semantic": {
    "what": "Gerenciamento de sessão do usuário e renovação de token.",
    "why": "Centralizar o ciclo de vida de sessão para que autorização não dependa de estado espalhado.",
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

| Campo | Tipo | Obrigatório | Descrição |
|---|---|---|---|
| `schema_version` | inteiro | sim | versão do formato; hoje `1` |
| `file` | string | sim | caminho relativo à raiz do workspace, sempre com `/` |
| `hash` | string | sim | `sha256:<hex>` do conteúdo analisado |
| `size` | inteiro | sim | tamanho em bytes do conteúdo analisado |
| `mtime` | inteiro | sim | mtime em segundos desde a epoch |
| `semantic.what` | string | sim* | do que se trata o arquivo |
| `semantic.why` | string | sim* | por que ele existe |
| `semantic.goal` | string | sim* | que objetivo ele garante |
| `semantic.summary` | string | sim* | responsabilidade em uma frase |
| `semantic.responsibility` | string | sim* | o que é de sua propriedade |
| `links[].to` | string | — | caminho destino, relativo à raiz |
| `links[].type` | string | — | `imports`, `includes`, `mod`, `require`, `extends` |

\* Pelo menos um campo semântico precisa estar preenchido: metadado sem
semântica é recusado na escrita (`MetaFile::validate`). Esse é o bug do
protótipo bash que gravava `.c2.json` vazio quando o modelo devolvia nada.

## 3. Regras de escrita

1. **Atômica**: grava em `.c2-tmp-<pid>-<nanos>` no mesmo diretório, `fsync`,
   depois `rename`. Interrupção no meio não deixa JSON truncado.
2. **Validada**: sem semântica ou sem hash → erro, nada é gravado.
3. **Re-hash pós-análise**: o `hash` gravado é o do conteúdo *no momento da
   análise*. Se o arquivo mudou durante a chamada do modelo, o metadado é
   descartado e o arquivo é reenfileirado (`c2-e3t5`), em vez de gravar hash
   novo com semântica velha.

## 4. Regras de leitura (tolerância)

O leitor aceita três gerações do formato, sem falhar:

| Origem | Como aparece | Comportamento |
|---|---|---|
| v1 (atual) | `semantic{}` + `links[]` + `schema_version` | normal |
| protótipo bash | `semantic{why,when,goal,summary}`, sem `what`/`links` | `when` é ignorado; `what` vira `""` |
| legado solto | `why`/`goal`/`summary` na raiz | mesmos campos, lidos da raiz |

`dependencies` e `uses` também são aceitos como sinônimos de `links`.
Aliases: `reasoning` → `why`; `path`/`file` dentro de um link → `to`.

## 5. Frescor

`is_fresh(hash, size)` compara os dois campos com o conteúdo atual. Metadado
que não bate é **stale** e deve ser reindexado — nunca apresentado como verdade.

## 6. Migração

O comando `c2 migrate` (`c2-e11t2`) reescreve metadados legados no schema v1,
com backup. Até ele existir, a leitura tolerante cobre o período de transição.

## 7. Evolução

Campos novos entram apenas com incremento de `schema_version` e regra explícita
de leitura para a versão anterior. Campos planejados: `concepts`, `inputs`,
`outputs`, `interfaces`, `constraints`, `decisions`.
