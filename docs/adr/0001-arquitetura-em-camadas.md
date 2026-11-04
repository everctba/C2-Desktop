# ADR-001 — Arquitetura em camadas e fronteira do Core

**Status:** aceito · **Data:** 2026-11-04 · **Beads:** `c2-e1t4`, `c2-dec2`

## Contexto

O C2 tem quatro consumidores previstos: CLI, desktop (Tauri), servidor MCP e,
no futuro, outros agentes via biblioteca. Se cada um implementar descoberta,
hash, indexação ou busca, o comportamento diverge e o produto perde a
propriedade mais importante do PRD: **um núcleo, múltiplas interfaces**.

Ao mesmo tempo, o MVP precisa ser pequeno: o desktop completo e o MCP estão
deferidos (`c2-e16`, `c2-e13`), mas o desktop mínimo entra no MVP (`c2-e14`).

## Decisão

1. **`c2-core` é uma biblioteca pura.** Não lê argumentos de linha de comando,
   não imprime em stdout, não abre janela, não conhece o protocolo MCP.
   Expoe tipos e funções: `Workspace`, `Config`, `MetaFile`, `discover`,
   `cleaner`, `hash`.
2. **`c2-cli` é um binário fino.** Faz parsing (clap), formata saída (humana e
   `--json`) e traduz erros do Core em código de saída. Nenhuma regra de
   indexação vive aqui.
3. **Desktop e MCP consumem o mesmo Core.** Quando chegarem, são adaptadores:
   o desktop recebe eventos do engine; o MCP traduz chamadas de ferramenta para
   funções do Core. Nenhum dos dois reimplementa descoberta, hash ou ranking.
4. **Estado em disco é contrato público.** `.c2/config.json`, `.c2/state.json`
   e os `<arquivo>.c2.json` são versionados e documentados em
   [`docs/schema.md`](../schema.md).

## Consequências

**Positivas**

- Testar o Core não exige processo, porta ou UI: `cargo test -p c2-core`.
- A CLI pode ser reescrita ou o desktop adicionado sem tocar no índice.
- O MCP (pós-MVP) reaproveita busca e leitura — apenas serializa.

**Negativas / custos**

- O Core precisa definir tipos de evento e progresso cedo, senão a UI acaba
  lendo arquivos por conta própria.
- Operações longas exigem uma camada de orquestração (engine) que não é nem
  Core puro nem CLI; ela vive em `c2-core::engine` (a partir de `c2-e5`).

## Alternativas descartadas

| Alternativa | Por que não |
|---|---|
| Monolito no binário da CLI | Desktop e MCP duplicariam lógica ou dependeriam de `Command + parse de stdout` |
| Core dentro do app Tauri | Core vira dependente de UI; CLI e MCP passam a exigir WebView2 |
| Bibliotecas separadas por área (`c2-index`, `c2-search`) | Cedo demais: três crates para um MVP de um desenvolvedor |
| Serviço HTTP local como única API | Processo extra e protocolo a manter antes de existir a segunda interface |
