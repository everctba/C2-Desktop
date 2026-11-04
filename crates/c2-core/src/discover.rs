//! Descoberta de arquivos indexaveis.
//!
//! Regras (beads `c2-e2`):
//! - respeita `.gitignore` (mesmo fora de repo git) e `.c2ignore`;
//! - nao segue symlink, para nao sair do workspace;
//! - ignora arquivos ocultos e os diretorios de `index.excluded_dirs`;
//! - pula binarios (byte NUL na amostra) e arquivos acima de
//!   `index.max_file_bytes`, contando o motivo em [`Skipped`].

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use ignore::WalkBuilder;
use serde::Serialize;

use crate::config::Config;
use crate::error::Result;
use crate::schema::is_meta_path;

const SNIFF_SIZE: usize = 8192;

/// Arquivo candidato a indexacao.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    /// Caminho absoluto.
    pub path: PathBuf,
    /// Caminho relativo a raiz, com `/` como separador.
    pub rel: String,
    pub size: u64,
    /// mtime em segundos desde a epoch.
    pub mtime: i64,
}

impl FileEntry {
    /// Extensao em minusculas, ou `sem-extensao`.
    pub fn extension(&self) -> String {
        Path::new(&self.rel)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .unwrap_or_else(|| "sem-extensao".to_string())
    }
}

/// Motivos de descarte.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Skipped {
    pub binary: usize,
    pub too_large: usize,
    pub unsupported_extension: usize,
    pub other: usize,
}

/// Resultado da varredura.
#[derive(Debug, Clone, Default)]
pub struct Discovery {
    pub files: Vec<FileEntry>,
    pub skipped: Skipped,
}

/// Varre `root` aplicando a configuracao efetiva.
pub fn discover(root: &Path, config: &Config) -> Result<Discovery> {
    let excluded = config.index.excluded_dirs.clone();
    let extensions: Vec<String> = config
        .index
        .extensions
        .iter()
        .map(|e| e.trim_start_matches('.').to_ascii_lowercase())
        .collect();
    let max_bytes = config.index.max_file_bytes;

    let mut builder = WalkBuilder::new(root);
    builder
        .hidden(true)
        .follow_links(false)
        .require_git(false)
        .git_ignore(config.index.respect_gitignore)
        .git_global(config.index.respect_gitignore)
        .git_exclude(config.index.respect_gitignore)
        .add_custom_ignore_filename(".c2ignore");

    let excluded_for_filter = excluded.clone();
    builder.filter_entry(move |entry| {
        if entry.depth() == 0 {
            return true;
        }
        let name = entry.file_name().to_string_lossy();
        !excluded_for_filter.iter().any(|dir| dir == &name)
    });

    let mut discovery = Discovery::default();

    for item in builder.build() {
        let Ok(entry) = item else {
            discovery.skipped.other += 1;
            continue;
        };
        if entry.depth() == 0 {
            continue;
        }
        let Some(file_type) = entry.file_type() else {
            discovery.skipped.other += 1;
            continue;
        };
        if !file_type.is_file() {
            continue;
        }

        let path = entry.path().to_path_buf();
        if is_meta_path(&path) {
            continue;
        }

        let Ok(metadata) = entry.metadata() else {
            discovery.skipped.other += 1;
            continue;
        };
        let size = metadata.len();
        if size > max_bytes {
            discovery.skipped.too_large += 1;
            continue;
        }

        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase());

        if !extensions.is_empty() {
            if let Some(ext) = &extension {
                if !extensions.iter().any(|allowed| allowed == ext) {
                    discovery.skipped.unsupported_extension += 1;
                    continue;
                }
            }
            // Sem extensao: deixa a checagem de binario decidir (Makefile, Dockerfile...).
        }

        if is_binary(&path) {
            discovery.skipped.binary += 1;
            continue;
        }

        let mtime = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        discovery.files.push(FileEntry {
            rel: relative_unix(root, &path),
            path,
            size,
            mtime,
        });
    }

    discovery.files.sort_by(|a, b| a.rel.cmp(&b.rel));
    Ok(discovery)
}

/// Verdadeiro quando a amostra parece binaria (contem byte NUL).
pub fn is_binary(path: &Path) -> bool {
    let Ok(mut file) = File::open(path) else {
        return true;
    };
    let mut buffer = [0u8; SNIFF_SIZE];
    let Ok(read) = file.read(&mut buffer) else {
        return true;
    };
    buffer[..read].contains(&0)
}

/// Caminho relativo com separador `/`, estavel entre Windows e Linux.
pub fn relative_unix(root: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path);
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp(tag: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("c2-disc-{tag}-{}-{stamp}", std::process::id()));
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn write(path: &Path, content: &[u8]) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    #[test]
    fn descobre_texto_e_ignora_binario_grande_e_meta() {
        let base = tmp("basic");
        write(&base.join("src").join("a.rs"), b"fn main() {}");
        write(&base.join("README.md"), b"# doc");
        write(&base.join("img.bin"), &[0u8, 1, 2, 3]);
        write(&base.join("binario.txt"), &[b'a', 0u8, b'b']);
        write(&base.join("grande.rs"), &vec![b'x'; 4096]);
        write(&base.join("a.rs.c2.json"), b"{}");

        let mut config = Config::default();
        config.index.max_file_bytes = 1024;

        let found = discover(&base, &config).unwrap();
        let names: Vec<_> = found.files.iter().map(|f| f.rel.as_str()).collect();
        assert_eq!(names, vec!["README.md", "src/a.rs"]);
        assert_eq!(found.skipped.binary, 1); // binario.txt traz NUL
        assert_eq!(found.skipped.too_large, 1); // grande.rs
        assert_eq!(found.skipped.unsupported_extension, 1); // img.bin
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn respeita_excluded_dirs_e_c2ignore() {
        let base = tmp("ignore");
        write(&base.join("src").join("keep.rs"), b"ok");
        write(&base.join("node_modules").join("lib.js"), b"x");
        write(&base.join("vendor").join("v.rs"), b"x");
        write(&base.join("build").join("b.rs"), b"x");
        write(&base.join("ignorado.rs"), b"x");
        write(&base.join(".c2ignore"), b"ignorado.rs\n");

        let config = Config::default();
        let found = discover(&base, &config).unwrap();
        let names: Vec<_> = found.files.iter().map(|f| f.rel.as_str()).collect();
        assert_eq!(names, vec!["src/keep.rs"]);
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn respeita_allowlist_de_extensoes() {
        let base = tmp("ext");
        write(&base.join("a.rs"), b"ok");
        write(&base.join("b.xyz"), b"ok");
        write(&base.join("Makefile"), b"all:");

        let mut config = Config::default();
        config.index.extensions = vec!["rs".into()];

        let found = discover(&base, &config).unwrap();
        let names: Vec<_> = found.files.iter().map(|f| f.rel.as_str()).collect();
        assert_eq!(names, vec!["Makefile", "a.rs"]);
        assert_eq!(found.skipped.unsupported_extension, 1);
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn caminho_relativo_usa_barra() {
        let base = tmp("rel");
        write(&base.join("src").join("deep").join("a.rs"), b"x");
        let config = Config::default();
        let found = discover(&base, &config).unwrap();
        assert_eq!(found.files[0].rel, "src/deep/a.rs");
        assert!(found.files[0].path.is_absolute() || found.files[0].path.exists());
        fs::remove_dir_all(&base).ok();
    }
}
