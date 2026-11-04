# Benchmark do C2: corpus sintetico, indexacao real e medicao da busca.
#
# Uso:
#   pwsh -File scripts/bench.ps1                  # 100 arquivos (padrao)
#   pwsh -File scripts/bench.ps1 -Files 500       # corpus grande (opcional)
#   pwsh -File scripts/bench.ps1 -Files 100
#
# Mede: tempo do primeiro indice, chamadas ao modelo, tempo da segunda passada
# (deve ser ~0 chamadas), latencia do grep e do related. Grava JSON no fim.
#
# Requisitos: runtime do llama-server configurado (model.binary) ou no PATH.

param(
    [int]$Files = 100,
    [string]$Out = '',
    # CLI a usar; por padrao prefere o pacote portatil (que tem o runtime embarcado)
    [string]$C2 = ''
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$c2 = $C2
if (-not $c2) {
    $portable = Join-Path $root 'dist\desktop\cli\c2.exe'
    $release = Join-Path $root 'target\release\c2.exe'
    $debug = Join-Path $root 'target\debug\c2.exe'
    if (Test-Path $portable) { $c2 = $portable }
    elseif (Test-Path $release) { $c2 = $release }
    elseif (Test-Path $debug) { $c2 = $debug }
}
if (-not $c2 -or -not (Test-Path $c2)) { throw 'compile o C2 (ou gere dist/desktop) antes do benchmark' }

$work = Join-Path $env:TEMP "c2-bench-$Files"
if (Test-Path $work) { Remove-Item $work -Recurse -Force }
New-Item -ItemType Directory -Path $work -Force | Out-Null

Write-Host "gerando corpus de $Files arquivos em $work..."
$domains = @(
    'autenticacao e sessao', 'estoque e reserva', 'faturamento', 'conciliacao bancaria',
    'permissoes e papeis', 'relatorios gerenciais', 'integracao com transporte',
    'precificacao e descontos', 'auditoria de operacoes', 'notificacoes ao cliente'
)
$folders = 20
$perFolder = [int][math]::Ceiling($Files / $folders)
$made = 0
for ($folder = 0; $folder -lt $folders -and $made -lt $Files; $folder++) {
    $dir = Join-Path $work ("src\mod{0:D2}" -f $folder)
    New-Item -ItemType Directory -Path $dir -Force | Out-Null
    for ($i = 0; $i -lt $perFolder -and $made -lt $Files; $i++) {
        $made++
        $domain = $domains[($folder + $i) % $domains.Count]
        $next = [int](($i + 1) % $perFolder)
        $content = @(
            "//! Modulo $made`: $domain"
            "//!"
            "//! Responsavel por $domain dentro do fluxo de $($domains[$folder % $domains.Count])."
            ""
            ("use crate::mod{0:D2}::arquivo_{1:D3};" -f $folder, $next)
            ""
            "pub struct Servico$made {"
            "    limite: usize,"
            "}"
            ""
            "impl Servico$made {"
            "    pub fn validar(&self, valor: usize) -> bool {"
            "        valor <= self.limite"
            "    }"
            ""
            "    pub fn processar(&self, valor: usize) -> Result<usize, String> {"
            "        if !self.validar(valor) {"
            "            return Err(format!(`"valor {valor} acima do limite de $domain`"));"
            "        }"
            "        Ok(valor * 2)"
            "    }"
            "}"
            ""
            "#[cfg(test)]"
            "mod tests {"
            "    use super::*;"
            ""
            "    #[test]"
            "    fn processa_valor_valido() {"
            "        let servico = Servico$made { limite: 10 };"
            "        assert_eq!(servico.processar(4).unwrap(), 8);"
            "    }"
            "}"
        ) -join "`n"
        Set-Content -Path (Join-Path $dir ("arquivo_{0:D3}.rs" -f $i)) -Value $content -Encoding utf8
    }
}

# src/lib.rs: raiz do crate, para o extrator resolver `crate::modNN::...`
$libLines = @('//! Crate raiz do corpus de benchmark.')
$libLines += (0..($folders - 1) | ForEach-Object { "pub mod mod{0:D2};" -f $_ })
Set-Content -Path (Join-Path $work 'src\lib.rs') -Value ($libLines -join "`n") -Encoding utf8
& $c2 --root $work init | Out-Null

# Usa a mesma configuracao de modelo do projeto (modelo, binario, DLLs, porta).
$projectConfig = Join-Path $root '.c2\config.json'
if (Test-Path $projectConfig) {
    Copy-Item $projectConfig (Join-Path $work '.c2\config.json') -Force
} else {
    Write-Warning 'sem .c2/config.json no projeto: configure o modelo a mao no corpus'
}

Write-Host 'primeiro indice (sobe runtime, indexa, para)...'
$first = [Diagnostics.Stopwatch]::StartNew()
$log1 = & $c2 --root $work reindex 2>&1
$first.Stop()
$summary = ($log1 | Select-Object -Last 3) -join ' | '

Write-Host 'segunda passada (nao deve chamar o modelo)...'
$second = [Diagnostics.Stopwatch]::StartNew()
$log2 = & $c2 --root $work reindex 2>&1
$second.Stop()
$summary2 = ($log2 | Select-Object -Last 2) -join ' | '

$queries = @(
    'onde valida o limite de estoque',
    'processamento de faturamento',
    'conciliacao bancaria e auditoria',
    'permissoes de acesso do usuario',
    'relatorio gerencial'
)
$greps = @()
foreach ($q in $queries) {
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $hits = & $c2 --root $work grep $q --limit 5 --json | ConvertFrom-Json
    $sw.Stop()
    $greps += [pscustomobject]@{
        query = $q
        ms    = [int]$sw.Elapsed.TotalMilliseconds
        hits  = @($hits).Count
    }
}

$relatedSw = [Diagnostics.Stopwatch]::StartNew()
$related = & $c2 --root $work related 'src/mod00/arquivo_001.rs' --json | ConvertFrom-Json
$relatedSw.Stop()

$result = [pscustomobject]@{
    files               = $Files
    binary              = $c2
    first_index_seconds = [math]::Round($first.Elapsed.TotalSeconds, 1)
    first_index_summary = $summary
    second_pass_seconds = [math]::Round($second.Elapsed.TotalSeconds, 1)
    second_pass_summary = $summary2
    grep                = $greps
    grep_p50_ms         = ($greps | Sort-Object ms | Select-Object -Index ([math]::Floor($greps.Count / 2))).ms
    grep_p95_ms         = ($greps | Sort-Object ms | Select-Object -Last 1).ms
    related_ms          = [int]$relatedSw.Elapsed.TotalMilliseconds
    related_uses        = @($related.uses).Count
    related_used_by     = @($related.used_by).Count
}

$json = $result | ConvertTo-Json -Depth 5
if ($Out) { Set-Content -Path $Out -Value $json -Encoding utf8 }
Write-Host ''
$json
