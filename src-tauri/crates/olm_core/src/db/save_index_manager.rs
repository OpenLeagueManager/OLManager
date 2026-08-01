use log::info;
use std::path::{Path, PathBuf};

use crate::db::save_index::{SaveEntry, SaveIndex, load_or_create_index, write_index};

pub struct SaveIndexManager {
    index_path: PathBuf,
    index: SaveIndex,
}

impl SaveIndexManager {
    pub fn init(saves_dir: &Path) -> Result<Self, String> {
        let index_path = saves_dir.join("save_index.json");
        let index = load_or_create_index(&index_path)?;

        info!(
            "[save_manager] initialized with {} saves",
            index.saves.len()
        );

        Ok(Self { index_path, index })
    }

    pub fn list_saves(&self) -> &[SaveEntry] {
        &self.index.saves
    }

    pub fn find(&self, save_id: &str) -> Option<&SaveEntry> {
        self.index.find(save_id)
    }

    pub fn record_new_save(&mut self, entry: SaveEntry) -> Result<(), String> {
        self.index.add(entry);
        self.persist()
    }

    pub fn update_save(&mut self, entry: SaveEntry) -> Result<(), String> {
        if !self.index.update(&entry) {
            return Err(format!("Failed to update index for save '{}'", entry.id));
        }

        self.persist()
    }

    pub fn remove_save(&mut self, save_id: &str) -> Result<bool, String> {
        let removed = self.index.remove(save_id);
        self.persist()?;
        Ok(removed)
    }

    fn persist(&self) -> Result<(), String> {
        write_index(&self.index_path, &self.index)
    }
}
