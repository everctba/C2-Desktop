# Harness dos tres criterios de adocao do C2 (benchmark de agente).
#
# Uso:
#   pwsh -File scripts/bench-agent.ps1 -AgentCommand "pwsh -File C:\meu-agente.ps1" -Mode c2
#   pwsh -File scripts/bench-agent.ps1 -AgentCommand "pwsh -File C:\meu-agente.ps1" -Mode baseline
#
# O agente recebe -Root <projeto> e -Prompt <TASK.md>; deve escrever
# <root>/answer.json com a classificacao dos quatro modulos.
# Rode uma vez em c2 e uma em baseline para comparar leituras e acertos.

param(
    [Parameter(Mandatory = $true)][string]$AgentCommand,
    [ValidateSet('c2', 'baseline')][string]$Mode = 'c2',
    [string]$OutFile = 'docs/bench-agent.json'
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$tmp = Join-Path $env:TEMP ("c2-agent-bench-" + [guid]::NewGuid().ToString('N'))
$src = Join-Path $tmp 'src'
New-Item -ItemType Directory -Force (Join-Path $src 'audio') | Out-Null
New-Item -ItemType Directory -Force (Join-Path $src 'ui') | Out-Null
New-Item -ItemType Directory -Force (Join-Path $src 'mcp') | Out-Null
New-Item -ItemType Directory -Force (Join-Path $src 'cli') | Out-Null

Push-Location $root
try {
    & cargo run -q -p c2-cli -- --root $tmp init --no-open *> $null

    '// audio: toca sons e faz fallback para silencio' | Set-Content (Join-Path $src 'audio/sound.rs') -Encoding utf8
    '// ui: monta CSS de botoes' | Set-Content (Join-Path $src 'ui/css.rs') -Encoding utf8
    '// protocolo JSON-RPC do servidor MCP' | Set-Content (Join-Path $src 'mcp/lib.rs') -Encoding utf8
    '// parsing de argumentos da CLI' | Set-Content (Join-Path $src 'cli/args.rs') -Encoding utf8

    $metas = @(
        @{ file = 'src/audio/sound.rs'; what = 'reproducao de audio e fallback'; why = 'tocar sons'; goal = 'audio nunca quebra'; summary = 'sound engine'; responsibility = 'audio' },
        @{ file = 'src/ui/css.rs'; what = 'gerador de CSS'; why = 'estilo dos botoes'; goal = 'UI consistente'; summary = 'css builder'; responsibility = 'ui' },
        @{ file = 'src/mcp/lib.rs'; what = 'servidor MCP JSON-RPC'; why = 'expor ferramentas para agentes'; goal = 'protocolo MCP'; summary = 'MCP server'; responsibility = 'mcp' },
        @{ file = 'src/cli/args.rs'; what = 'parser de argumentos da CLI'; why = 'comandos de terminal'; goal = 'CLI estavel'; summary = 'CLI parser'; responsibility = 'cli' }
    )
    foreach ($m in $metas) {
        $meta = [ordered]@{
            schema_version = 1
            file = $m.file
            hash = 'sha256:bench'
            size = 40
            mtime = 0
            semantic = [ordered]@{
                what = $m.what
                why = $m.why
                goal = $m.goal
                summary = $m.summary
                responsibility = $m.responsibility
            }
            links = @()
        }
        $meta | ConvertTo-Json -Depth 5 | Set-Content ((Join-Path $tmp $m.file) + '.c2.json') -Encoding utf8
    }

    @'
# Tarefa

Classifique os quatro arquivos abaixo em `answer.json`:

```json
{
  "src/audio/sound.rs": "audio",
  "src/ui/css.rs": "ui",
  "src/mcp/lib.rs": "mcp",
  "src/cli/args.rs": "cli"
}
```

Use os metadados `.c2.json`/C2 se disponiveis. Nao altere codigo.
'@ | Set-Content (Join-Path $tmp 'TASK.md') -Encoding utf8

    if ($Mode -eq 'baseline') {
        Get-ChildItem $tmp -Recurse -File -Filter '*.c2.json' | Remove-Item -Force
    }

    $env:C2_BENCH_MODE = $Mode
    $env:C2_ROOT = $tmp
    Invoke-Expression "$AgentCommand -Root '$tmp' -Prompt '$(Join-Path $tmp 'TASK.md')'"

    $answerPath = Join-Path $tmp 'answer.json'
    $answer = if (Test-Path $answerPath) { Get-Content $answerPath -Raw | ConvertFrom-Json } else { $null }
    $expected = [ordered]@{
        'src/audio/sound.rs' = 'audio'
        'src/ui/css.rs' = 'ui'
        'src/mcp/lib.rs' = 'mcp'
        'src/cli/args.rs' = 'cli'
    }
    $acertos = 0
    foreach ($chave in $expected.Keys) {
        if ($answer -and $answer.$chave -eq $expected[$chave]) { $acertos++ }
    }

    $statsRaw = (& cargo run -q -p c2-cli -- --root $tmp stats --json 2>$null) -join "`n"
    $stats = if ($statsRaw) { $statsRaw | ConvertFrom-Json } else { $null }

    $resultado = [ordered]@{
        mode = $Mode
        success = ($acertos -eq 4)
        correct = $acertos
        expected = 4
        c2_calls = if ($stats) { $stats.calls } else { 0 }
        code_reads = if ($stats) { $stats.code_reads } else { 0 }
        meta_reads = if ($stats) { $stats.meta_reads } else { 0 }
        preflight_before_code = if ($stats) { $stats.preflight_before_code } else { false }
        answer = $answer
    }
    $destino = Join-Path $root $OutFile
    New-Item -ItemType Directory -Force (Split-Path -Parent $destino) | Out-Null
    $resultado | ConvertTo-Json -Depth 5 | Set-Content $destino -Encoding utf8
    $resultado | ConvertTo-Json -Depth 5
} finally {
    Pop-Location
    Remove-Item Env:C2_BENCH_MODE -ErrorAction SilentlyContinue
    Remove-Item Env:C2_ROOT -ErrorAction SilentlyContinue
    Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
}
