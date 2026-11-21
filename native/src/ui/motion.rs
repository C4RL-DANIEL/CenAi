//! Frame-rate-independent motion primitives for the native renderer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Motion {
    pub from: f32,
    pub to: f32,
    pub elapsed_seconds: f32,
    pub duration_seconds: f32,
}

impl Motion {
    pub fn new(from: f32, to: f32, duration_seconds: f32) -> Self {
        Self { from, to, elapsed_seconds: 0.0, duration_seconds: duration_seconds.max(0.001) }
    }

    pub fn advance(&mut self, delta_seconds: f32, reduced_motion: bool) -> f32 {
        if reduced_motion {
            self.elapsed_seconds = self.duration_seconds;
            return self.to;
        }
        self.elapsed_seconds = (self.elapsed_seconds + delta_seconds.max(0.0)).min(self.duration_seconds);
        self.value()
    }

    pub fn value(&self) -> f32 {
        let t = (self.elapsed_seconds / self.duration_seconds).clamp(0.0, 1.0);
        let eased = 1.0 - (1.0 - t).powi(3);
        self.from + (self.to - self.from) * eased
    }

    pub fn is_complete(&self) -> bool {
        self.elapsed_seconds >= self.duration_seconds
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn motion_is_monotonic_and_finishes_at_target() {
        let mut motion = Motion::new(0.0, 1.0, 0.2);
        let first = motion.advance(0.05, false);
        let second = motion.advance(0.05, false);
        let last = motion.advance(0.1, false);
        assert!(first < second && second < last);
        assert_eq!(last, 1.0);
        assert!(motion.is_complete());
    }

    #[test]
    fn reduced_motion_completes_without_interpolation() {
        let mut motion = Motion::new(2.0, 8.0, 1.0);
        assert_eq!(motion.advance(0.0, true), 8.0);
        assert!(motion.is_complete());
    }
}
