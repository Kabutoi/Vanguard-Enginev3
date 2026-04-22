use rapier3d::prelude::*;
use tracing::info;

pub struct ProneController {
    pub velocity: Vector<f32>,
    pub is_prone: bool,
}

impl ProneController {
    pub fn new() -> Self {
        ProneController {
            velocity: Vector::zeros(),
            is_prone: true,
        }
    }

    pub fn update(&mut self, rigid_body: &mut RigidBody, input: Vector<f32>) {
        if self.is_prone {
            // Prone movement is slower and closer to the ground
            let prone_speed = 1.2;
            self.velocity = input * prone_speed;
            rigid_body.set_linvel(Vector::new(self.velocity.x, rigid_body.linvel().y, self.velocity.z), true);
            
            info!("ProneController updated: velocity {:?}.", self.velocity);
        }
    }
}
