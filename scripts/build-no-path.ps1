# Gera o binario de teste do harness em harness/no_path/c2h.exe.
# Adicione harness/no_path ao PATH do usuario para testar sempre a versao atual.

param([switch]$Debug)

$ErrorActionPreference = 'Continue'
$root = Split-Path -Parent $PSScriptRoot
$harness = Join-Path $root 'harness'
$outDir = Join-Path $harness 'no_path'
New-Item -ItemType Directory -Force $outDir | Out-Null

Push-Location $harness
try {
    $args = @('build')
    if (-not $Debug) { $args += '--release' }
    & cargo @args
    if ($LASTEXITCODE -ne 0) { throw 'falha ao compilar o c2h' }

    $profile = if ($Debug) { 'debug' } else { 'release' }
    $exe = Join-Path $harness "target/$profile/c2h.exe"
    if (-not (Test-Path $exe)) { throw "binario nao encontrado: $exe" }
    $destino = Join-Path $outDir 'c2h.exe'
    try {
        Copy-Item $exe $destino -Force -ErrorAction Stop
        $hash = (Get-FileHash $destino -Algorithm SHA256).Hash.ToLower()
        "c2h: $destino"
        "sha256: $hash"
    } catch {
        $novo = Join-Path $outDir 'c2h-new.exe'
        Copy-Item $exe $novo -Force
        $hash = (Get-FileHash $novo -Algorithm SHA256).Hash.ToLower()
        Write-Warning "c2h.exe esta em uso; binario novo salvo em $novo"
        "c2h_new: $novo"
        "sha256: $hash"
    }
} finally {
    Pop-Location
}
