pub mod core;
pub mod animation;
pub mod systems;
pub mod ui;
pub mod renderer;

pub mod vge {
    pub use crate::vge::core;
    pub use crate::vge::animation;
    pub use crate::vge::systems;
    pub use crate::vge::ui;
    pub use crate::vge::renderer;
}
