# Comparacao Rust x bash no mesmo corpus (temp; apagar depois).
$ErrorActionPreference = 'Continue'
$root = 'H:\TUDO\Codigo\Codigo\C2 Desktop'
$base = Join-Path $env:TEMP 'c2-cmp'
Remove-Item $base -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $base | Out-Null

$origem = Join-Path $root 'examples\bench-ab\loja'
Copy-Item $origem (Join-Path $base 'rust') -Recurse -Force
Copy-Item $origem (Join-Path $base 'bash') -Recurse -Force
foreach ($copia in @('rust', 'bash')) {
    Remove-Item (Join-Path $base "$copia\target") -Recurse -Force -ErrorAction SilentlyContinue
}

$cfg = Get-Content (Join-Path $root '.c2\config.json') -Raw | ConvertFrom-Json
$modelo = $cfg.model.path
$server = Join-Path $root 'dist\desktop\runtime\llama-server.exe'

Write-Host '== subindo llama-server na porta 1234 =='
$proc = Start-Process -FilePath $server -ArgumentList @(
    '-m', "`"$modelo`"", '--host', '127.0.0.1', '--port', '1234',
    '-c', '8192', '-ngl', '99', '--no-webui'
) -PassThru -RedirectStandardOutput (Join-Path $base 'llama.out.log') -RedirectStandardError (Join-Path $base 'llama.err.log')

$ok = $false
for ($i = 0; $i -lt 120; $i++) {
    try {
        $r = Invoke-WebRequest -Uri 'http://127.0.0.1:1234/health' -UseBasicParsing -TimeoutSec 2
        if ($r.StatusCode -eq 200) { $ok = $true; break }
    } catch { Start-Sleep -Seconds 1 }
}
if (-not $ok) {
    Write-Host 'ERRO: runtime nao respondeu'
    Get-Content (Join-Path $base 'llama.err.log') -Tail 10
    exit 1
}
Write-Host 'runtime pronto'

$bash = 'D:\mingw-w64\Git\bin\bash.exe'
$wrapper = Join-Path $root 'scripts\tmp-run-bash.sh'
Write-Host '== rodando o prototipo bash =='
$sw = [Diagnostics.Stopwatch]::StartNew()
$saida = & $bash $wrapper 2>&1
$sw.Stop()
$texto = ($saida | Out-String)
Write-Host '--- ultimas linhas do prototipo ---'
$saida | Select-Object -Last 15
Write-Host '--- fim ---'
$processados = ([regex]::Matches($texto, 'Processando:')).Count
$okMetas = ([regex]::Matches($texto, 'OK ->')).Count
$invalidos = ([regex]::Matches($texto, 'JSON inv')).Count
Write-Host "bash: $processados processados, $okMetas metadados, $invalidos invalidos, $([math]::Round($sw.Elapsed.TotalSeconds,1)) s"

Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 2

[pscustomobject]@{
    corpus           = 'examples/bench-ab/loja'
    bash_seconds     = [math]::Round($sw.Elapsed.TotalSeconds, 1)
    bash_processados = $processados
    bash_metadados   = $okMetas
    bash_invalidos   = $invalidos
} | ConvertTo-Json | Set-Content (Join-Path $base 'bash-result.json') -Encoding utf8
Write-Host '== bash concluido =='
