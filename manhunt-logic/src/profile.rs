use serde::{Deserialize, Serialize};

#[derive(Clone, Default, Debug, Serialize, Deserialize, specta::Type)]
pub struct PlayerProfile {
    display_name: String,
    pfp_base64: Option<String>,
}
