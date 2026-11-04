# Corrige os caminhos de SDK que apontam para um drive J: inexistente e
# instala o Windows SDK 10.0.19041 em E:. Rodar como Administrador.

$ErrorActionPreference = 'Continue'
$result = 'E:\vs-bootstrap\sdk-result.txt'
"inicio $(Get-Date -Format o)" | Set-Content $result

$keys = @(
  @{ Path = 'HKLM\SOFTWARE\WOW6432Node\Microsoft\Microsoft SDKs\Windows\v10.0'; Name = 'InstallationFolder'; Value = 'E:\Windows Kits\10\' },
  @{ Path = 'HKLM\SOFTWARE\WOW6432Node\Microsoft\Windows Kits\Installed Roots'; Name = 'KitsRoot10'; Value = 'E:\Windows Kits\10\' },
  @{ Path = 'HKLM\SOFTWARE\Microsoft\Windows Kits\Installed Roots'; Name = 'KitsRoot10'; Value = 'E:\Windows Kits\10\' },
  @{ Path = 'HKLM\SOFTWARE\Microsoft\Windows Kits\Installed Roots'; Name = 'WindowsDebuggersRoot10'; Value = 'E:\Windows Kits\10\Debuggers\' },
  @{ Path = 'HKLM\SOFTWARE\WOW6432Node\Microsoft\Windows Kits\Installed Roots'; Name = 'WindowsDebuggersRoot10'; Value = 'E:\Windows Kits\10\Debuggers\' }
)

foreach ($key in $keys) {
  & reg.exe add $key.Path /v $key.Name /t REG_SZ /d $key.Value /f | Out-Null
  "reg $($key.Path) $($key.Name) -> exit $LASTEXITCODE" | Add-Content $result
}

$setup = 'H:\VISUAL STUDIO\Download\Win10SDK_10.0.19041,version=10.0.19041.4\winsdksetup.exe'
"instalando SDK..." | Add-Content $result
$p = Start-Process -FilePath $setup -ArgumentList @('/quiet', '/norestart', '/ceip', 'off', '/features', 'OptionId.DesktopCPPx64', 'OptionId.UCRT') -Wait -PassThru
"SDK_EXIT=$($p.ExitCode)" | Add-Content $result
"fim $(Get-Date -Format o)" | Add-Content $result
