# A/B — raciocínio ligado vs desligado (2026-11-05)

**Pergunta:** ligar o raciocínio (`enable_thinking`) melhora o entendimento
semântico do arquivo o suficiente para justificar a lentidão e as respostas
maiores?

**Método:** mesmo corpus de 6 arquivos reais, mesmo modelo
(`gemma-4-E2B-it-Q4_K_M`, GPU), mesma config — só muda
`model.disable_thinking` (A = `true`, B = `false`). Uma passada em cada
workspace temporário; métricas do `c2 reindex --json`.

## Números

| Métrica | A — desligado | B — ligado | B/A |
|---|---:|---:|---:|
| Tokens de resposta (completion) | **635** | **3.648** | **5,7×** |
| Latência somada (LLM) | **17,7 s** | **55,3 s** | **3,1×** |
| Por arquivo (LLM) | ~3,5 s | ~11,1 s | 3,1× |
| Prompt tokens | 12.759 | 12.794 | ≈1,00× |
| Chamadas | 5 | 5 | — |
| Erros | 0 | 0 | — |
| Tamanho do metadado (JSON) | ~10% menor | ~10% maior | 1,1× |
| Tempo total do comando | 75,8 s | 82,7 s | 1,09× |

O tempo total é dominado por subir o runtime e carregar o modelo
(~55–60 s); a diferença real está na inferência (3,1×).

## Qualidade (leitura lado a lado)

| Arquivo | A (desligado) | B (ligado) | Veredito |
|---|---|---|---|
| `Mcp.ts` | `what: "renderMcp"` (nome de função, não semântica) | "Renderiza a interface do painel MCP…" | **B melhor** |
| `mcp.rs` | "Servidor MCP mínimo sobre STDIO" (correto) | "MCP (**Minimum Control Protocol**)" — sigla errada | **A melhor** |
| `pricing.rs` | "Funções para cálculo de desconto por volume" | "…descontos progressivos por volume" | empate |
| `process.rs` | "executar comandos de console sem abrir janelas" | idem, mais formal | empate |
| `search.rs` | "busca híbrida (lexical + metadados)" | "busca híbrida (lexical e semântica)" | empate |

B tende a escrever descrições mais longas e "explicadas"; A é mais direto e,
em código com acrônimos do domínio, **B alucinou a expansão de MCP**. Nenhum
dos dois estourou o teto de tokens nem gerou JSON inválido.

**Resultado:** não houve ganho consistente de qualidade que justifique 3× o
tempo e 5,7× os tokens por arquivo. O modo raciocínio ajuda em código ambíguo
(UI), mas atrapalha quando o modelo "inventa" contexto (siglas).

## Recomendação

- **Padrão: desligado** (como está) para indexação em lote.
- **Ligado sob demanda** quando o arquivo for complexo/ambíguo e valer esperar
  ~11 s.
- O switch na tela Projetos grava `model.disable_thinking` e traz um tooltip com
  esses números.
- Um refinamento futuro: ligar raciocínio automaticamente só para arquivos
  acima de X KB/linhas (custo concentrado onde importa).

## Limitações

- 6 arquivos, uma rodada; diferenças de redação são julgamento qualitativo.
- `security.rs` não entrou: o próprio detector bloqueou (o arquivo contém
  assinaturas de segredo como literais).
- Sem medição de "acerto em tarefa real" (localizar/entender), só qualidade do
  metadado.
