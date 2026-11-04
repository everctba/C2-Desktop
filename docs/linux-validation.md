# Validação em Linux

**Bead:** `c2-e12.2` · **Status:** roteiro pronto, execução pendente (esta máquina não tem distro WSL)

---

## O que já está validado sem Linux instalado

| Verificação | Como | Resultado |
|---|---|---|
| Código compila para Linux | `rustup target add x86_64-unknown-linux-gnu` + `cargo check --workspace --target x86_64-unknown-linux-gnu` | ✅ |
| Lints e testes compilam para Linux | `cargo clippy --workspace --all-targets --target x86_64-unknown-linux-gnu -- -D warnings` | ✅ |
| Teste de symlink portátil | `std::os::unix::fs::symlink` no `cfg(unix)` | ✅ |
| CI roda em `ubuntu-latest` | [.github/workflows/ci.yml](../.github/workflows/ci.yml) | pendente de remote |

O que **só** roda em Linux de verdade: `cargo test` executando, `c2 reindex` com
um `llama-server` Linux, `c2 grep`, `c2 related`, `c2 read`, `c2 cleaner` e o
encerramento de processo (`kill`).

---

## Roteiro A — WSL2 (recomendado)

Requer **PowerShell como Administrador** e, normalmente, **um reboot**.

```powershell
wsl --install -d Ubuntu
# reinicie se pedido; depois abra o Ubuntu e crie o usuário
```

Dentro do Ubuntu:

```bash
sudo apt update && sudo apt install -y build-essential unzip curl

# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"

# projeto (o disco do Windows aparece em /mnt/<letra>)
cd /mnt/h/TUDO/Codigo/Codigo/"C2 Desktop"

# 1. build de release no Linux
cargo build --workspace --release
./target/release/c2 --version

# 2. runtime oficial do llama.cpp para Linux (build CPU serve para validar fluxo)
VER=$(curl -s https://api.github.com/repos/ggml-org/llama.cpp/releases?per_page=10 \
      | grep -o '"tag_name": "b[0-9]*"' | head -1 | cut -d'"' -f4)
echo "build: $VER"
ASSET="llama-${VER}-bin-ubuntu-x64.zip"
curl -L -o /tmp/llama.zip \
  "https://github.com/ggml-org/llama.cpp/releases/download/${VER}/${ASSET}"
mkdir -p runtime-linux && unzip -oq /tmp/llama.zip -d runtime-linux
chmod +x runtime-linux/llama-server

# 3. projeto de teste e configuração (modelo do Windows via /mnt)
mkdir -p /tmp/c2-proj && cd /tmp/c2-proj
printf 'fn main() { println!("oi"); }\n' > main.rs
printf '# doc\n' > README.md

C2="/mnt/h/TUDO/Codigo/Codigo/C2 Desktop/target/release/c2"
GGUF="/mnt/h/ChatRTX/LocalAI/models/lmstudio-community/gemma-4-E2B-it-GGUF/gemma-4-E2B-it-Q4_K_M.gguf"

$C2 --root . init --json
$C2 --root . config set model.path "$GGUF"
$C2 --root . config set model.binary "/tmp/runtime-linux/llama-server"
$C2 --root . config set model.port 8099

# 4. fluxo completo
$C2 --root . status
$C2 --root . reindex --dry-run
$C2 --root . reindex --limit 1        # LLM de verdade (CPU: pode demorar)
$C2 --root . status                   # metadata = 1
$C2 --root . reindex                  # 2ª passada: 0 chamadas
$C2 --root . grep "onde imprime oi" --limit 3
$C2 --root . related main.rs --json
$C2 --root . read main.rs --lines 1:2
$C2 --root . cleaner                  # lista, não apaga
PGREP=$(pgrep -f llama-server || true); echo "processos llama-server: ${PGREP:-0}"
```

### Checklist a registrar no bead

- [ ] `cargo build --release` conclui no Linux
- [ ] `c2 init` cria `.c2/config.json`
- [ ] `c2 reindex --limit 1` grava metadado com semântica
- [ ] 2ª passada: **0 chamadas** ao modelo
- [ ] `c2 grep` retorna o arquivo com motivo e trecho
- [ ] `c2 related` lista `usa`/`usado por`
- [ ] `c2 read` devolve faixa de linhas
- [ ] `c2 cleaner` lista sem apagar
- [ ] nenhum processo `llama-server` órfão ao final

---

## Roteiro B — CI (automático)

O workflow [`ci.yml`](../.github/workflows/ci.yml) roda em `ubuntu-latest`:
`fmt`, `clippy -D warnings`, `test`, `build --release` e smoke test
(`init`, `status`, `reindex --dry-run`, `cleaner`). Basta haver um remote git
para o job executar — depois o link do run serve como evidência do bead.

---

## Observações

- Em WSL2 a GPU pode não estar disponível para o build Vulkan; use o build CPU
  para validar **fluxo** (não performance).
- O caminho com acento/espaco do repositório (`C2 Desktop`) é um bom teste de
  portabilidade por si só.
- Se o WSL não estiver disponível, a alternativa é uma máquina Linux real ou o
  job de CI.
