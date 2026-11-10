# Benchmark deterministico da floresta (c2-jgf.8).
# Cria um projeto sintetico com inconsistencias plantadas, roda o forest e mede
# se as divergencias/memorias foram expostas.

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$tmp = Join-Path $env:TEMP ("c2-forest-bench-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force (Join-Path $tmp 'src/pagamento') | Out-Null
New-Item -ItemType Directory -Force (Join-Path $tmp 'src/pedido') | Out-Null
New-Item -ItemType Directory -Force (Join-Path $tmp 'docs') | Out-Null
New-Item -ItemType Directory -Force (Join-Path $tmp 'crates/c2-core/src') | Out-Null

Push-Location $root
try {
    & cargo run -q -p c2-cli -- --root $tmp init --no-open *> $null

    @'
# Projeto de benchmark

Este projeto NAO usa embeddings (afirmacao desatualizada de proposito).
O fluxo principal esta em `src/nao_existe.rs`.
'@ | Set-Content (Join-Path $tmp 'README.md') -Encoding utf8

    '// codigo real' | Set-Content (Join-Path $tmp 'src/pagamento/servico.rs') -Encoding utf8
    '// codigo real' | Set-Content (Join-Path $tmp 'src/pedido/status.rs') -Encoding utf8
    '// existe para contradizer o README' | Set-Content (Join-Path $tmp 'crates/c2-core/src/embedding.rs') -Encoding utf8

    $metaPagamento = @'
{
  "schema_version": 1,
  "file": "src/pagamento/servico.rs",
  "hash": "sha256:pagamento",
  "size": 14,
  "mtime": 0,
  "semantic": {
    "what": "valida o pagamento",
    "why": "evita cobranca indevida",
    "goal": "so avancar apos confirmacao",
    "summary": "consulta o provedor",
    "responsibility": "retry e confirmacao"
  },
  "links": [],
  "agent_notes": [{
    "by": "agent",
    "model": "deepseek",
    "task": "fix-retry",
    "at": "2026-11-10T15:00:00Z",
    "hash": "sha256:pagamento",
    "kind": "gotcha",
    "text": "O retry nao pode ser removido: o provedor devolve 202 antes de confirmar."
  }]
}
'@
    $metaPagamento | Set-Content (Join-Path $tmp 'src/pagamento/servico.rs.c2.json') -Encoding utf8

    $metaPedido = @'
{
  "schema_version": 1,
  "file": "src/pedido/status.rs",
  "hash": "sha256:pedido",
  "size": 12,
  "mtime": 0,
  "semantic": {
    "what": "mantem o pedido",
    "why": "controla o fluxo",
    "goal": "status correto",
    "summary": "estado do pedido",
    "responsibility": "status"
  },
  "links": [],
  "agent_notes": [{
    "by": "agent",
    "task": "status",
    "at": "2026-11-10T15:01:00Z",
    "hash": "sha256:pedido",
    "kind": "pending",
    "text": "Validar se o status pode voltar para aguardando."
  }]
}
'@
    $metaPedido | Set-Content (Join-Path $tmp 'src/pedido/status.rs.c2.json') -Encoding utf8

    & cargo run -q -p c2-cli -- --root $tmp forest build *> $null

    $forest = Get-Content (Join-Path $tmp '.c2/forest.json') -Raw | ConvertFrom-Json
    $brief = (& cargo run -q -p c2-cli -- --root $tmp forest brief --task 'retry pagamento' 2>&1) -join "`n"

    $divergencias = @($forest.divergences)
    $temCaminho = [bool]($divergencias | Where-Object { $_.claim -like '*nao_existe*' } | Select-Object -First 1)
    $temClaimEmbedding = [bool]($divergencias | Where-Object { $_.claim -like '*embeddings*' } | Select-Object -First 1)
    $resultado = [pscustomobject]@{
        generated_at = (Get-Date).ToString('o')
        domains = @($forest.domains).Count
        decisions = @($forest.decisions).Count
        risks = @($forest.risks).Count
        divergences = $divergencias.Count
        has_missing_path = $temCaminho
        has_embeddings_claim = $temClaimEmbedding
        brief_chars = $brief.Length
        brief_has_gotcha = $brief -like '*retry*'
        divergence_claims = @($divergencias | ForEach-Object { $_.claim + ' | ' + $_.docs_ref })
        files = @(Get-ChildItem (Join-Path $tmp 'src') -Recurse -File | Where-Object { $_.Name -like '*.rs' }).Count
        ok = ($temCaminho -and $temClaimEmbedding -and ($brief -like '*retry*'))
    }
    $resultado | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $root 'docs/bench-forest.json') -Encoding utf8
    $resultado | Format-List
    if (-not $resultado.ok) { throw 'benchmark forest falhou nas expectativas' }
} finally {
    Pop-Location
    Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
}
