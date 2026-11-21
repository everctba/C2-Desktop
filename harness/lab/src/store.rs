use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Task {
    pub id: u64,
    pub title: String,
    pub done: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Store {
    pub tasks: Vec<Task>,
    pub next_id: u64,
}

impl Store {
    pub fn load(path: &Path) -> Self {
        let texto = fs::read_to_string(path).unwrap_or_default();
        serde_json::from_str(&texto).unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let texto = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        fs::write(path, texto)
    }

    pub fn add(&mut self, title: &str) -> Result<u64, String> {
        self.next_id += 1;
        let id = self.next_id;
        self.tasks.push(Task {
            id,
            title: title.to_string(),
            done: false,
        });
        Ok(id)
    }

    pub fn complete(&mut self, id: u64) -> bool {
        if let Some(tarefa) = self.tasks.iter_mut().find(|t| t.id == id) {
            tarefa.done = true;
            true
        } else {
            false
        }
    }

    pub fn list(&self) -> &[Task] {
        &self.tasks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adiciona_e_completa() {
        let mut store = Store::default();
        let id = store.add("comprar cafe").unwrap();
        assert!(store.complete(id));
        assert!(store.list()[0].done);
    }

    #[test]
    fn rejeita_titulo_vazio() {
        let mut store = Store::default();
        assert!(store.add("").is_err());
    }

    #[test]
    fn persiste_em_json() {
        let dir = std::env::temp_dir().join(format!("taskline-{}", std::process::id()));
        let path = dir.join("tasks.json");
        std::fs::create_dir_all(&dir).unwrap();
        let mut store = Store::default();
        store.add("uma tarefa").unwrap();
        store.save(&path).unwrap();
        let recarregado = Store::load(&path);
        assert_eq!(recarregado.list().len(), 1);
        std::fs::remove_dir_all(dir).ok();
    }
}
