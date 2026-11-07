# Mostra o diagnostico de inicializacao do C2 (perf.log).
#
# Uso:
#   pwsh -File scripts/perf-report.ps1
#   pwsh -File scripts/perf-report.ps1 -Tail 200
#   pwsh -File scripts/perf-report.ps1 -Path "C:\outro\perf.log"

param(
    [string]$Path = (Join-Path $env:APPDATA 'dev.c2.desktop\perf.log'),
    [int]$Tail = 100
)

if (-not (Test-Path $Path)) {
    Write-Host "sem log ainda: $Path"
    Write-Host 'rode o C2 uma vez (as marcas de boot sao gravadas sempre).'
    exit 0
}

$linhas = Get-Content $Path -ErrorAction Stop | Select-Object -Last $Tail
$eventos = foreach ($linha in $linhas) {
    try { $linha | ConvertFrom-Json } catch { }
}

if (-not $eventos) {
    Write-Host "nenhum JSON valido em $Path"
    exit 0
}

Write-Host "perf.log: $Path"
Write-Host ("{0,10}  {1,-26} {2}" -f 'ms', 'evento', 'dados')
Write-Host ('-' * 90)
foreach ($evento in $eventos) {
    $dados = if ($evento.data) { $evento.data | ConvertTo-Json -Compress -Depth 5 } else { '' }
    $ms = if ($null -ne $evento.elapsed_ms) { $evento.elapsed_ms } else { '' }
    Write-Host ("{0,10}  {1,-26} {2}" -f $ms, $evento.event, $dados)
}

$boot = $eventos | Where-Object { $_.event -in @('js_boot_start', 'status_ok', 'render_ok', 'first_raf', 'workspace_status') }
if ($boot) {
    Write-Host ''
    Write-Host 'resumo do boot:'
    $boot | ForEach-Object {
        $extra = if ($null -ne $_.data.ms) { " · status $($_.data.ms) ms" } else { '' }
        Write-Host ("  {0,8} ms  {1}{2}" -f $_.elapsed_ms, $_.event, $extra)
    }
}
