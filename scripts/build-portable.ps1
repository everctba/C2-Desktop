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
# O exe em execucao trava o arquivo: avisa em vez de copiar pela metade.
Get-Process C2, c2-desktop -ErrorAction SilentlyContinue |
    Where-Object { $_.Path -like '*dist\desktop*' } |
    ForEach-Object { throw "feche o C2.exe que esta rodando (pid $($_.Id)) antes de gerar o pacote" }

$destExe = Join-Path $out 'C2.exe'
Copy-Item $appExe $destExe -Force
# C2-console.exe e artefato de diagnostico, nao vai no pacote
Remove-Item (Join-Path $out 'C2-console.exe') -Force -ErrorAction SilentlyContinue
if (-not (Test-Path $destExe)) { throw 'falha ao copiar C2.exe para dist/desktop' }

# Windows e case-insensitive: a CLI vai para cli\c2.exe para nao sobrescrever C2.exe
$cliDir = Join-Path $out 'cli'
New-Item -ItemType Directory -Force -Path $cliDir | Out-Null
$coreExe = Join-Path $root "target\$buildProfile\c2.exe"
if (-not (Test-Path $coreExe)) {
    $coreExe = Join-Path $root "target\x86_64-pc-windows-msvc\$buildProfile\c2.exe"
}
if (Test-Path $coreExe) {
    Copy-Item $coreExe (Join-Path $cliDir 'c2.exe') -Force
    Copy-Item (Join-Path $root 'scripts\c2-mcp.cmd') (Join-Path $cliDir 'c2-mcp.cmd') -Force
} else {
    Write-Warning "CLI c2.exe nao encontrada em target\$buildProfile"
}

# runtime oficial embutido (build/runtime, baixado por fetch-runtime.ps1)
$runtimeSrc = Join-Path $root 'build\runtime'
$runtimeDst = Join-Path $out 'runtime'
if (Test-Path (Join-Path $runtimeSrc 'llama-server.exe')) {
    Write-Host '    incluindo runtime embarcado...'
    if (Test-Path $runtimeDst) { Remove-Item $runtimeDst -Recurse -Force }
    if (Test-Path $runtimeDst) {
        throw "nao foi possivel limpar $runtimeDst (runtime/llama-server em uso?)"
    }
    Copy-Item $runtimeSrc $runtimeDst -Recurse -Force
    # Blindagem: se destino existisse no momento do copy, o PowerShell cria
    # runtime\runtime; isso dobrava o pacote para ~186 MB.
    if (Test-Path (Join-Path $runtimeDst 'runtime')) {
        throw "copia aninhada em $runtimeDst\runtime; feche runtime/llama-server e gere de novo"
    }
    Copy-Item (Join-Path $root 'scripts\instalar.ps1') (Join-Path $out 'instalar.ps1') -Force
    Copy-Item (Join-Path $root 'scripts\desinstalar.ps1') (Join-Path $out 'desinstalar.ps1') -Force
} else {
    Write-Warning 'sem build/runtime: rode scripts/fetch-runtime.ps1 para embarcar o llama.cpp'
}

Write-Host '4/4 escrevendo LEIA-ME...'
$readme = @'
C2 - pacote portatil - Comments 2.0 for AI
============================================

O C2 comenta seu codigo automaticamente com IA local e mantem o comentario
atualizado (hash) para a sua IA gastar menos tokens entendendo o projeto.

O que tem aqui
--------------
C2.exe             app desktop (janela), com o frontend embutido
cli\c2.exe         CLI do Core (init, status, grep, related, read)
cli\c2-mcp.cmd     atalho para o servidor MCP (STDIO JSON-RPC)
runtime\            llama-server oficial do llama.cpp (Vulkan + CPU)
instalar.ps1       instalador com escolha de pasta
desinstalar.ps1    remove a instalacao (preserva os projetos)

Requisitos
----------
- Windows 10/11 com WebView2.
- Um modelo .gguf no seu disco (o C2 NAO copia o modelo).

Instalar
--------
    pwsh -File instalar.ps1                       (abre o seletor de pasta)
    pwsh -File instalar.ps1 -Dest "D:\Apps\C2"   (instala direto)
    pwsh -File instalar.ps1 -NoPath               (nao mexe no PATH)

O instalador coloca <dest>\cli no PATH do usuario: em um terminal novo,
"c2 --version" funciona de qualquer pasta. O desinstalador remove a entrada.
Sem instalar (modo portatil), basta abrir C2.exe desta pasta.

Primeiro uso
------------
1. Abra C2.exe (atalho criado pelo instalador ou direto da pasta).
2. "Abrir projeto" e escolha a pasta do projeto.
3. Se ainda nao existir .c2/config.json, clique em "Inicializar".
4. "Selecionar .gguf" e escolha o modelo (mostra tamanho, quantizacao e VRAM).
5. "Iniciar".

O runtime do llama.cpp ja vai junto: nao precisa de LM Studio nem de CUDA.
A CLI em cli\c2.exe usa o mesmo runtime (procura ..\runtime).

MCP (agentes)
-------------
O MCP e servido pela CLI:   c2 --root "<projeto>" mcp
Se o CLI estiver no PATH (instalador), o agente pode usar:
  command: "c2"
  args: ["--root", "<projeto>", "mcp"]
Atalho equivalente: cli\c2-mcp.cmd --root "<projeto>"

O indice fica em <projeto>/.c2 e os metadados em <arquivo>.c2.json ao lado de
cada arquivo.
'@
Set-Content -Path (Join-Path $out 'LEIA-ME.txt') -Value $readme -Encoding utf8

# checksums do artefato (verificacao de integridade do pacote)
$checksumFile = Join-Path $out 'CHECKSUMS.txt'
$linhas = @()
foreach ($relativo in @('C2.exe', 'cli\c2.exe', 'runtime\llama-server.exe', 'runtime\.version')) {
    $arquivo = Join-Path $out $relativo
    if (Test-Path $arquivo) {
        $hash = (Get-FileHash $arquivo -Algorithm SHA256).Hash.ToLower()
        $linhas += "$hash  $relativo"
    }
}
Set-Content -Path $checksumFile -Value $linhas -Encoding utf8
Write-Host "    checksums: $checksumFile"

Write-Host ''
Get-ChildItem $out | Select-Object Name, @{ n = 'MB'; e = { if ($_.PSIsContainer) { '' } else { [math]::Round($_.Length / 1MB, 1) } } } | Format-Table -AutoSize
Write-Host "tamanho total do pacote: $([math]::Round(((Get-ChildItem $out -Recurse -File | Measure-Object Length -Sum).Sum) / 1MB, 1)) MB"
Write-Host "pronto: $out"
