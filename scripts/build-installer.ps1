# Gera o instalador Windows do C2 com Inno Setup 6.
#
# Uso:
#   pwsh -File scripts/build-installer.ps1              # build-portable + ISCC
#   pwsh -File scripts/build-installer.ps1 -SkipBuild   # usa dist/desktop atual
#   pwsh -File scripts/build-installer.ps1 -Version 0.1.1
#
# Saida:
#   dist/installer/C2-Setup-<versao>.exe
#   dist/installer/C2-Setup-<versao>.exe.sha256

param(
    [switch]$SkipBuild,
    [string]$Version = ''
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$desktop = Join-Path $root 'dist\desktop'

if (-not $Version) {
    $package = Get-Content (Join-Path $root 'apps\desktop\package.json') -Raw | ConvertFrom-Json
    $Version = $package.version
}
if (-not $Version) { throw 'versao nao encontrada (apps/desktop/package.json)' }

if (-not $SkipBuild) {
    Write-Host "1/3 gerando pacote portatil ($Version)..."
    & (Join-Path $root 'scripts\build-portable.ps1')
    if ($LASTEXITCODE -ne 0) { throw 'falha no build-portable.ps1' }
} else {
    Write-Host '1/3 usando dist/desktop existente (-SkipBuild)'
}

# Requisitos do instalador
foreach ($obrigatorio in @('C2.exe', 'cli\c2.exe', 'cli\c2-mcp.cmd', 'runtime\llama-server.exe')) {
    $caminho = Join-Path $desktop $obrigatorio
    if (-not (Test-Path $caminho)) {
        throw "pacote incompleto: $caminho nao existe (rode sem -SkipBuild)"
    }
}

$iscc = Get-Command iscc, ISCC -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $iscc) {
    $candidatos = @(
        (Join-Path $env:LOCALAPPDATA 'Programs\Inno Setup 6\ISCC.exe'),
        (Join-Path $env:ProgramFiles 'Inno Setup 6\ISCC.exe'),
        (Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6\ISCC.exe')
    )
    foreach ($candidato in $candidatos) {
        if ($candidato -and (Test-Path $candidato)) { $iscc = Get-Item $candidato; break }
    }
}
if (-not $iscc) {
    throw 'Inno Setup 6 nao encontrado. Instale de https://jrsoftware.org/isdl.php e rode de novo.'
}
$isccPath = if ($iscc.Source) { $iscc.Source } else { $iscc.FullName }

Write-Host "2/3 compilando instalador com $isccPath ..."
$script = Join-Path $root 'scripts\c2.iss'
$argumentos = @("/DMyAppVersion=$Version", $script)
& $isccPath @argumentos
if ($LASTEXITCODE -ne 0) { throw 'falha ao compilar o instalador (ISCC)' }

$saida = Join-Path $root "dist\installer\C2-Setup-$Version.exe"
if (-not (Test-Path $saida)) { throw "instalador nao gerado: $saida" }

$hash = (Get-FileHash $saida -Algorithm SHA256).Hash.ToLower()
$arquivoHash = "$saida.sha256"
[System.IO.File]::WriteAllText($arquivoHash, "$hash  $(Split-Path -Leaf $saida)`r`n", (New-Object System.Text.UTF8Encoding($false)))
$tamanho = [math]::Round((Get-Item $saida).Length / 1MB, 1)
Write-Host ''
Write-Host "pronto: $saida ($tamanho MB)"
Write-Host "sha256: $arquivoHash"
