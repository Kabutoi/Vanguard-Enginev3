use std::collections::HashSet;
use crate::vge::core::scene_node::{SceneNode, Anchor};
use tracing::info;

pub struct VoxelCell {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

pub struct WorldPartitionManager {
    pub loaded_cells: HashSet<(i32, i32, i32)>,
    pub cell_size: f32,
}

impl WorldPartitionManager {
    pub fn new(cell_size: f32) -> Self {
        WorldPartitionManager {
            loaded_cells: HashSet::new(),
            cell_size,
        }
    }

    pub fn update_streaming(&mut self, player_pos: [f32; 3]) {
        let voxel_x = (player_pos[0] / self.cell_size).floor() as i32;
        let voxel_y = (player_pos[1] / self.cell_size).floor() as i32;
        let voxel_z = (player_pos[2] / self.cell_size).floor() as i32;

        let current_cell = (voxel_x, voxel_y, voxel_z);

        if !self.loaded_cells.contains(&current_cell) {
            info!("Streaming: Loading new Voxel Cell at {:?}.", current_cell);
            self.loaded_cells.insert(current_cell);
            
            // Logic to fetch cell data from JSON Assets and inject into Renderer
            self.perform_cell_hydration(current_cell);
        }
    }

    fn perform_cell_hydration(&self, cell: (i32, i32, i32)) {
        info!("Hydrating Voxel Cell {:?} from Grid-Based World Partition...", cell);
    }
}
