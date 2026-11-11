//! Shared monochrome renderer for preview and simulated display frames.

mod frame;
mod renderer;

pub use frame::Frame;
pub use renderer::{
    BlockReport, ComponentReport, DataState, RenderWarning, RenderedFrame, Renderer,
};

mod icons;
pub use icons::{IconPackInfo, IconStore};
