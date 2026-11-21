//! GPU renderer and retained visual scene graph.
pub mod palette;
pub mod scene;

/// Rendering backend contract. Concrete GPU implementations must preserve
/// device-loss recovery and a low-feature fallback path.
pub trait RenderBackend {
    fn backend_name(&self) -> &'static str;
}
