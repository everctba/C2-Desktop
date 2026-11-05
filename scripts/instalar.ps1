# Instalador do C2 Desktop (pacote portatil).
#
# Uso (na pasta do pacote ou de qualquer lugar):
#   pwsh -File instalar.ps1
#   pwsh -File instalar.ps1 -Dest "D:\Apps\C2"
#   pwsh -File instalar.ps1 -NoShortcut
#   pwsh -File instalar.ps1 -NoGui        # sem dialogo: usa prompt de texto
#   pwsh -File instalar.ps1 -NoPath       # nao adiciona o CLI ao PATH
#   pwsh -File instalar.ps1 -PrependPath  # poe o CLI antes no PATH (ver conflito)
#   pwsh -File instalar.ps1 -Lang en      # en (padrao) | pt-BR (sem dialogos graficos)
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
    [switch]$PrependPath,
    # Idioma: en (padrao) ou pt-BR; define o box inicial e o language.json
    [ValidateSet('en', 'pt-BR')]
    [string]$Lang = 'en'
)

$ErrorActionPreference = 'Continue'
$source = $PSScriptRoot

if (-not (Test-Path (Join-Path $source 'C2.exe'))) {
    throw "instalar.ps1 precisa estar na pasta do pacote (com C2.exe). Pasta atual: $source"
}
if (-not (Test-Path (Join-Path $source 'runtime\llama-server.exe'))) {
    Write-Warning $T.runtimeWarn
}

# ---------- idioma ----------
function MostrarSeletorIdioma {
    try {
        Add-Type -AssemblyName System.Windows.Forms
        $form = New-Object System.Windows.Forms.Form
        $form.Text = 'C2 — Choose language / Escolha o idioma'
        $form.Size = New-Object System.Drawing.Size(380, 190)
        $form.StartPosition = 'CenterScreen'
        $form.FormBorderStyle = 'FixedDialog'
        $form.MaximizeBox = $false
        $form.MinimizeBox = $false

        $label = New-Object System.Windows.Forms.Label
        $label.Text = 'Install language / Idioma da instalacao:'
        $label.Location = New-Object System.Drawing.Point(18, 18)
        $label.AutoSize = $true
        $form.Controls.Add($label)

        $radioEn = New-Object System.Windows.Forms.RadioButton
        $radioEn.Text = 'English (default)'
        $radioEn.Location = New-Object System.Drawing.Point(22, 48)
        $radioEn.AutoSize = $true
        $radioEn.Checked = $true
        $form.Controls.Add($radioEn)

        $radioPt = New-Object System.Windows.Forms.RadioButton
        $radioPt.Text = 'Português (Brasil)'
        $radioPt.Location = New-Object System.Drawing.Point(22, 76)
        $radioPt.AutoSize = $true
        $form.Controls.Add($radioPt)

        $ok = New-Object System.Windows.Forms.Button
        $ok.Text = 'OK'
        $ok.Location = New-Object System.Drawing.Point(270, 112)
        $ok.DialogResult = [System.Windows.Forms.DialogResult]::OK
        $form.Controls.Add($ok)
        $form.AcceptButton = $ok

        if ($form.ShowDialog() -ne [System.Windows.Forms.DialogResult]::OK) {
            return $null
        }
        if ($radioPt.Checked) { return 'pt-BR' }
        return 'en'
    } catch {
        Write-Warning "sem seletor grafico de idioma ($($_.Exception.Message)); usando -Lang $Lang"
        return $Lang
    }
}

$idioma = $Lang
if (-not $PSBoundParameters.ContainsKey('Lang') -and -not $NoGui) {
    $escolhido = MostrarSeletorIdioma
    if ($null -eq $escolhido) {
        Write-Host 'instalacao cancelada'
        exit 0
    }
    $idioma = $escolhido
}

