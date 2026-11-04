# Regras de leitura de código — C2

Cole este bloco no arquivo de regras do seu agente (CLAUDE.md, `.cursor/rules/`,
`.github/copilot-instructions.md` ou `AGENTS.md`). Versão completa e exemplos:
[agent-protocol.md](agent-protocol.md).

---

## Contexto antes de código

Este projeto é indexado pelo **C2**. Ao lado de cada arquivo existe um
`<arquivo>.c2.json` com metadados semânticos gerados por modelo local. Ele
responde "o que é, por que existe, qual o objetivo, qual a responsabilidade,
quais arquivos se relacionam" — em ~300 tokens, contra milhares do código.

**Regra de ouro:** não leia código para descobrir *onde* está. Use o índice para
decidir *se* vale ler o código.

## Ordem obrigatória de investigação

1. `c2 status` — engine ativo e índice atualizado? Se não: `c2 start`.
2. `c2 grep "<tarefa em linguagem natural>" --limit 5` — candidatos.
3. Leia o `.c2.json` de cada candidato **antes** do arquivo de código.
4. `c2 related <arquivo>` — vizinhos do grafo antes de abrir mais arquivos.
5. `c2 read <arquivo> --lines A:B` — só o trecho necessário.
6. Arquivo inteiro apenas como último recurso, com motivo.

## Regras

- **Metadado é hipótese; código é evidência.** Toda afirmação sobre
  comportamento deve vir do código. Se o metadado divergir, diga isso e rode
  `c2 reindex --path <arquivo>` — nunca invente nem edite `.c2.json` à mão.
- **Não leia o projeto inteiro.** Comece com `--limit 5`; 1 a 3 arquivos
  costumam resolver. Só expanda quando faltar informação concreta.
- **Não leia todos os `.c2.json`.** Use só os dos arquivos que o `grep` trouxe.
- `links[].type` diz a relação: `imports`, `includes`, `mod`, `require`, `extends`.
- Cite sempre `caminho/arquivo:linha` ao referenciar código.
- Sem `.c2.json` = projeto não indexado: avise e use leitura normal.
- **C2 ≠ bd.** C2 é contexto de código; `bd` é tarefa/projeto. Não misture.

## Fallback (CLI do C2 ainda não instalada)

```bash
Base/c2_context.sh <símbolo> [profundidade]   # devolve {root, depth, nodes, links}
```

Protótipo em bash: tolerante a schema, mas com bugs conhecidos (ver bead
`c2-e11t3`). Sem ele, leia normalmente e avise que o projeto não está indexado.
