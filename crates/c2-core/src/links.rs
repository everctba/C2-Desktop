//! Extracao estatica de links entre arquivos (bead `c2-e6`).
//!
//! Sem LLM e sem parser completo: heuristica por linha para Rust, JS/TS e
//! Python. Toda aresta so existe se o destino for um arquivo real do workspace,
//! e o caminho gravado e sempre relativo a raiz, com `/`.

use std::path::Path;

use crate::schema::Link;

/// Teto de arestas por arquivo (o mesmo de `index.max_links_per_file`).
pub const DEFAULT_MAX_LINKS: usize = 64;

/// Extrai as arestas de `content`, resolvendo alvos a partir da raiz do workspace.
pub fn extract(root: &Path, rel: &str, content: &str, max_links: usize) -> Vec<Link> {
    let extension = extension_of(rel);
    let mut raw: Vec<(String, &'static str)> = Vec::new();

    match extension.as_str() {
        "rs" => extract_rust(root, rel, content, &mut raw),
        "js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx" | "vue" | "svelte" => {
            extract_js(rel, content, &mut raw)
        }
        "py" => extract_python(rel, content, &mut raw),
        _ => {}
    }

    let mut links: Vec<Link> = Vec::new();
    for (target, kind) in raw {
        let Some(resolved) = resolve(root, rel, &target) else {
            continue;
        };
        if resolved == rel {
            continue;
        }
        let link = Link::new(resolved, kind);
        if !links.contains(&link) {
            links.push(link);
        }
        if links.len() >= max_links {
            break;
        }
    }
    links.sort_by(|a, b| (a.to.as_str(), a.kind.as_str()).cmp(&(b.to.as_str(), b.kind.as_str())));
    links
}

fn extension_of(rel: &str) -> String {
    Path::new(rel)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default()
}

fn dir_of(rel: &str) -> String {
    match rel.rfind('/') {
        Some(index) => rel[..index].to_string(),
        None => String::new(),
    }
}

fn extract_rust(root: &Path, rel: &str, content: &str, raw: &mut Vec<(String, &'static str)>) {
    let dir = dir_of(rel);
    let crate_root = crate_root_of(root, &dir);

    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("mod ") {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                raw.push((join(&dir, &format!("{name}.rs")), "mod"));
                raw.push((join(&dir, &format!("{name}/mod.rs")), "mod"));
            }
        } else if let Some(rest) = trimmed.strip_prefix("pub mod ") {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                raw.push((join(&dir, &format!("{name}.rs")), "mod"));
            }
        } else if trimmed.starts_with("use ") || trimmed.starts_with("pub use ") {
            let path = trimmed
                .trim_start_matches("pub ")
                .trim_start_matches("use ")
                .trim_end_matches(';')
                .split(" as ")
                .next()
                .unwrap_or("")
                .trim();
            if let Some(segments) = use_segments(path) {
                push_rust_candidates(&crate_root, &dir, path, &segments, raw);
            }
        } else if let Some(start) = trimmed.find("include!(") {
            let rest = &trimmed[start + "include!(".len()..];
            let literal: String = rest
                .trim_start_matches('"')
                .chars()
                .take_while(|c| *c != '"')
                .collect();
            if !literal.is_empty() {
                raw.push((join(&dir, &literal), "includes"));
            }
        }
    }
}

fn crate_root_of(root: &Path, dir: &str) -> String {
    let mut current = dir.to_string();
    loop {
        let base = root.join(current.replace('/', std::path::MAIN_SEPARATOR_STR));
        let has_root = ["lib.rs", "main.rs"]
            .iter()
            .any(|name| base.join(name).is_file());
        if has_root {
            return current;
        }
        if current.is_empty() {
            return String::new();
        }
        let parent = dir_of(&current);
        if parent == current {
            return dir.to_string();
        }
        current = parent;
    }
}

fn use_segments(path: &str) -> Option<Vec<String>> {
    let path = path.split('{').next().unwrap_or("").trim();
    if path.is_empty() {
        return None;
    }
    Some(
        path.split("::")
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
    )
}

