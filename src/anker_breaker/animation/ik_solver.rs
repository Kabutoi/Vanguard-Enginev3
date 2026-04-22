use rapier3d::prelude::Vector;

pub struct Bone {
    pub position: Vector<f32>,
    pub length: f32,
}

pub struct FabrikSolver {
    pub bones: Vec<Bone>,
    pub tolerance: f32,
    pub max_iterations: usize,
}

impl FabrikSolver {
    pub fn new(joint_positions: Vec<Vector<f32>>) -> Self {
        let mut bones = Vec::new();
        for i in 0..joint_positions.len() - 1 {
            let length = (joint_positions[i+1] - joint_positions[i]).norm();
            bones.push(Bone {
                position: joint_positions[i],
                length,
            });
        }
        // Add tip
        bones.push(Bone {
            position: joint_positions[joint_positions.len() - 1],
            length: 0.0,
        });

        FabrikSolver {
            bones,
            tolerance: 0.001,
            max_iterations: 10,
        }
    }

    pub fn solve(&mut self, target: Vector<f32>) {
        let root_pos = self.bones[0].position;
        let dist = (target - root_pos).norm();
        let total_length: f32 = self.bones.iter().map(|b| b.length).sum();

        if dist > total_length {
            // Target is unreachable
            for i in 0..self.bones.len() - 1 {
                let r = (target - self.bones[i].position).norm();
                let t = self.bones[i].length / r;
                self.bones[i+1].position = (1.0 - t) * self.bones[i].position + t * target;
            }
        } else {
            // Target is reachable
            for _ in 0..self.max_iterations {
                // Forward pass
                self.bones.last_mut().unwrap().position = target;
                for i in (0..self.bones.len() - 1).rev() {
                    let r = (self.bones[i+1].position - self.bones[i].position).norm();
                    let t = self.bones[i].length / r;
                    self.bones[i].position = (1.0 - t) * self.bones[i+1].position + t * self.bones[i].position;
                }

                // Backward pass
                self.bones[0].position = root_pos;
                for i in 0..self.bones.len() - 1 {
                    let r = (self.bones[i+1].position - self.bones[i].position).norm();
                    let t = self.bones[i].length / r;
                    self.bones[i+1].position = (1.0 - t) * self.bones[i].position + t * self.bones[i+1].position;
                }

                if (self.bones.last().unwrap().position - target).norm() < self.tolerance {
                    break;
                }
            }
        }
        
        tracing::debug!("FABRIK solver converged for target {:?}", target);
    }
}
