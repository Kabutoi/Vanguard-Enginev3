use std::sync::{Arc, RwLock};
use std::collections::HashMap;
use serde::{Serialize, Deserialize};

/// Sentinel Directive root anchors for hierarchy sovereignty.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    Management,
    Systems,
    Player,
    Environment,
    Lighting,
    UI,
}

/// The base SceneNode structure for Traditional OOP in Rust.
pub struct SceneNode {
    pub name: String,
    pub anchor: Anchor,
    pub parent: Option<Arc<RwLock<SceneNode>>>,
    pub children: Vec<Arc<RwLock<SceneNode>>>,
    pub properties: HashMap<String, String>,
}

impl SceneNode {
    pub fn new(name: &str, anchor: Anchor) -> Arc<RwLock<Self>> {
        Arc::new(RwLock::new(SceneNode {
            name: name.to_string(),
            anchor,
            parent: None,
            children: Vec::new(),
            properties: HashMap::new(),
        }))
    }

    pub fn add_child(parent: Arc<RwLock<SceneNode>>, child: Arc<RwLock<SceneNode>>) {
        {
            let mut child_locked = child.write().unwrap();
            child_locked.parent = Some(parent.clone());
        }
        let mut parent_locked = parent.write().unwrap();
        parent_locked.children.push(child);
        
        tracing::info!(
            "SceneNode '{}' added child '{}' under anchor {:?}.",
            parent_locked.name,
            parent_locked.children.last().unwrap().read().unwrap().name,
            parent_locked.anchor
        );
    }
}
