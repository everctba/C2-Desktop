# Gera o pacote portatil do C2 Desktop em dist/desktop.
#
# Uso:
#   pwsh -File scripts/build-portable.ps1            # release (recomendado)
#   pwsh -File scripts/build-portable.ps1 -Debug     # debug, mais rapido
#
# Saida:
#   dist/desktop/C2.exe       executavel portatil (frontend embutido)
#   dist/desktop/LEIA-ME.txt  requisitos e primeiros passos
#   dist/desktop/c2.exe       CLI do Core (para init/status/grep no terminal)

param([switch]$Debug)

$ErrorActionPreference = 'Continue'
$root = Split-Path -Parent $PSScriptRoot
$desktop = Join-Path $root 'apps\desktop'
$out = Join-Path $root 'dist\desktop'

Write-Host '1/4 compilando o Core (CLI)...'
$cargoArgs = @('build', '-p', 'c2-cli')
if (-not $Debug) { $cargoArgs += '--release' }
Push-Location $root
& cargo @cargoArgs
if ($LASTEXITCODE -ne 0) { throw 'falha ao compilar o Core' }
Pop-Location

Write-Host '2/4 compilando o desktop (frontend embutido)...'
Push-Location $desktop
$tauriArgs = @('tauri', 'build', '--no-bundle')
if ($Debug) { $tauriArgs += '--debug' }
& pnpm @tauriArgs
if ($LASTEXITCODE -ne 0) { throw 'falha ao compilar o desktop' }
Pop-Location

Write-Host '3/4 copiando para dist/desktop...'
New-Item -ItemType Directory -Force -Path $out | Out-Null
$buildProfile = if ($Debug) { 'debug' } else { 'release' }
$appExe = Join-Path $desktop "src-tauri\target\$buildProfile\c2-desktop.exe"
if (-not (Test-Path $appExe)) { throw "executavel nao encontrado: $appExe" }
Copy-Item $appExe (Join-Path $out 'C2.exe') -Force

# Windows e case-insensitive: a CLI vai para cli\c2.exe para nao sobrescrever C2.exe
$cliDir = Join-Path $out 'cli'
New-Item -ItemType Directory -Force -Path $cliDir | Out-Null
$coreExe = Join-Path $root "target\$buildProfile\c2.exe"
if (-not (Test-Path $coreExe)) {
    $coreExe = Join-Path $root "target\x86_64-pc-windows-msvc\$buildProfile\c2.exe"
}
if (Test-Path $coreExe) {
    Copy-Item $coreExe (Join-Path $cliDir 'c2.exe') -Force
} else {
    Write-Warning "CLI c2.exe nao encontrada em target\$buildProfile"
}

Write-Host '4/4 escrevendo LEIA-ME...'
$readme = @'
C2 - pacote portatil
====================

Requisitos
----------
- Windows 10/11 com WebView2 (ja vem no Windows 11; no 10 pode vir do Edge).
- Um modelo .gguf no seu disco (o C2 NAO copia o modelo) e um llama-server.
  Configure uma vez no projeto:

    cli\c2.exe config set model.path  "H:\modelos\modelo.gguf"
    cli\c2.exe config set model.binary "C:\runtime\llama-server.exe"   (opcional)

Primeiro uso
------------
1. Abra C2.exe.
2. "Abrir projeto" e escolha a pasta do projeto.
3. Se ainda nao existir .c2/config.json, clique em "Inicializar".
4. Configure o modelo (o caminho acima) e clique em "Iniciar".

O indice fica em <projeto>/.c2 e os metadados em <arquivo>.c2.json ao lado de
cada arquivo. A CLI em cli\c2.exe faz init/status/grep/related/read.

Observacao: este pacote ainda NAO embarca o llama.cpp. Enquanto isso, aponte
model.binary para um llama-server existente (o do LM Studio serve para
desenvolvimento).
'@
Set-Content -Path (Join-Path $out 'LEIA-ME.txt') -Value $readme -Encoding utf8

Write-Host ''
Get-ChildItem $out | Select-Object Name, @{ n = 'MB'; e = { [math]::Round($_.Length / 1MB, 1) } } | Format-Table -AutoSize
Write-Host "pronto: $out"
