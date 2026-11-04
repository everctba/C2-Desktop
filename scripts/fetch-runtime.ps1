# Baixa o runtime oficial do llama.cpp para build/runtime (cache de empacotamento).
#
# Uso:
#   pwsh -File scripts/fetch-runtime.ps1                     # ultimo build, Vulkan x64
#   pwsh -File scripts/fetch-runtime.ps1 -Flavor cpu         # CPU apenas
#   pwsh -File scripts/fetch-runtime.ps1 -Version b12110     # build fixo
#
# O build Vulkan (~36 MB) usa a GPU pelo driver (sem CUDA) e cai para CPU quando
# nao ha GPU Vulkan — e o padrao do pacote portatil.

param(
    [string]$Version = '',
    [string]$Flavor = 'vulkan'
)

$ErrorActionPreference = 'Continue'
$root = Split-Path -Parent $PSScriptRoot
$dest = Join-Path $root 'build\runtime'

if (-not $Version) {
    Write-Host 'consultando o ultimo build do llama.cpp...'
    $releases = Invoke-RestMethod -Uri 'https://api.github.com/repos/ggml-org/llama.cpp/releases?per_page=10' `
        -Headers @{ 'User-Agent' = 'c2-build' } -TimeoutSec 60
    $build = $releases | Where-Object { $_.tag_name -match '^b\d+$' } | Select-Object -First 1
    if (-not $build) { throw 'nao encontrei um build (tag b####) do llama.cpp' }
    $Version = $build.tag_name
} else {
    $build = Invoke-RestMethod -Uri "https://api.github.com/repos/ggml-org/llama.cpp/releases/tags/$Version" `
        -Headers @{ 'User-Agent' = 'c2-build' } -TimeoutSec 60
}

$asset = $build.assets | Where-Object { $_.name -match "-bin-win-$Flavor-x64\.zip$" } | Select-Object -First 1
if (-not $asset) { throw "asset win-$Flavor-x64 nao encontrado no build $Version" }

New-Item -ItemType Directory -Force -Path $dest | Out-Null
$zip = Join-Path $dest 'runtime.zip'
Write-Host "baixando $($asset.name) ($([math]::Round($asset.size / 1MB, 1)) MB)..."
Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $zip -UseBasicParsing -TimeoutSec 600

Write-Host 'extraindo...'
Get-ChildItem $dest -Exclude 'runtime.zip' | Remove-Item -Recurse -Force -ErrorAction SilentlyContinue
Expand-Archive -Path $zip -DestinationPath $dest -Force
Remove-Item $zip -Force

Set-Content -Path (Join-Path $dest '.version') -Value $Version -Encoding utf8

$server = Get-ChildItem $dest -Recurse -Filter 'llama-server.exe' | Select-Object -First 1
if (-not $server) { throw 'llama-server.exe nao encontrado no pacote baixado' }
$versao = & $server.FullName --version 2>&1 | Select-Object -First 1
Write-Host "runtime pronto: $($server.FullName)"
Write-Host "versao: $versao"
Write-Host "tamanho: $([math]::Round(((Get-ChildItem $dest -Recurse -File | Measure-Object Length -Sum).Sum) / 1MB, 1)) MB"
