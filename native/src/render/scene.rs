//! Retained scene primitives consumed by the GPU renderer.
use super::palette::Rgba;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub origin: Vec2,
    pub size: Size,
}

impl Rect {
    pub fn contains(self, point: Vec2) -> bool {
        point.x >= self.origin.x
            && point.y >= self.origin.y
            && point.x <= self.origin.x + self.size.width
            && point.y <= self.origin.y + self.size.height
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fill {
    Solid(Rgba),
    LinearGradient {
        start: Rgba,
        end: Rgba,
        angle_radians: f32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Surface {
    pub bounds: Rect,
    pub fill: Fill,
    pub corner_radius: f32,
    pub glow: f32,
    pub opacity: f32,
}

impl Surface {
    pub fn sanitized(self) -> Self {
        Self {
            corner_radius: self.corner_radius.max(0.0),
            glow: self.glow.clamp(0.0, 1.0),
            opacity: self.opacity.clamp(0.0, 1.0),
            ..self
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Viewport {
    pub width_px: u32,
    pub height_px: u32,
    pub density_milli: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LayoutClass {
    Compact,
    Medium,
    Expanded,
}

impl Viewport {
    pub fn layout_class(self) -> LayoutClass {
        let dp_width = (self.width_px as u64 * 1000 / self.density_milli.max(1) as u64) as u32;
        match dp_width {
            0..=599 => LayoutClass::Compact,
            600..=839 => LayoutClass::Medium,
            _ => LayoutClass::Expanded,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_testing_uses_inclusive_surface_bounds() {
        let rect = Rect {
            origin: Vec2 { x: 10.0, y: 20.0 },
            size: Size {
                width: 30.0,
                height: 40.0,
            },
        };
        assert!(rect.contains(Vec2 { x: 10.0, y: 20.0 }));
        assert!(rect.contains(Vec2 { x: 40.0, y: 60.0 }));
        assert!(!rect.contains(Vec2 { x: 41.0, y: 60.0 }));
    }

    #[test]
    fn viewport_breakpoints_are_density_aware() {
        let compact = Viewport {
            width_px: 1080,
            height_px: 2400,
            density_milli: 3000,
        };
        let expanded = Viewport {
            width_px: 2560,
            height_px: 1600,
            density_milli: 2000,
        };
        assert_eq!(compact.layout_class(), LayoutClass::Compact);
        assert_eq!(expanded.layout_class(), LayoutClass::Expanded);
    }

    #[test]
    fn surface_effect_values_are_bounded() {
        let surface = Surface {
            bounds: Rect::default(),
            fill: Fill::Solid(Rgba(0.0, 0.0, 0.0, 1.0)),
            corner_radius: -5.0,
            glow: 4.0,
            opacity: -1.0,
        }
        .sanitized();
        assert_eq!(surface.corner_radius, 0.0);
        assert_eq!(surface.glow, 1.0);
        assert_eq!(surface.opacity, 0.0);
    }
}
