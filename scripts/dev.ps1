# Desenvolvimento rapido do desktop.
#
# Uso:
#   pwsh -File scripts/dev.ps1              # Tauri + Vite HMR (janela real)
#   pwsh -File scripts/dev.ps1 -UiOnly      # so o frontend no navegador (mocks)
#   pwsh -File scripts/dev.ps1 -Build       # pacote/instalador (quando necessario)
#
# No modo Tauri:
#   - mudou .ts/.css/.html -> Vite faz HMR/reload, sem recompilar Rust
#   - mudou src-tauri/*.rs -> Tauri recompila e reinicia a janela
#
# UiOnly abre http://127.0.0.1:1420 com dados falsos (hasTauri=false); serve
# para ajustar texto/CSS sem compilar nada de Rust.

param(
    [switch]$UiOnly,
    [switch]$Build
)

$ErrorActionPreference = 'Continue'
$root = Split-Path -Parent $PSScriptRoot
$desktop = Join-Path $root 'apps\desktop'

if ($Build) {
    & (Join-Path $root 'scripts\build-installer.ps1')
    exit $LASTEXITCODE
}

Push-Location $desktop
try {
    if ($UiOnly) {
        Write-Host 'UI so no navegador: http://127.0.0.1:1420 (mocks, sem Rust)'
        $abridor = Start-Job -ScriptBlock {
            Start-Sleep -Seconds 2
            Start-Process 'http://127.0.0.1:1420'
        }
        try {
            & pnpm dev
        } finally {
            Stop-Job $abridor -ErrorAction SilentlyContinue
            Remove-Job $abridor -Force -ErrorAction SilentlyContinue
        }
    } else {
        Write-Host 'Tauri dev: HMR no frontend; Rust so recompila quando muda.'
        & pnpm tauri dev
    }
} finally {
    Pop-Location
}
