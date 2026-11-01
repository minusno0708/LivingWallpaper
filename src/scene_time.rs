//! Shared scene time: independent of model choice, input timers and frame scheduling.
#[derive(Default)]
pub struct SceneClock {
    elapsed: f64,
    was_paused: bool,
}
#[derive(Default)]
pub struct StepPlan {
    pub steps: u32,
    pub step_dt: f32,
}
impl SceneClock {
    pub fn advance(&mut self, real_dt: f32, scale: f32, paused: bool) -> StepPlan {
        let scale = if scale.is_finite() {
            scale.clamp(0.0, 2.0)
        } else {
            1.0
        };
        if paused || scale == 0.0 {
            self.was_paused = true;
            return StepPlan::default();
        }
        if !real_dt.is_finite() {
            return StepPlan::default();
        }
        // The interval ending at the first resumed frame was spent paused.
        // Do not charge even a partial paused interval to scene time.
        let real_dt = if std::mem::take(&mut self.was_paused) {
            0.0
        } else {
            real_dt
        };
        // Bound real catch-up first, then scale. Subdivide afterward, so the
        // model's small-step clamp cannot silently discard accelerated time.
        let elapsed = real_dt.clamp(0.0, 0.10) * scale;
        self.elapsed += elapsed as f64;
        let steps = (elapsed / (1.0 / 60.0)).ceil().max(1.0) as u32;
        StepPlan {
            steps,
            step_dt: elapsed / steps as f32,
        }
    }
    pub fn seconds(&self) -> f32 {
        self.elapsed as f32
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rate_changes_are_continuous_and_pause_has_no_catch_up() {
        let mut clock = SceneClock::default();
        clock.advance(0.05, 1.0, false);
        clock.advance(0.05, 2.0, false);
        assert!((clock.seconds() - 0.15).abs() < 0.00001);
        assert_eq!(clock.advance(100.0, 1.0, true).steps, 0);
        assert_eq!(clock.advance(0.05, 0.0, false).steps, 0);
        assert!((clock.seconds() - 0.15).abs() < 0.00001);
        clock.advance(0.5, 0.5, false);
        assert!((clock.seconds() - 0.15).abs() < 0.00001);
        clock.advance(0.05, 0.5, false);
        assert!((clock.seconds() - 0.175).abs() < 0.00001);
    }
    #[test]
    fn accelerated_time_is_fully_integrated_with_small_steps() {
        let mut clock = SceneClock::default();
        let plan = clock.advance(0.1, 2.0, false);
        assert!(plan.step_dt <= 1.0 / 60.0 + 0.000001);
        assert!((plan.step_dt * plan.steps as f32 - 0.2).abs() < 0.00001);
        assert!((clock.seconds() - 0.2).abs() < 0.00001);
        let mut other = SceneClock::default();
        for _ in 0..6 {
            other.advance(1.0 / 60.0, 2.0, false);
        }
        assert!((other.seconds() - clock.seconds()).abs() < 0.00001);
    }
    #[test]
    fn invalid_values_cannot_poison_the_scene_clock() {
        let mut clock = SceneClock::default();
        assert_eq!(clock.advance(f32::NAN, 1.0, false).steps, 0);
        clock.advance(-1.0, 1.0, false);
        assert_eq!(clock.seconds(), 0.0);
        clock.advance(0.05, f32::NAN, false);
        assert!((clock.seconds() - 0.05).abs() < 0.00001);
    }
}
