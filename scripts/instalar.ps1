# Instalador do C2 Desktop (pacote portatil).
#
# Uso (na pasta do pacote ou de qualquer lugar):
#   pwsh -File instalar.ps1
#   pwsh -File instalar.ps1 -Dest "D:\Apps\C2"
#   pwsh -File instalar.ps1 -NoShortcut
#   pwsh -File instalar.ps1 -NoGui        # sem dialogo: usa prompt de texto
#
# O que faz:
#   1. escolhe a pasta de instalacao (padrao %LOCALAPPDATA%\C2);
#   2. copia C2.exe, cli\, runtime\, LEIA-ME, este instalador e o desinstalador;
#   3. cria atalho no Menu Iniciar e na Area de Trabalho (opcional);
#   4. nao toca em nenhum projeto nem em .c2/config.json de workspace.

param(
    [string]$Dest = '',
    [switch]$NoShortcut,
    # Nao abre o seletor grafico de pasta (usa prompt de texto)
    [switch]$NoGui
)

$ErrorActionPreference = 'Continue'
$source = $PSScriptRoot

if (-not (Test-Path (Join-Path $source 'C2.exe'))) {
    throw "instalar.ps1 precisa estar na pasta do pacote (com C2.exe). Pasta atual: $source"
}
if (-not (Test-Path (Join-Path $source 'runtime\llama-server.exe'))) {
    Write-Warning 'runtime\llama-server.exe nao encontrado no pacote: o C2 vai exigir model.binary ou llama-server no PATH'
}

$padrao = Join-Path $env:LOCALAPPDATA 'C2'

if (-not $Dest -and -not $NoGui) {
    # Seletor grafico de pasta (escolhe qualquer disco/pasta)
    try {
        Add-Type -AssemblyName System.Windows.Forms
        $dialogo = New-Object System.Windows.Forms.FolderBrowserDialog
        $dialogo.Description = 'Escolha a pasta onde o C2 sera instalado'
        $dialogo.UseDescriptionForTitle = $true
        $dialogo.ShowNewFolderButton = $true
        $dialogo.SelectedPath = $padrao
        if ($dialogo.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
            $Dest = $dialogo.SelectedPath
        } else {
            Write-Host 'instalacao cancelada'
            exit 0
        }
    } catch {
        Write-Warning "nao foi possivel abrir o seletor grafico ($($_.Exception.Message)); usando prompt de texto"
        $entrada = Read-Host "Pasta de instalacao [$padrao]"
        if ([string]::IsNullOrWhiteSpace($entrada)) { $Dest = $padrao } else { $Dest = $entrada }
    }
}

if (-not $Dest) {
    $entrada = Read-Host "Pasta de instalacao [$padrao]"
    if ([string]::IsNullOrWhiteSpace($entrada)) { $Dest = $padrao } else { $Dest = $entrada }
}
$Dest = [System.IO.Path]::GetFullPath($Dest)

Write-Host ''
Write-Host "origem : $source"
Write-Host "destino: $Dest"
Write-Host ''

if (Get-Process C2 -ErrorAction SilentlyContinue) {
    throw 'feche o C2 Desktop antes de instalar'
}

New-Item -ItemType Directory -Force -Path $Dest | Out-Null

Write-Host 'copiando arquivos...'
foreach ($item in @('C2.exe', 'LEIA-ME.txt')) {
    $origem = Join-Path $source $item
    if (Test-Path $origem) { Copy-Item $origem (Join-Path $Dest $item) -Force }
}
foreach ($pasta in @('cli', 'runtime')) {
    $origem = Join-Path $source $pasta
    if (Test-Path $origem) {
        Copy-Item $origem (Join-Path $Dest $pasta) -Recurse -Force
    }
}
Copy-Item (Join-Path $source 'instalar.ps1') (Join-Path $Dest 'instalar.ps1') -Force
$desinstalador = Join-Path $source 'desinstalar.ps1'
if (Test-Path $desinstalador) {
    Copy-Item $desinstalador (Join-Path $Dest 'desinstalar.ps1') -Force
}

if (-not $NoShortcut) {
    $shell = New-Object -ComObject WScript.Shell
    $alvos = @(
        (Join-Path ([Environment]::GetFolderPath('Desktop')) 'C2.lnk'),
        (Join-Path ([Environment]::GetFolderPath('Programs')) 'C2.lnk')
    )
    foreach ($atalho in $alvos) {
        try {
            $link = $shell.CreateShortcut($atalho)
            $link.TargetPath = Join-Path $Dest 'C2.exe'
            $link.WorkingDirectory = $Dest
            $link.IconLocation = Join-Path $Dest 'C2.exe'
            $link.Description = 'C2 — contexto semantico de projetos'
            $link.Save()
            Write-Host "atalho: $atalho"
        } catch {
            Write-Warning "nao foi possivel criar $atalho : $($_.Exception.Message)"
        }
    }
}

Write-Host ''
Write-Host "instalado em: $Dest"
Write-Host 'proximos passos:'
Write-Host "  1. abra $Dest\C2.exe"
Write-Host '  2. Abrir projeto -> escolha a pasta do projeto'
Write-Host '  3. Selecionar .gguf (o modelo nao e copiado)'
Write-Host '  4. Iniciar'
Write-Host ''
Write-Host "para remover: pwsh -File `"$Dest\desinstalar.ps1`""
