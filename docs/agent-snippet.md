# Regras de leitura de código — C2

Cole este bloco no arquivo de regras do seu agente (CLAUDE.md, `.cursor/rules/`,
`.github/copilot-instructions.md` ou `AGENTS.md`). Versão completa e exemplos:
[agent-protocol.md](agent-protocol.md). Versao completa e autocontida (para colar nas regras): [agent-instructions.md](agent-instructions.md).

---

## Contexto antes de código

Existe um `<arquivo>.c2.json` ao lado de cada arquivo de código (mesmo
diretório, mesmo basename). Ele fornece o contexto semântico do código, gerado
por modelo local. Ele
responde "o que é, por que existe, qual o objetivo, qual a responsabilidade,
quais arquivos se relacionam" — em ~300 tokens, contra milhares do código.

**Regra de ouro:** não leia código para descobrir *onde* está. Use o índice para
decidir *se* vale ler o código.

## Ordem obrigatória de investigação

1. **Antes de ler um arquivo de código, leia o `<arquivo>.c2.json` ao lado**
   (mesmo diretório, mesmo basename). Essa é a regra mais importante do projeto.
2. `c2 status` — engine ativo e comentários atualizados? Se não: `c2 start`.
3. `c2 grep "<tarefa em linguagem natural>" --limit 5` — candidatos.
4. `c2 related <arquivo>` — vizinhos do grafo antes de abrir mais arquivos.
5. `c2 read <arquivo> --lines A:B` — só o trecho necessário.
6. Arquivo inteiro apenas como último recurso, com motivo.

## Regras

- **Pense teleologicamente: "por que isso existe?", "o que quebra se remover?",
"o que o código NÃO revela?". Só se a resposta não for derivável do código,
registre com a tool MCP `c2_annotate` (file, text, kind) — no máximo 1–2 por
tarefa, curto (max 2000 chars). Use `correction` se você pensou A e era B;
`pending` se depende do humano. Nunca anote o óbvio ("calcula descontos").

Metadado é hipótese; código é evidência.** Toda afirmação sobre
  comportamento deve vir do código. Se o metadado divergir, diga isso e rode
  `c2 reindex --path <arquivo>` — nunca invente nem edite `.c2.json` à mão.
- **Não leia o projeto inteiro.** Comece com `--limit 5`; 1 a 3 arquivos
  costumam resolver. Só expanda quando faltar informação concreta.
- **Não leia todos os `.c2.json`.** Use só os dos arquivos que o `grep` trouxe.
- `links[].type` diz a relação: `imports`, `includes`, `mod`, `require`, `extends`.
- Cite sempre `caminho/arquivo:linha` ao referenciar código.
- Sem `.c2.json` = projeto sem comentários do C2: avise e use leitura normal.
- **C2 ≠ bd.** C2 é contexto de código; `bd` é tarefa/projeto. Não misture.

## Fallback (CLI do C2 ainda não instalada)

```bash
Base/c2_context.sh <símbolo> [profundidade]   # devolve {root, depth, nodes, links}
```

Protótipo em bash: tolerante a schema, mas com bugs conhecidos (ver bead
`c2-e11t3`). Sem ele, leia normalmente e avise que o projeto não está indexado.
