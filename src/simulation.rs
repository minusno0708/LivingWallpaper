//! Platform independent model contract. All positions use physical wallpaper pixels.
use glam::Vec2;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug)]
pub struct Fish {
    pub position: Vec2,
    pub velocity: Vec2,
    pub phase: f32,
}
#[derive(Clone, Copy, Debug)]
pub struct Obstacle {
    pub min: Vec2,
    pub max: Vec2,
}
#[derive(Default, Clone, Copy)]
pub struct Pointer {
    pub position: Vec2,
    pub speed: f32,
    pub still_seconds: f32,
    pub enabled: bool,
}
pub trait SchoolModel {
    fn resize_population(&mut self, count: usize, size: Vec2);
    fn step(&mut self, dt: f32, size: Vec2, obstacles: &[Obstacle], pointer: Pointer);
    fn fish(&self) -> &[Fish];
}
pub struct FishSchool {
    fish: Vec<Fish>,
    seed: u32,
}
impl Default for FishSchool {
    fn default() -> Self {
        Self {
            fish: Vec::new(),
            seed: 0x12345678,
        }
    }
}
impl FishSchool {
    fn random(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        self.seed as f32 / u32::MAX as f32
    }
}
fn pointer_force(position: Vec2, p: Pointer) -> Vec2 {
    if !p.enabled {
        return Vec2::ZERO;
    }
    let delta = p.position - position;
    let distance = delta.length();
    if p.speed > 35.0 && distance < 220.0 {
        -delta.normalize_or_zero() * 430.0 * (1.0 - distance / 220.0)
    } else if p.still_seconds > 0.8 && distance > 42.0 && distance < 420.0 {
        delta.normalize_or_zero() * 32.0
    } else {
        Vec2::ZERO
    }
}
impl SchoolModel for FishSchool {
    fn resize_population(&mut self, count: usize, size: Vec2) {
        self.fish.truncate(count);
        while self.fish.len() < count {
            let position = Vec2::new(self.random() * size.x, self.random() * size.y);
            let angle = self.random() * std::f32::consts::TAU;
            let phase = self.random() * std::f32::consts::TAU;
            self.fish.push(Fish {
                position,
                velocity: Vec2::new(angle.cos(), angle.sin()) * 65.0,
                phase,
            });
        }
    }
    fn fish(&self) -> &[Fish] {
        &self.fish
    }
    fn step(&mut self, dt: f32, size: Vec2, obstacles: &[Obstacle], pointer: Pointer) {
        if size.min_element() < 1.0 {
            return;
        }
        let dt = dt.clamp(0.0, 1.0 / 20.0);
        // Snapshot positions; spatial buckets avoid scanning the whole school for each fish.
        let old = self.fish.clone();
        let mut grid: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
        let cell = |p: Vec2| ((p.x / 85.0) as i32, (p.y / 85.0) as i32);
        for (i, f) in old.iter().enumerate() {
            grid.entry(cell(f.position)).or_default().push(i);
        }
        for (i, f) in self.fish.iter_mut().enumerate() {
            let mut separation = Vec2::ZERO;
            let mut alignment = Vec2::ZERO;
            let mut center = Vec2::ZERO;
            let mut count = 0.0;
            let (cx, cy) = cell(f.position);
            for x in cx - 1..=cx + 1 {
                for y in cy - 1..=cy + 1 {
                    if let Some(neighbors) = grid.get(&(x, y)) {
                        for &j in neighbors {
                            if i == j {
                                continue;
                            }
                            let d = f.position - old[j].position;
                            let d2 = d.length_squared();
                            if d2 < 85.0 * 85.0 {
                                alignment += old[j].velocity;
                                center += old[j].position;
                                count += 1.0;
                                if d2 < 28.0 * 28.0 {
                                    separation += d / d2.max(1.0);
                                }
                            }
                        }
                    }
                }
            }
            let mut force = separation * 1600.0;
            if count > 0.0 {
                force +=
                    (alignment / count - f.velocity) * 0.85 + (center / count - f.position) * 0.20;
            }
            force += pointer_force(f.position, pointer);
            for obstacle in obstacles {
                let future = f.position + f.velocity * 0.30;
                let nearest = future.clamp(obstacle.min, obstacle.max);
                let d = future - nearest;
                if d.length_squared() < 65.0 * 65.0 {
                    let outward = if d.length_squared() > 0.01 {
                        d.normalize()
                    } else {
                        (future - (obstacle.min + obstacle.max) * 0.5).normalize_or_zero()
                    };
                    force += outward * 500.0 * (1.0 - d.length() / 65.0);
                }
            }
            let margin = 95.0;
            if f.position.x < margin {
                force.x += (margin - f.position.x) * 1.6;
            }
            if f.position.x > size.x - margin {
                force.x -= (f.position.x - size.x + margin) * 1.6;
            }
            if f.position.y < margin {
                force.y += (margin - f.position.y) * 1.6;
            }
            if f.position.y > size.y - margin {
                force.y -= (f.position.y - size.y + margin) * 1.6;
            }
            force += Vec2::new(f.phase.cos(), f.phase.sin()) * 6.0;
            f.velocity += force.clamp_length_max(600.0) * dt;
            if f.velocity.length_squared() < 0.001 {
                f.velocity = Vec2::X * 35.0;
            }
            f.velocity = f.velocity.clamp_length(35.0, 125.0);
            f.position = (f.position + f.velocity * dt).clamp(Vec2::ZERO, size);
            // Hard projection is the final safety net for icons moved directly onto a fish.
            for o in obstacles {
                let min = o.min - Vec2::splat(8.0);
                let max = o.max + Vec2::splat(8.0);
                if f.position.x > min.x
                    && f.position.x < max.x
                    && f.position.y > min.y
                    && f.position.y < max.y
                {
                    let distances = [
                        f.position.x - min.x,
                        max.x - f.position.x,
                        f.position.y - min.y,
                        max.y - f.position.y,
                    ];
                    let side = (0..4)
                        .min_by(|&a, &b| distances[a].total_cmp(&distances[b]))
                        .unwrap();
                    match side {
                        0 => {
                            f.position.x = min.x;
                            f.velocity.x = -f.velocity.x.abs();
                        }
                        1 => {
                            f.position.x = max.x;
                            f.velocity.x = f.velocity.x.abs();
                        }
                        2 => {
                            f.position.y = min.y;
                            f.velocity.y = -f.velocity.y.abs();
                        }
                        _ => {
                            f.position.y = max.y;
                            f.velocity.y = f.velocity.y.abs();
                        }
                    }
                }
            }
            f.position = f.position.clamp(Vec2::ZERO, size);
            f.phase += dt * (5.0 + f.velocity.length() * 0.03);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mouse_modes() {
        let mut p = Pointer {
            enabled: true,
            position: Vec2::new(100.0, 0.0),
            speed: 100.0,
            still_seconds: 0.0,
        };
        assert!(pointer_force(Vec2::ZERO, p).x < 0.0);
        p.speed = 0.0;
        p.still_seconds = 1.0;
        assert!(pointer_force(Vec2::ZERO, p).x > 0.0);
        p.enabled = false;
        assert_eq!(pointer_force(Vec2::ZERO, p), Vec2::ZERO);
    }
    #[test]
    fn long_run_is_finite_and_avoids_icons() {
        let mut school = FishSchool::default();
        let size = Vec2::new(800.0, 600.0);
        school.resize_population(300, size);
        let o = Obstacle {
            min: Vec2::new(300.0, 200.0),
            max: Vec2::new(380.0, 300.0),
        };
        for _ in 0..600 {
            school.step(1.0 / 30.0, size, &[o], Pointer::default());
        }
        for f in school.fish() {
            assert!(f.position.is_finite() && f.velocity.is_finite());
            assert!(f.position.cmpge(Vec2::ZERO).all() && f.position.cmple(size).all());
            assert!(
                !(f.position.x > o.min.x
                    && f.position.x < o.max.x
                    && f.position.y > o.min.y
                    && f.position.y < o.max.y)
            );
        }
        school.resize_population(32, size);
        assert_eq!(school.fish().len(), 32);
    }
    #[test]
    #[ignore = "manual performance measurement; run with --release --ignored --nocapture"]
    fn simulation_benchmark() {
        for n in [240, 1000] {
            let mut s = FishSchool::default();
            let size = Vec2::new(1920.0, 1080.0);
            s.resize_population(n, size);
            let start = std::time::Instant::now();
            for _ in 0..600 {
                s.step(1.0 / 60.0, size, &[], Pointer::default());
            }
            println!(
                "{n} fish: {:.3} ms/step (600 steps, simulation only)",
                start.elapsed().as_secs_f64() * 1000.0 / 600.0
            );
        }
    }
}
