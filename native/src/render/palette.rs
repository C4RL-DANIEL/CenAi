//! CenAI visual tokens. Keep effects restrained and surface-oriented.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba(pub f32, pub f32, pub f32, pub f32);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub canvas: Rgba,
    pub surface: Rgba,
    pub elevated_surface: Rgba,
    pub foreground: Rgba,
    pub muted: Rgba,
    pub accent: Rgba,
    pub accent_glow: Rgba,
}

pub const DARK: Palette = Palette {
    canvas: Rgba(0.035, 0.043, 0.063, 1.0),
    surface: Rgba(0.067, 0.078, 0.106, 0.96),
    elevated_surface: Rgba(0.094, 0.110, 0.145, 0.98),
    foreground: Rgba(0.93, 0.95, 0.99, 1.0),
    muted: Rgba(0.56, 0.61, 0.70, 1.0),
    accent: Rgba(0.42, 0.55, 1.0, 1.0),
    accent_glow: Rgba(0.30, 0.48, 1.0, 0.24),
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_colors_are_finite_and_normalized() {
        let p = DARK;
        let colors = [p.canvas, p.surface, p.elevated_surface, p.foreground, p.muted, p.accent, p.accent_glow];
        for color in colors {
            for channel in [color.0, color.1, color.2, color.3] {
                assert!(channel.is_finite());
                assert!((0.0..=1.0).contains(&channel));
            }
        }
    }
}
