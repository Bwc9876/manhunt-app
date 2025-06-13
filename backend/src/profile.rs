use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlayerProfile {
    display_name: String,
    pfp_base64: Option<String>,
}

const STORE_NAME: &str = "profile.json";

impl PlayerProfile {
    pub fn has_pfp(&self) -> bool {
        self.pfp_base64.is_some()
    }

    pub fn load_from_store(app: &AppHandle) -> Option<Self> {
        let store = app.store(STORE_NAME).expect("Couldn't Create Store");

        let profile = store
            .get("profile")
            .and_then(|v| serde_json::from_value::<Self>(v).ok());

        store.close_resource();

        profile
    }

    pub fn write_to_store(&self, app: &AppHandle) {
        let store = app.store(STORE_NAME).expect("Couldn't create store");

        let value = serde_json::to_value(self.clone()).expect("Failed to serialize");
        store.set("profile", value);
    }
}
