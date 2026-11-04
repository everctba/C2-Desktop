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

# Remove o CLI do PATH do usuario, se estiver la.
$pastaCli = (Join-Path $instalado 'cli').TrimEnd('\')
$atual = [Environment]::GetEnvironmentVariable('Path', 'User')
if ($atual) {
    $entradas = @($atual -split ';' | Where-Object { $_.Trim() -ne '' })
    $restantes = @($entradas | Where-Object { $_.TrimEnd('\') -ne $pastaCli })
    if ($restantes.Count -ne $entradas.Count) {
        [Environment]::SetEnvironmentVariable('Path', ($restantes -join ';'), 'User')
        Write-Host "PATH: entrada removida ($pastaCli)"
        try {
            Add-Type -Namespace C2Inst -Name Win32B -MemberDefinition '[DllImport("user32.dll", SetLastError=true, CharSet=CharSet.Auto)] public static extern IntPtr SendMessageTimeout(IntPtr hWnd, uint Msg, UIntPtr wParam, string lParam, uint fuFlags, uint uTimeout, out UIntPtr lpdwResult);'
            $resultado = [UIntPtr]::Zero
            [C2Inst.Win32B]::SendMessageTimeout([IntPtr]0xffff, 0x001A, [UIntPtr]::Zero, 'Environment', 0x0002, 1000, [ref]$resultado) | Out-Null
        } catch { }
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
