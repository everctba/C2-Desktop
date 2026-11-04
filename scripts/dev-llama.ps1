# Sobe o llama-server para desenvolvimento.
#
# Enquanto o C2 nao empacota o proprio runtime (c2-e4t3), usa o llama.cpp que
# ja vem com o LM Studio e o modelo apontado em .c2/config.json (model.path).
# O modelo NAO e copiado: e lido no caminho configurado.
#
# Uso:
#   pwsh -File scripts/dev-llama.ps1                 # porta e modelo do config
#   pwsh -File scripts/dev-llama.ps1 -Port 8099
#   pwsh -File scripts/dev-llama.ps1 -Stop
#
# Requisitos: LM Studio instalado (traz o backend CUDA) e um GGUF no disco.

param(
    [int]$Port = 0,
    [string]$Model = '',
    [string]$Backend = '',
    [switch]$Stop
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot

if ($Stop) {
    Get-Process llama-server -ErrorAction SilentlyContinue | Stop-Process -Force
    Write-Host 'llama-server parado.'
    return
}

$extensions = Join-Path $env:USERPROFILE '.lmstudio\extensions\backends'
if (-not (Test-Path $extensions)) {
    throw "LM Studio nao encontrado em $extensions. Instale o runtime do C2 (c2-e4t3)."
}

# Backend: o informado ou o mais recente com suporte a CUDA 12.
if (-not $Backend) {
    $Backend = Get-ChildItem $extensions -Directory |
        Where-Object { $_.Name -like 'llama.cpp-*cuda12*' } |
        Sort-Object Name |
        Select-Object -Last 1 -ExpandProperty Name
}
if (-not $Backend) { throw 'Nenhum backend llama.cpp com CUDA 12 encontrado.' }

$backendDir = Join-Path $extensions $Backend
$vendorDir = Join-Path $extensions 'vendor\win-llama-cuda12-vendor-v2'
if (-not (Test-Path (Join-Path $backendDir 'llama-server.exe'))) {
    throw "llama-server.exe nao encontrado em $backendDir"
}

# Configuracao efetiva do workspace.
$configPath = Join-Path $root '.c2\config.json'
$config = if (Test-Path $configPath) { Get-Content $configPath -Raw | ConvertFrom-Json } else { $null }
if (-not $Port) { $Port = if ($config) { $config.model.port } else { 8080 } }
if (-not $Model) { $Model = if ($config) { $config.model.path } else { '' } }
if (-not $Model -or -not (Test-Path $Model)) {
    throw "Modelo GGUF nao encontrado. Defina em .c2/config.json: c2 config set model.path <caminho.gguf>"
}

$logs = Join-Path $root '.c2\logs'
New-Item -ItemType Directory -Force -Path $logs | Out-Null

# O llama-server do LM Studio carrega as DLLs CUDA do diretorio de vendor via PATH.
$env:PATH = "$backendDir;$vendorDir;" + $env:PATH

Write-Host "backend : $backendDir"
Write-Host "modelo  : $Model"
Write-Host "porta   : $Port"

$process = Start-Process -FilePath (Join-Path $backendDir 'llama-server.exe') `
    -ArgumentList @('-m', $Model, '--host', '127.0.0.1', '--port', "$Port",
                    '-c', "$($config.model.ctx_size)", '-ngl', '99', '--no-webui') `
    -RedirectStandardOutput (Join-Path $logs 'llama.out.log') `
    -RedirectStandardError (Join-Path $logs 'llama.err.log') `
    -WindowStyle Hidden -PassThru

for ($i = 0; $i -lt 120; $i++) {
    Start-Sleep -Milliseconds 1000
    try {
        if ((Invoke-WebRequest "http://127.0.0.1:$Port/health" -TimeoutSec 2 -UseBasicParsing).StatusCode -eq 200) {
            Write-Host "pronto (pid $($process.Id)) apos ~$($i + 1)s: http://127.0.0.1:$Port"
            return
        }
    } catch { }
}

$tail = Get-Content (Join-Path $logs 'llama.err.log') -Tail 20 -ErrorAction SilentlyContinue
throw "llama-server nao respondeu em 120s. Log: $logs`n$tail"
