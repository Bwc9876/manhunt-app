use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use tauri::{AppHandle, Runtime};
use tauri_plugin_store::{Store, StoreExt};
use uuid::Uuid;

use crate::{
    game::{GameHistory, UtcDT},
    prelude::*,
    profile::PlayerProfile,
};

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct AppGameHistory {
    history: GameHistory,
    profiles: HashMap<Uuid, PlayerProfile>,
}

impl AppGameHistory {
    pub fn new(history: GameHistory, profiles: HashMap<Uuid, PlayerProfile>) -> Self {
        Self { history, profiles }
    }

    fn get_store<R: Runtime>(app: &AppHandle<R>) -> Result<Arc<Store<R>>> {
        app.store("histories.json")
            .context("Failed to get history store")
    }

    pub fn ls_histories(app: &AppHandle) -> Result<Vec<UtcDT>> {
        let store = Self::get_store(app)?;

        let mut histories = store
            .keys()
            .into_iter()
            .filter_map(|k| serde_json::from_str::<UtcDT>(&k).ok())
            .collect::<Vec<_>>();

        histories.sort_unstable_by(|a, b| a.cmp(b).reverse());

        Ok(histories)
    }

    pub fn get_history(app: &AppHandle, dt: UtcDT) -> Result<AppGameHistory> {
        let store = Self::get_store(app)?;
        let key = serde_json::to_string(&dt).context("Failed to make key")?;
        let val = store.get(key).context("Key not found")?;
        serde_json::from_value(val).context("Failed to deserialize game history")
    }

    pub fn save_history(&self, app: &AppHandle) -> Result {
        let store = Self::get_store(app)?;
        let serialized = serde_json::to_value(self).context("Failed to serialize history")?;
        let key =
            serde_json::to_string(&self.history.game_started).context("Failed to make key")?;
        store.set(key, serialized);
        Ok(())
    }
}
