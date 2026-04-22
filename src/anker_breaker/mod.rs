pub mod core;
pub mod animation;
pub mod systems;
pub mod ui;
pub mod renderer;

pub mod anker_breaker {
    pub use crate::anker_breaker::core;
    pub use crate::anker_breaker::animation;
    pub use crate::anker_breaker::systems;
    pub use crate::anker_breaker::ui;
    pub use crate::anker_breaker::renderer;
}