$M = @{
    en = @{
        chooseDest = 'Choose the folder where C2 will be installed'
        cancelled = 'installation cancelled'
        origin = 'source'
        dest = 'target'
        closeApp = 'close C2 Desktop before installing'
        copying = 'copying files...'
        pathAlready = 'PATH: {0} was already in the user PATH'
        pathAdded = 'PATH: {0} added to the user PATH (new terminals)'
        pathPrepend = 'PATH: {0} added to the BEGINNING of the user PATH'
        pathFail = 'could not change the user PATH: {0}'
        shortcut = 'shortcut: {0}'
        installed = 'installed to: {0}'
        next = 'next steps:'
        next1 = '  1. open {0}\C2.exe'
        next2 = '  2. Open project -> choose the project folder'
        next3 = '  3. Select .gguf (the model is not copied)'
        next4 = '  4. Start'
        cli = "CLI: {0}\cli\c2.exe (in the user PATH: open a new terminal and run 'c2 --version')"
        remove = 'to remove: pwsh -File "{0}"'
        runtimeWarn = 'runtime\llama-server.exe not found: C2 will need model.binary or llama-server in PATH'
        conflict = "another 'c2' already exists in PATH: {0}"
        conflict2 = '  Windows checks the SYSTEM PATH first; if the other one is there it wins.'
        conflict3 = '  use the full path "{0} --version", or rename/remove the other one.'
        language = 'installation language: {0}'
    }
    pt = @{
        chooseDest = 'Escolha a pasta onde o C2 sera instalado'
        cancelled = 'instalacao cancelada'
        origin = 'origem'
        dest = 'destino'
        closeApp = 'feche o C2 Desktop antes de instalar'
        copying = 'copiando arquivos...'
        pathAlready = 'PATH: {0} ja estava no PATH do usuario'
        pathAdded = 'PATH: {0} adicionado ao PATH do usuario (vale em novos terminais)'
        pathPrepend = 'PATH: {0} adicionado no INICIO do PATH do usuario'
        pathFail = 'nao foi possivel alterar o PATH do usuario: {0}'
        shortcut = 'atalho: {0}'
        installed = 'instalado em: {0}'
        next = 'proximos passos:'
        next1 = '  1. abra {0}\C2.exe'
        next2 = '  2. Abrir projeto -> escolha a pasta do projeto'
        next3 = '  3. Selecionar .gguf (o modelo nao e copiado)'
        next4 = '  4. Iniciar'
        cli = "CLI: {0}\cli\c2.exe (no PATH do usuario: abra um terminal novo e rode 'c2 --version')"
        remove = 'para remover: pwsh -File "{0}"'
        runtimeWarn = 'runtime\llama-server.exe nao encontrado no pacote: o C2 vai exigir model.binary ou llama-server no PATH'
        conflict = "ja existe outro 'c2' no PATH: {0}"
        conflict2 = '  o Windows procura primeiro no PATH do SISTEMA; se o outro estiver la, ele vence.'
        conflict3 = '  para usar o C2: "{0} --version", ou remova/renomeie o outro.'
        language = 'idioma da instalacao: {0}'
    }
}
$T = $M[$(if ($idioma -eq 'pt-BR') { 'pt' } else { 'en' })]
Write-Host ($T.language -f $idioma)

$padrao = Join-Path $env:LOCALAPPDATA 'C2\'

if (-not $Dest -and -not $NoGui) {
    # Seletor grafico de pasta (escolhe qualquer disco/pasta)
    try {
        Add-Type -AssemblyName System.Windows.Forms
        $dialogo = New-Object System.Windows.Forms.FolderBrowserDialog
        $dialogo.Description = $T.chooseDest
        $dialogo.UseDescriptionForTitle = $true
        $dialogo.ShowNewFolderButton = $true
        $dialogo.SelectedPath = $padrao
        if ($dialogo.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
            $Dest = $dialogo.SelectedPath
        } else {
            Write-Host $T.cancelled
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
Write-Host "$($T.origin) : $source"
Write-Host "$($T.dest): $Dest"
Write-Host ''

if (Get-Process C2 -ErrorAction SilentlyContinue) {
    throw $T.closeApp
}

New-Item -ItemType Directory -Force -Path $Dest | Out-Null

Write-Host $T.copying
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
            Write-Host ($T.pathAlready -f $pastaCli)
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
                    Write-Host ($T.pathPrepend -f $pastaCli)
                } else {
                    Write-Host ($T.pathAdded -f $pastaCli)
                }
            } catch {
                Write-Warning ($T.pathFail -f $_.Exception.Message)
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
            Write-Host ($T.shortcut -f $atalho)
        } catch {
            Write-Warning "nao foi possivel criar $atalho : $($_.Exception.Message)"
        }
    }
}

Write-Host ''
Write-Host ($T.installed -f $Dest)
Write-Host $T.next
Write-Host ($T.next1 -f $Dest)
Write-Host $T.next2
Write-Host $T.next3
Write-Host $T.next4
Write-Host ''
Write-Host ($T.cli -f $Dest)
Write-Host ''
Write-Host ($T.remove -f (Join-Path $Dest 'desinstalar.ps1'))

# Preferencia de idioma para o app ler no primeiro start (i18n c2-gj4).
$appData = Join-Path $env:APPDATA 'dev.c2.desktop'
New-Item -ItemType Directory -Force -Path $appData | Out-Null
$jsonIdioma = '{ "lang": "' + $idioma + '" }'
[System.IO.File]::WriteAllText((Join-Path $appData 'language.json'), $jsonIdioma, (New-Object System.Text.UTF8Encoding($false)))
