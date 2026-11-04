# Corrige os caminhos de SDK que apontam para um drive J: inexistente
# (Set-ItemProperty, sem reg.exe) e instala o Windows SDK 10.0.19041 em E:.
# Rodar em PowerShell como Administrador.

$ErrorActionPreference = 'Continue'
$result = 'E:\vs-bootstrap\sdk-result2.txt'
"inicio $(Get-Date -Format o)" | Set-Content $result

function Log($message) {
    "$(Get-Date -Format 'HH:mm:ss') $message" | Add-Content $result
}

# Encerra o reg.exe que travou na tentativa anterior, se ainda estiver vivo.
Get-Process reg -ErrorAction SilentlyContinue | ForEach-Object {
    try { $_.Kill(); Log "kill reg pid $($_.Id)" } catch { Log "falha ao matar reg: $($_.Exception.Message)" }
}

$fixes = @(
    @{ Path = 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Microsoft SDKs\Windows\v10.0'; Name = 'InstallationFolder'; Value = 'E:\Windows Kits\10\' },
    @{ Path = 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows Kits\Installed Roots'; Name = 'KitsRoot10'; Value = 'E:\Windows Kits\10\' },
    @{ Path = 'HKLM:\SOFTWARE\Microsoft\Windows Kits\Installed Roots'; Name = 'KitsRoot10'; Value = 'E:\Windows Kits\10\' },
    @{ Path = 'HKLM:\SOFTWARE\Microsoft\Windows Kits\Installed Roots'; Name = 'WindowsDebuggersRoot10'; Value = 'E:\Windows Kits\10\Debuggers\' },
    @{ Path = 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows Kits\Installed Roots'; Name = 'WindowsDebuggersRoot10'; Value = 'E:\Windows Kits\10\Debuggers\' }
)

foreach ($fix in $fixes) {
    try {
        if (-not (Test-Path $fix.Path)) { New-Item -Path $fix.Path -Force | Out-Null }
        Set-ItemProperty -Path $fix.Path -Name $fix.Name -Value $fix.Value -Type String -Force -ErrorAction Stop
        Log "ok $($fix.Name) = $($fix.Value)"
    } catch {
        Log "ERRO em $($fix.Name): $($_.Exception.Message)"
    }
}

$setup = 'H:\VISUAL STUDIO\Download\Win10SDK_10.0.19041,version=10.0.19041.4\winsdksetup.exe'
Log 'instalando Windows SDK 10.0.19041 (DesktopCPPx64 + UCRT)...'
try {
    $process = Start-Process -FilePath $setup -ArgumentList @(
        '/quiet', '/norestart', '/ceip', 'off', '/features', 'OptionId.DesktopCPPx64', 'OptionId.UCRT'
    ) -Wait -PassThru -ErrorAction Stop
    Log "SDK_EXIT=$($process.ExitCode)"
} catch {
    Log "SDK_FALHOU: $($_.Exception.Message)"
}

Log 'fim'
