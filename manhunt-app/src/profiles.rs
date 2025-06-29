use manhunt_logic::PlayerProfile;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

const STORE_NAME: &str = "profile";

pub fn read_profile_from_store(app: &AppHandle) -> Option<PlayerProfile> {
    let store = app.store(STORE_NAME).expect("Couldn't Create Store");

    let profile = store
        .get("profile")
        .and_then(|v| serde_json::from_value::<PlayerProfile>(v).ok());

    store.close_resource();

    profile
}

pub fn write_profile_to_store(app: &AppHandle, profile: PlayerProfile) {
    let store = app.store(STORE_NAME).expect("Couldn't create store");

    let value = serde_json::to_value(profile).expect("Failed to serialize");
    store.set("profile", value);
}
