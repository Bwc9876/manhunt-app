use serde::{Deserialize, Serialize};

#[derive(Clone, Default, Debug, Serialize, Deserialize, specta::Type)]
pub struct PlayerProfile {
    pub display_name: String,
    pub pfp_base64: Option<String>,
}
