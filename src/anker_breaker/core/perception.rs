use serde::{Serialize, Deserialize};
use crate::anker_breaker::core::scene_node::{SceneNode, Anchor};
use std::sync::{Arc, RwLock};

#[derive(Serialize, Deserialize, Debug)]
pub struct PerceptionGrid {
    pub center_voxel: [i32; 3],
    pub resolution: f32,
    pub semantic_data: Vec<SemanticObject>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SemanticObject {
    pub name: String,
    pub anchor: String,
    pub position: [f32; 3],
}

pub struct PerceptionSystem;

impl PerceptionSystem {
    pub fn generate_view(nodes: &Vec<Arc<RwLock<SceneNode>>>, player_pos: [f32; 3]) -> PerceptionGrid {
        let mut semantic_data = Vec::new();

        for node in nodes {
            let n = node.read().unwrap();
            semantic_data.push(SemanticObject {
                name: n.name.clone(),
                anchor: format!("{:?}", n.anchor),
                position: [0.0, 0.0, 0.0], // Placeholder for real transform
            });
        }

        PerceptionGrid {
            center_voxel: [
                (player_pos[0] / 0.5).floor() as i32,
                (player_pos[1] / 0.5).floor() as i32,
                (player_pos[2] / 0.5).floor() as i32,
            ],
            resolution: 0.5,
            semantic_data,
        }
    }
}
