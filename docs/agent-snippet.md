# Regras de leitura de código — C2

Cole este bloco no arquivo de regras do seu agente (CLAUDE.md, `.cursor/rules/`,
`.github/copilot-instructions.md` ou `AGENTS.md`). Ele é o mesmo texto que o
`c2 init` imprime.

---

# C2 Usage Guide for Agents

Copie e cole este bloco no arquivo de regras do seu agente:
`AGENTS.md`, `CLAUDE.md`, `rules.md` (ou `.cursor/rules/` e
`.github/copilot-instructions.md`).

################################################################################
<EXTREMELY_IMPORTANT>

Este projeto usa C2. Cada arquivo de código tem um `<arquivo>.c2.json` ao lado
(mesmo diretório, mesmo basename) com o contexto semântico do arquivo:
what, why, goal, summary, responsibility e links[].

1. ANTES de ler um arquivo de código, leia o `<arquivo>.c2.json` ao lado.
   Essa é a regra mais importante do projeto: contexto antes de código.
2. `c2 status` — os comentários estão atualizados?
3. Use `c2 grep` para descobrir candidatos por significado e `c2 read` para
   ler só o trecho necessário.
4. Metadado é hipótese; código é evidência. Se o `.c2.json` divergir do código,
   o código vence — e o metadado precisa ser reindexado.
5. Nunca edite `.c2.json` manualmente. Use `c2_annotate` (MCP) ou `c2 reindex`.
6. Pense teleologicamente antes de qualquer anotação: por que este arquivo
   existe? O que quebra se eu remover isso? O que o código NÃO revela?
   Se a resposta não for óbvia, registre com a tool MCP `c2_annotate`
   (file, text, kind) — no máximo 1–2 por tarefa, curtas (até 2000 chars).
   Use `correction` se você pensou A e era B; `pending` se depende do humano.
   Nunca anote o óbvio (ex.: "este arquivo calcula descontos").
   Só anote o que não é derivável do código.

## Fluxo para implementar

ANTES:

1. `c2 status` — o índice está atualizado? Existe runtime/modelo configurado?
2. `c2 grep "<tarefa>" --limit 5` — candidatos por significado.
3. Leia o `.c2.json` de cada candidato.
4. `c2 related <arquivo>` — veja o que usa e o que é usado por ele.
5. `c2 read <arquivo> --lines A:B` — só o trecho relevante.

DURANTE:

- prefira os menores trechos possíveis;
- se precisar criar algo novo, procure primeiro o padrão existente via C2;
- não copie o metadado como se fosse verdade absoluta: confirme no código.

AO TERMINAR:

- valide o que o próprio projeto já oferece (testes, lint, typecheck, build),
  usando os comandos documentados no README/CI; não presuma que existem e não
  instale ferramentas só para cumprir isso;
- o engine, quando está rodando, reindexa sozinho os arquivos alterados a cada
  ciclo. Se estiver parado ou você precisar do metadado atualizado agora, rode
  `c2 reindex --path <arquivo>`;
- se o que mudou foi o prompt ou o modelo (e não o código), use
  `c2 reindex --full`: o hash não muda, mas o conteúdo gerado precisa ser
  regenerado.

O projeto traz `.mcp.json` e `.cursor/mcp.json` (gerados por `c2 init --mcp`):
se o seu cliente suportar MCP, use as tools `c2_grep`, `c2_read`,
`c2_related` e `c2_map` em vez de varrer o código com shell.

## Comandos úteis

```bash
# Visão geral do workspace, índice e runtime
c2 status
c2 status --json

# Busca semântica: descreva a responsabilidade, não o nome do arquivo
c2 grep "onde o sistema valida permissão de usuário" --limit 5
c2 grep "fila de reindexação" --limit 5 --json

# Dependências no grafo estático (usa / usado por)
c2 related src/pedido/servico.rs
c2 related src/pedido/servico.rs --json

# Ler só o trecho necessário
c2 read src/pedido/servico.rs --lines 120:180
c2 read src/pedido/servico.rs --lines 120:180 --json

# Mapa plano do projeto (uma linha por arquivo indexável)
c2 map

# Reindexar um arquivo específico ou o projeto inteiro
c2 reindex --path src/pedido/servico.rs
c2 reindex --full

# Servidor MCP do projeto (STDIO)
c2 mcp
```

Use `c2 <comando> --help` para as opções completas.

</EXTREMELY_IMPORTANT>
################################################################################

