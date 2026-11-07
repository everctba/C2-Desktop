# Gera a variante do C2 que abre um console (release) para ver erros.
#
# Uso:
#   pwsh -File scripts/build-console.ps1
#   pwsh -File scripts/build-console.ps1 -Run
#
# Saida:
#   dist/desktop/C2-console.exe
#
# O console recebe erros do Rust/panic e erros do frontend encaminhados por
# frontend_log. Para o console JS completo, use o DevTools/CDP.

param(
    [switch]$Run
)

$ErrorActionPreference = 'Continue'
$root = Split-Path -Parent $PSScriptRoot
$desktop = Join-Path $root 'apps\desktop'
$srcTauri = Join-Path $desktop 'src-tauri'

Write-Host '1/3 compilando release com feature console (Tauri CLI)...'
Push-Location $desktop
# Usa o Tauri CLI (e nao cargo puro): ele define TAURI_ENV_DEBUG=false e embute
# o frontend; cargo puro compila com cfg(dev) e abre http://127.0.0.1:1420.
& pnpm tauri build --no-bundle --features console
if ($LASTEXITCODE -ne 0) { Pop-Location; throw 'falha no build console' }
Pop-Location

$origem = Join-Path $srcTauri 'target\release\c2-desktop.exe'
if (-not (Test-Path $origem)) { throw "executavel nao encontrado: $origem" }
$destino = Join-Path $root 'dist\desktop\C2-console.exe'
New-Item -ItemType Directory -Force -Path (Split-Path $destino) | Out-Null
Copy-Item $origem $destino -Force
Write-Host "2/3 pronto: $destino"

if ($Run) {
    Write-Host 'abrindo C2-console.exe (feche a janela para encerrar)...'
    & $destino
}
