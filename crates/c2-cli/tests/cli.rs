//! Testes de integracao da CLI: exercitam o binario `c2` de verdade.

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

fn tmp(tag: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("c2-cli-{tag}-{}-{stamp}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    path
}

fn c2() -> Command {
    Command::new(env!("CARGO_BIN_EXE_c2"))
}

fn stdout_of(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

#[test]
fn ciclo_init_status_cleaner() {
    let base = tmp("ciclo");
    fs::write(base.join("a.rs"), "fn main() {}").unwrap();
    fs::write(base.join("README.md"), "# doc").unwrap();

    let out = c2().arg("--root").arg(&base).arg("init").output().unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(base.join(".c2").join("config.json").is_file());
    assert!(base.join(".c2").join("logs").is_dir());

    // idempotente: a segunda execucao nao sobrescreve
    let out = c2().arg("--root").arg(&base).arg("init").output().unwrap();
    assert!(out.status.success());
    assert!(stdout_of(&out).contains("ja existia"));

    let out = c2()
        .arg("--root")
        .arg(&base)
        .args(["status", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["files"], 2);
    assert_eq!(value["metadata"], 0);
    assert_eq!(value["engine"]["running"], false);

    let out = c2()
        .arg("--root")
        .arg(&base)
        .arg("cleaner")
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(stdout_of(&out).contains("0 metadados"));

    fs::remove_dir_all(&base).ok();
}

#[test]
fn cleaner_so_apaga_com_yes() {
    let base = tmp("cleaner");
    c2().arg("--root").arg(&base).arg("init").output().unwrap();
    fs::write(base.join("a.rs"), "x").unwrap();
    let meta = base.join("a.rs.c2.json");
    fs::write(&meta, "{}").unwrap();

    let out = c2()
        .arg("--root")
        .arg(&base)
        .arg("cleaner")
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(meta.exists(), "dry-run nao deve apagar");

    let out = c2()
        .arg("--root")
        .arg(&base)
        .args(["cleaner", "--yes"])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(!meta.exists(), "--yes deve apagar");

    fs::remove_dir_all(&base).ok();
}

#[test]
fn config_roundtrip_pela_cli() {
    let base = tmp("config");
    c2().arg("--root").arg(&base).arg("init").output().unwrap();

    let out = c2()
        .arg("--root")
        .arg(&base)
        .args(["config", "set", "model.port", "9099"])
        .output()
        .unwrap();
    assert!(out.status.success());

    let out = c2()
        .arg("--root")
        .arg(&base)
        .args(["config", "get", "model.port"])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(stdout_of(&out).trim(), "9099");

    let out = c2()
        .arg("--root")
        .arg(&base)
        .args(["config", "get", "nao.existe"])
        .output()
        .unwrap();
    assert!(!out.status.success(), "chave invalida deve falhar");

    fs::remove_dir_all(&base).ok();
}

#[test]
fn read_mostra_faixa_de_linhas() {
    let base = tmp("read");
    c2().arg("--root").arg(&base).arg("init").output().unwrap();
    fs::write(base.join("a.rs"), "linha1\nlinha2\nlinha3\nlinha4\n").unwrap();

    let out = c2()
        .arg("--root")
        .arg(&base)
        .args(["read", "a.rs", "--lines", "2:3"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = stdout_of(&out);
    assert!(text.contains("linha2"));
    assert!(text.contains("linha3"));
    assert!(!text.contains("linha4"));

    let out = c2()
        .arg("--root")
        .arg(&base)
        .args(["read", "fora.rs"])
        .output()
        .unwrap();
    assert!(!out.status.success(), "arquivo inexistente deve falhar");

    fs::remove_dir_all(&base).ok();
}

#[test]
fn related_lista_dependencias() {
    let base = tmp("related");
    c2().arg("--root").arg(&base).arg("init").output().unwrap();

    let meta = |file: &str, links: serde_json::Value| {
        serde_json::json!({
            "schema_version": 1,
            "file": file,
            "hash": "sha256:abc",
            "size": 10,
            "mtime": 1,
            "semantic": {
                "what": "coisa", "why": "porque", "goal": "objetivo",
                "summary": "resumo", "responsibility": "dono"
            },
            "links": links
        })
        .to_string()
    };
    fs::write(base.join("b.rs"), "fn b() {}\n").unwrap();
    fs::write(
        base.join("b.rs.c2.json"),
        meta("b.rs", serde_json::json!([])),
    )
    .unwrap();
    fs::write(base.join("a.rs"), "mod b;\n").unwrap();
    fs::write(
        base.join("a.rs.c2.json"),
        meta("a.rs", serde_json::json!([{ "to": "b.rs", "type": "mod" }])),
    )
    .unwrap();

    let out = c2()
        .arg("--root")
        .arg(&base)
        .args(["related", "b.rs", "--json"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["used_by"][0]["from"], "a.rs");
    assert_eq!(value["used_by"][0]["type"], "mod");

    fs::remove_dir_all(&base).ok();
}

#[test]
fn status_sem_workspace_falha_com_mensagem() {
    let base = tmp("sem-ws");
    let out = c2()
        .arg("--root")
        .arg(&base)
        .arg("status")
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("nao inicializado"));
    fs::remove_dir_all(&base).ok();
}

#[test]
fn grep_encontra_pelo_metadado() {
    let base = tmp("grep");
    c2().arg("--root").arg(&base).arg("init").output().unwrap();

    let source = base.join("auth").join("session.rs");
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(&source, "pub fn valida_token() {}\n").unwrap();

    let meta = serde_json::json!({
        "schema_version": 1,
        "file": "auth/session.rs",
        "hash": "sha256:abc",
        "size": 24,
        "mtime": 1,
        "semantic": {
            "what": "sessao do usuario",
            "why": "validar autenticacao",
            "goal": "acesso seguro",
            "summary": "sessao e token",
            "responsibility": "dono da sessao"
        },
        "links": []
    });
    fs::write(
        base.join("auth").join("session.rs.c2.json"),
        meta.to_string(),
    )
    .unwrap();

    let out = c2()
        .arg("--root")
        .arg(&base)
        .args(["grep", "autenticacao", "--json", "--limit", "1"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let hits: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(hits[0]["file"], "auth/session.rs");
    assert!(hits[0]["score"].as_f64().unwrap() > 0.0);

    fs::remove_dir_all(&base).ok();
}

#[test]
fn mcp_responde_handshake_e_tools_list() {
    use std::io::BufRead;

    let base = tmp("mcp");
    fs::write(base.join("a.rs"), "fn main() {}").unwrap();
    let init = c2().arg("--root").arg(&base).arg("init").output().unwrap();
    assert!(init.status.success());

    let mut filho = c2()
        .arg("--root")
        .arg(&base)
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    {
        let mut stdin = filho.stdin.take().unwrap();
        writeln!(
            stdin,
            r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"protocolVersion":"2024-11-05"}}}}"#
        )
        .unwrap();
        writeln!(stdin, r#"{{"jsonrpc":"2.0","id":2,"method":"tools/list"}}"#).unwrap();
    }

    let mut linhas = Vec::new();
    {
        let stdout = filho.stdout.take().unwrap();
        let leitor = std::io::BufReader::new(stdout);
        for linha in leitor.lines().take(2) {
            linhas.push(linha.unwrap());
        }
    }
    let _ = filho.kill();
    let _ = filho.wait();

    let tudo = linhas.join("\n");
    assert!(tudo.contains("\"protocolVersion\""), "{tudo}");
    assert!(tudo.contains("2024-11-05"), "{tudo}");
    assert!(tudo.contains("c2_grep"), "{tudo}");
    assert!(tudo.contains("c2_map"), "{tudo}");
    fs::remove_dir_all(&base).ok();
}

#[test]
fn init_agent_grava_bloco_de_regras_idempotente() {
    let base = tmp("init-agent");
    fs::write(base.join("a.rs"), "fn a() {}").unwrap();

    let out = c2()
        .arg("--root")
        .arg(&base)
        .arg("init")
        .arg("--agent")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let agents = fs::read_to_string(base.join("AGENTS.md")).unwrap();
    assert!(agents.contains("c2:agent:begin"));
    assert!(agents.contains("c2 grep"));
    assert!(agents.contains("Metadado e hipotese"));

    // segunda execucao nao duplica o bloco
    let out2 = c2()
        .arg("--root")
        .arg(&base)
        .arg("init")
        .arg("--agent")
        .output()
        .unwrap();
    assert!(out2.status.success());
    let agents2 = fs::read_to_string(base.join("AGENTS.md")).unwrap();
    assert_eq!(agents2.matches("c2:agent:begin").count(), 1);

    fs::remove_dir_all(&base).ok();
}