fn push_rust_candidates(
    crate_root: &str,
    file_dir: &str,
    path: &str,
    segments: &[String],
    raw: &mut Vec<(String, &'static str)>,
) {
    if path.starts_with("crate::") {
        let base = crate_root;
        push_prefix_candidates(base, segments.get(1..).unwrap_or(&[]), "imports", raw);
    } else if path.starts_with("super::") {
        let parent = dir_of(file_dir);
        push_prefix_candidates(&parent, segments.get(1..).unwrap_or(&[]), "imports", raw);
    } else if path.starts_with("self::") {
        push_prefix_candidates(file_dir, segments.get(1..).unwrap_or(&[]), "imports", raw);
    } else {
        // Caminho relativo ao proprio arquivo (modulos irmaos).
        push_prefix_candidates(file_dir, segments, "imports", raw);
    }
}

fn push_prefix_candidates(
    base: &str,
    segments: &[String],
    kind: &'static str,
    raw: &mut Vec<(String, &'static str)>,
) {
    for length in (1..=segments.len()).rev() {
        let module = segments[..length].join("/");
        raw.push((join(base, &format!("{module}.rs")), kind));
        raw.push((join(base, &format!("{module}/mod.rs")), kind));
    }
}

fn extract_js(rel: &str, content: &str, raw: &mut Vec<(String, &'static str)>) {
    let dir = dir_of(rel);
    for line in content.lines() {
        let trimmed = line.trim();
        let mut spec: Option<(&str, &'static str)> = None;

        if let Some(index) = trimmed.find("from ") {
            let rest = &trimmed[index + "from ".len()..];
            if let Some(value) = quoted(rest) {
                spec = Some((value, "imports"));
            }
        }
        if spec.is_none() {
            for marker in ["require(", "import("] {
                if let Some(index) = trimmed.find(marker) {
                    let rest = &trimmed[index + marker.len()..];
                    if let Some(value) = quoted(rest) {
                        spec = Some((value, "require"));
                    }
                }
            }
        }
        if let Some((spec, kind)) = spec {
            if spec.starts_with('.') {
                for candidate in js_candidates(&dir, spec) {
                    raw.push((candidate, kind));
                }
            }
        }
    }
}

fn quoted(text: &str) -> Option<&str> {
    let text = text.trim_start();
    let quote = text.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let rest = &text[quote.len_utf8()..];
    let end = rest.find(quote)?;
    Some(&rest[..end])
}

fn js_candidates(dir: &str, spec: &str) -> Vec<String> {
    let base = join(dir, spec.trim_start_matches("./"));
    let mut out = vec![base.clone()];
    for extension in ["ts", "tsx", "js", "jsx", "mjs", "cjs", "json"] {
        out.push(format!("{base}.{extension}"));
        out.push(format!("{base}/index.{extension}"));
    }
    out
}

fn extract_python(rel: &str, content: &str, raw: &mut Vec<(String, &'static str)>) {
    let file_dir = dir_of(rel);
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("from ") {
            let Some((module, _)) = rest.split_once(" import ") else {
                continue;
            };
            let module = module.trim();
            let dots = module.chars().take_while(|c| *c == '.').count();
            let remainder = module[dots..].trim();
            // Em import relativo o ponto de partida e o diretorio do arquivo:
            // um ponto = este diretorio, dois pontos = diretorio pai.
            let base = if dots > 0 {
                up(&file_dir, dots - 1)
            } else {
                String::new()
            };
            let stem = remainder.replace('.', "/");
            if !stem.is_empty() {
                raw.push((join(&base, &format!("{stem}.py")), "imports"));
                raw.push((join(&base, &format!("{stem}/__init__.py")), "imports"));
            }
        } else if let Some(rest) = trimmed.strip_prefix("import ") {
            for module in rest.split(',') {
                let module = module.trim();
                if module.is_empty() {
                    continue;
                }
                let stem = module.replace('.', "/");
                raw.push((format!("{stem}.py"), "imports"));
                raw.push((format!("{stem}/__init__.py"), "imports"));
            }
        }
    }
}

/// Sobe `levels` diretorios a partir de `dir`.
fn up(dir: &str, levels: usize) -> String {
    let mut parts: Vec<&str> = dir.split('/').filter(|p| !p.is_empty()).collect();
    let remove = levels.min(parts.len());
    parts.truncate(parts.len() - remove);
    parts.join("/")
}

/// Resolve `path` relativo a `base` (ambos relativos a raiz), normalizando `..`.
fn join(base: &str, path: &str) -> String {
    let mut parts: Vec<String> = base
        .split('/')
        .filter(|p| !p.is_empty() && *p != ".")
        .map(|p| p.to_string())
        .collect();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other.to_string()),
        }
    }
    parts.join("/")
}

fn resolve(root: &Path, rel: &str, candidate: &str) -> Option<String> {
    if candidate.is_empty() || candidate.starts_with("..") {
        return None;
    }
    let full = root.join(candidate.replace('/', std::path::MAIN_SEPARATOR_STR));
    if !full.is_file() {
        return None;
    }
    let _ = rel;
    Some(candidate.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn workspace(tag: &str) -> std::path::PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base =
            std::env::temp_dir().join(format!("c2-links-{tag}-{}-{stamp}", std::process::id()));
        fs::create_dir_all(&base).unwrap();
        base
    }

    fn touch(base: &Path, rel: &str) {
        let path = base.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "").unwrap();
    }

    #[test]
    fn use_crate_com_modulo_aninhado_resolve_contra_o_workspace() {
        let base = workspace("aninhado");
        touch(&base, "src/lib.rs");
        touch(&base, "src/mod00/arquivo_001.rs");
        touch(&base, "src/mod00/arquivo_002.rs");

        let links = extract(
            &base,
            "src/mod00/arquivo_001.rs",
            "use crate::mod00::arquivo_002;\n",
            DEFAULT_MAX_LINKS,
        );
        let targets: Vec<&str> = links.iter().map(|link| link.to.as_str()).collect();
        assert!(
            targets.contains(&"src/mod00/arquivo_002.rs"),
            "crate::mod00::arquivo_002 deveria resolver; veio {targets:?}"
        );
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn rust_mod_e_use_resolvem_para_arquivos_do_workspace() {
        let base = workspace("rust");
        touch(&base, "src/lib.rs");
        touch(&base, "src/auth.rs");
        touch(&base, "src/auth/session.rs");
        let content =
            "mod auth;\npub use crate::auth::Session;\nuse crate::auth::session::Thing;\n";

        let links = extract(&base, "src/lib.rs", content, DEFAULT_MAX_LINKS);
        let targets: Vec<&str> = links.iter().map(|l| l.to.as_str()).collect();
        assert!(targets.contains(&"src/auth.rs"));
        assert!(targets.contains(&"src/auth/session.rs"));
        assert!(links.iter().all(|l| l.kind == "mod" || l.kind == "imports"));
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn rust_ignora_alvo_inexistente_e_aresta_propria() {
        let base = workspace("rust-miss");
        touch(&base, "src/lib.rs");
        let content = "mod inexistente;\nmod lib;\n";
        let links = extract(&base, "src/lib.rs", content, DEFAULT_MAX_LINKS);
        assert!(links.is_empty());
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn js_import_estatico_e_require() {
        let base = workspace("js");
        touch(&base, "src/app.ts");
        touch(&base, "src/util.ts");
        touch(&base, "src/legacy.js");
        let content = "import { x } from './util';\nconst y = require('./legacy');\n";
        let links = extract(&base, "src/app.ts", content, DEFAULT_MAX_LINKS);
        let targets: Vec<&str> = links.iter().map(|l| l.to.as_str()).collect();
        assert!(targets.contains(&"src/util.ts"));
        assert!(targets.contains(&"src/legacy.js"));
        assert!(links.iter().any(|l| l.kind == "require"));
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn python_import_absoluto_e_relativo() {
        let base = workspace("py");
        touch(&base, "pkg/core.py");
        touch(&base, "pkg/sub/helper.py");
        let content = "from pkg.core import coisa\nfrom .sub.helper import outra\n";
        let links = extract(&base, "pkg/main.py", content, DEFAULT_MAX_LINKS);
        let targets: Vec<&str> = links.iter().map(|l| l.to.as_str()).collect();
        assert!(targets.contains(&"pkg/core.py"));
        assert!(targets.contains(&"pkg/sub/helper.py"));
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn respeita_limite_de_arestas() {
        let base = workspace("limit");
        for i in 0..10 {
            touch(&base, &format!("src/m{i}.rs"));
        }
        let content: String = (0..10).map(|i| format!("mod m{i};\n")).collect();
        let links = extract(&base, "src/lib.rs", content.as_str(), 3);
        assert_eq!(links.len(), 3);
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn normaliza_caminho_com_parenteses() {
        assert_eq!(join("src/auth", "../util.rs"), "src/util.rs");
        assert_eq!(join("", "a/b.rs"), "a/b.rs");
        assert_eq!(join("src", "./x.rs"), "src/x.rs");
    }
}
