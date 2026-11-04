# Desinstalador do C2 Desktop.
#
# Remove o programa instalado e o atalho. NAO apaga projetos nem os metadados
# .c2.json deles (isso e dado do usuario, fica no workspace de cada projeto).

param([switch]$Force)

$ErrorActionPreference = 'Continue'
$instalado = $PSScriptRoot

if (Get-Process C2 -ErrorAction SilentlyContinue) {
    if (-not $Force) { throw 'feche o C2 Desktop antes de desinstalar (ou use -Force)' }
    Get-Process C2 | Stop-Process -Force
    Start-Sleep -Seconds 2
}

if (-not $Force) {
    $resposta = Read-Host "Remover a instalacao em $instalado? [s/N]"
    if ($resposta -notmatch '^[sSyY]') {
        Write-Host 'cancelado'
        return
    }
}

$shell = New-Object -ComObject WScript.Shell
foreach ($atalho in @(
        (Join-Path ([Environment]::GetFolderPath('Desktop')) 'C2.lnk'),
        (Join-Path ([Environment]::GetFolderPath('Programs')) 'C2.lnk')
    )) {
    if (Test-Path $atalho) {
        Remove-Item $atalho -Force
        Write-Host "atalho removido: $atalho"
    }
}

Write-Host "removendo $instalado ..."
Set-Location $env:TEMP
Remove-Item $instalado -Recurse -Force
Write-Host 'desinstalado. Metadados .c2.json dos seus projetos foram preservados.'
