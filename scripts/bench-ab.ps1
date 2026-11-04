# Prepara o benchmark A/B (C2 versus leitura direta).
#
# Uso:
#   pwsh -File scripts/bench-ab.ps1                 # prepara as duas condicoes
#   pwsh -File scripts/bench-ab.ps1 -SkipIndex      # nao indexa (so copia)
#   pwsh -File scripts/bench-ab.ps1 -Out resultado.json
#
# O script NAO roda o agente: ele monta as duas pastas limpas, indexa a
# condicao A (se houver modelo configurado) e imprime o prompt base + as 5
# tarefas para voce colar no harness/modelo.

param(
    [switch]$SkipIndex,
    [string]$Out = 'bench-ab-resultado.json'
)

$ErrorActionPreference = 'Continue'
$root = Split-Path -Parent $PSScriptRoot
$origem = Join-Path $root 'examples\bench-ab\loja'
if (-not (Test-Path (Join-Path $origem 'Cargo.toml'))) {
    throw "projeto de exemplo nao encontrado em $origem"
}

$base = Join-Path $env:TEMP 'c2-bench-ab'
Remove-Item $base -Recurse -Force -ErrorAction SilentlyContinue
$comC2 = Join-Path $base 'cond-A-com-c2'
$semC2 = Join-Path $base 'cond-B-sem-c2'

Write-Host '== copiando projeto de exemplo =='
Copy-Item $origem $comC2 -Recurse -Force
Copy-Item $origem $semC2 -Recurse -Force
foreach ($copia in @($comC2, $semC2)) {
    Remove-Item (Join-Path $copia 'target') -Recurse -Force -ErrorAction SilentlyContinue
}

$c2 = Join-Path $root 'dist\desktop\cli\c2.exe'
if (-not (Test-Path $c2)) { $c2 = Join-Path $root 'target\release\c2.exe' }
if (-not (Test-Path $c2)) { $c2 = Join-Path $root 'target\debug\c2.exe' }

if (-not $SkipIndex) {
    if (Test-Path $c2) {
        Write-Host '== indexando a condicao A (pode levar alguns minutos) =='
        & $c2 --root $comC2 init | Out-Null
        $configProjeto = Join-Path $root '.c2\config.json'
        if (Test-Path $configProjeto) {
            Copy-Item $configProjeto (Join-Path $comC2 '.c2\config.json') -Force
        }
        & $c2 --root $comC2 reindex
        & $c2 --root $comC2 status | Select-Object -Last 6
    } else {
        Write-Warning 'CLI nao encontrada: prepare o indice manualmente (c2 init + c2 reindex)'
    }
}

$tarefas = @(
    'T1. Em que ordem o preco final e calculado e onde esta cada etapa? Cite arquivo:linha.',
    'T2. Pedido de 1 x R$ 500,00 com cupom BEMVINDO10: qual o total final? Mostre a conta.',
    'T3. Por que um pedido de R$ 10,00 com FRETE20 nao fica negativo?',
    'T4. O imposto usa arredondamento bancario? Justifique com o codigo.',
    'T5. Adicione um aviso (sem mudar o valor) quando o cupom for desconhecido. Quais arquivos tocar e como testar?'
)

Write-Host ''
Write-Host '== condicoes prontas =='
Write-Host "A (com C2)   : $comC2"
Write-Host "B (sem C2)   : $semC2"
Write-Host ''
Write-Host '== prompt base (igual nas duas condicoes) =='
Write-Host @'
Voce e um engenheiro analisando o projeto na pasta indicada. Responda cada
tarefa citando arquivo:linha. Nao invente: se nao houver evidencia no codigo,
diga que nao encontrou.
'@
Write-Host ''
Write-Host '== tarefas =='
$tarefas | ForEach-Object { Write-Host "  $_" }
Write-Host ''
Write-Host '== ferramentas por condicao =='
Write-Host '  A: c2 status | c2 grep | cat *.c2.json | c2 related | c2 read'
Write-Host '  B: leitura direta dos arquivos (sem c2 e sem .c2.json)'
Write-Host ''
Write-Host "modelo de resultado: $Out"

$modelo = [pscustomobject]@{
    protocolo = 'docs/bench-ab/protocolo.md'
    rubrica   = 'docs/bench-ab/rubrica.md'
    projeto   = 'examples/bench-ab/loja'
    condicoes = @{
        A = $comC2
        B = $semC2
    }
    tarefas = $tarefas
    rodadas = @()
    metricas_por_rodada = [pscustomobject]@{
        condicao           = 'A'
        tarefa             = 'T1'
        acertos_rubrica    = 2
        tokens_entrada     = 0
        tokens_saida       = 0
        arquivos_abertos   = 0
        chamadas_ferramenta = 0
        segundos           = 0
        erro_sem_suporte   = $false
        observacao         = ''
    }
}

$json = $modelo | ConvertTo-Json -Depth 6
Set-Content -Path $Out -Value $json -Encoding utf8
Write-Host "template salvo em $Out (preencha uma rodada por linha em .rodadas)"
