use serde::{Deserialize, Serialize};
use std::collections::HashMap;

fn default_scale() -> f32 {
    1.0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AvatarMetadata {
    pub avatar_name: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub view_position: [f32; 3],
    #[serde(default = "default_scale")]
    pub scale: f32,
    #[serde(default)]
    pub viseme_blendshapes: HashMap<String, u32>,
    #[serde(default)]
    pub blink_blendshapes: HashMap<String, u32>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

impl Default for AvatarMetadata {
    fn default() -> Self {
        Self {
            avatar_name: "UnnamedAvatar".to_string(),
            author: "Unknown".to_string(),
            view_position: [0.0, 1.4, 0.1],
            scale: 1.0,
            viseme_blendshapes: HashMap::new(),
            blink_blendshapes: HashMap::new(),
            extra: HashMap::new(),
        }
    }
}
