# Instalador do C2 Desktop (pacote portatil).
#
# Uso (na pasta do pacote ou de qualquer lugar):
#   pwsh -File instalar.ps1
#   pwsh -File instalar.ps1 -Dest "D:\Apps\C2"
#   pwsh -File instalar.ps1 -NoShortcut
#   pwsh -File instalar.ps1 -NoGui        # sem dialogo: usa prompt de texto
#   pwsh -File instalar.ps1 -NoPath       # nao adiciona o CLI ao PATH
#   pwsh -File instalar.ps1 -PrependPath  # poe o CLI antes no PATH (ver conflito)
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
    [switch]$NoGui,
    # Nao adiciona <dest>\cli ao PATH do usuario
    [switch]$NoPath,
    # Coloca o CLI no inicio do PATH do usuario (pode sombrear outro "c2")
    [switch]$PrependPath
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

# CLI no PATH do usuario (nao precisa de admin); desinstalar remove depois.
$pastaCli = (Join-Path $Dest 'cli').TrimEnd('\')
$c2Proprio = Join-Path $pastaCli 'c2.exe'
if (Test-Path $c2Proprio) {
    # Outro "c2" no PATH? O Windows procura primeiro no PATH do sistema.
    $outros = @(Get-Command c2 -All -ErrorAction SilentlyContinue |
        Where-Object { $_.Source -and ($_.Source.TrimEnd('\') -ne $c2Proprio.TrimEnd('\')) })
    if ($outros.Count -gt 0) {
        $outro = $outros[0].Source
        Write-Warning "ja existe outro 'c2' no PATH: $outro"
        Write-Warning "  o Windows procura primeiro no PATH do SISTEMA; se o outro estiver la, ele vence mesmo com o C2 no PATH do usuario."
        Write-Warning "  para usar o C2: `"$c2Proprio`" --version, ou remova/renomeie o outro, ou use -PrependPath (so ajuda se o outro estiver no PATH do usuario)."
    }

    if (-not $NoPath) {
        $atual = [Environment]::GetEnvironmentVariable('Path', 'User')
        if ($null -eq $atual) { $atual = '' }
        $entradas = @($atual -split ';' | ForEach-Object { $_.Trim().TrimEnd('\') } | Where-Object { $_ -ne '' })
        if ($entradas -contains $pastaCli) {
            Write-Host "PATH: $pastaCli ja estava no PATH do usuario"
        } else {
            $novo = if ($atual.Trim().TrimEnd(';') -eq '') {
                $pastaCli
            } elseif ($PrependPath) {
                $pastaCli + ';' + $atual.TrimEnd(';')
            } else {
                $atual.TrimEnd(';') + ';' + $pastaCli
            }
            try {
                [Environment]::SetEnvironmentVariable('Path', $novo, 'User')
                try {
                    Add-Type -Namespace C2Inst -Name Win32 -MemberDefinition '[DllImport("user32.dll", SetLastError=true, CharSet=CharSet.Auto)] public static extern IntPtr SendMessageTimeout(IntPtr hWnd, uint Msg, UIntPtr wParam, string lParam, uint fuFlags, uint uTimeout, out UIntPtr lpdwResult);'
                    $resultado = [UIntPtr]::Zero
                    [C2Inst.Win32]::SendMessageTimeout([IntPtr]0xffff, 0x001A, [UIntPtr]::Zero, 'Environment', 0x0002, 1000, [ref]$resultado) | Out-Null
                } catch { }
                if ($PrependPath) {
                    Write-Host "PATH: $pastaCli adicionado no INICIO do PATH do usuario"
                } else {
                    Write-Host "PATH: $pastaCli adicionado ao PATH do usuario (vale em novos terminais)"
                }
            } catch {
                Write-Warning "nao foi possivel alterar o PATH do usuario: $($_.Exception.Message)"
            }
        }
    }
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
Write-Host "CLI: $Dest\cli\c2.exe (no PATH do usuario: abra um terminal novo e rode 'c2 --version')"
Write-Host ''
Write-Host "para remover: pwsh -File `"$Dest\desinstalar.ps1`""
