//! Platform independent model contract. All positions use physical wallpaper pixels.
use glam::Vec2;
use std::collections::HashMap;

const MAX_SPEED: f32 = 150.0;
const CRUISE_SPEED: f32 = 70.0;
const SCREEN_MARGIN: f32 = 95.0;
const CURSOR_RADIUS: f32 = 24.0;
const NORMAL_ACCELERATION: f32 = 600.0;
const REJOIN_ACCELERATION: f32 = 900.0;

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
    pending_spawn: Vec<bool>,
}
impl Default for FishSchool {
    fn default() -> Self {
        Self {
            fish: Vec::new(),
            seed: 0x12345678,
            pending_spawn: Vec::new(),
        }
    }
}
impl FishSchool {
    fn spawn_position(&mut self, initial: Vec2, size: Vec2, zones: &[Obstacle]) -> Option<Vec2> {
        let inset = Vec2::splat(SCREEN_MARGIN).min(size * 0.25);
        let upper = size - inset;
        let valid = |p: Vec2| {
            p.cmpge(inset).all() && p.cmple(upper).all() && !zones.iter().any(|&o| contains(o, p))
        };
        if valid(initial) {
            return Some(initial);
        }
        // Bounded random attempts, then a deterministic search for small free regions.
        for _ in 0..64 {
            let p = inset + Vec2::new(self.random(), self.random()) * (upper - inset);
            if valid(p) {
                return Some(p);
            }
        }
        for y in 0..20 {
            for x in 0..20 {
                let p = inset
                    + Vec2::new((x as f32 + 0.5) / 20.0, (y as f32 + 0.5) / 20.0) * (upper - inset);
                if valid(p) {
                    return Some(p);
                }
            }
        }
        None
    }
    fn random(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        self.seed as f32 / u32::MAX as f32
    }
}
fn contains(rect: Obstacle, point: Vec2) -> bool {
    point.cmpge(rect.min).all() && point.cmple(rect.max).all()
}
fn icon_clearance(rect: Obstacle, size: Vec2) -> Obstacle {
    let mut min = (rect.min - Vec2::splat(8.0)).clamp(Vec2::ZERO, size);
    let mut max = (rect.max + Vec2::splat(8.0)).clamp(Vec2::ZERO, size);
    // Only close physically tiny passages during swimming. Wider edge gaps
    // are excluded at spawn time without broadening normal icon avoidance.
    for axis in 0..2 {
        if min[axis] < 16.0 {
            min[axis] = 0.0;
        }
        if max[axis] > size[axis] - 16.0 {
            max[axis] = size[axis];
        }
    }
    Obstacle { min, max }
}
fn free_projection(point: Vec2, size: Vec2, obstacles: &[Obstacle]) -> Option<Vec2> {
    if !obstacles.iter().any(|&o| contains(o, point)) {
        return None;
    }
    // The bounding box of connected obstacles is not their union: an L-shaped
    // monitor gap can span the entire scene while leaving ample free space.
    // Test actual rectangle edges/corners and validate against every obstacle.
    let mut best: Option<Vec2> = None;
    for o in obstacles {
        let lo = o.min - Vec2::splat(0.5);
        let hi = o.max + Vec2::splat(0.5);
        for candidate in [
            Vec2::new(lo.x, point.y),
            Vec2::new(hi.x, point.y),
            Vec2::new(point.x, lo.y),
            Vec2::new(point.x, hi.y),
            lo,
            hi,
            Vec2::new(lo.x, hi.y),
            Vec2::new(hi.x, lo.y),
        ] {
            if candidate.cmpge(Vec2::ZERO).all()
                && candidate.cmple(size).all()
                && !obstacles.iter().any(|&r| contains(r, candidate))
                && best.map_or(true, |old| {
                    candidate.distance_squared(point) < old.distance_squared(point)
                })
            {
                best = Some(candidate);
            }
        }
    }
    best
}
fn cursor_is_solid(p: Pointer) -> bool {
    p.enabled && p.still_seconds >= 10.0 && p.speed <= 35.0
}
fn cursor_avoidance(position: Vec2, velocity: Vec2, p: Pointer) -> Vec2 {
    if !cursor_is_solid(p) {
        return Vec2::ZERO;
    }
    let relative = position - p.position;
    let future = relative + velocity * 0.18;
    let distance = relative.length().min(future.length());
    if distance >= CURSOR_RADIUS + 18.0 {
        return Vec2::ZERO;
    }
    let normal = if relative.length_squared() > 0.001 {
        relative.normalize()
    } else {
        Vec2::X
    };
    let mut tangent = Vec2::new(-normal.y, normal.x);
    if tangent.dot(velocity) < 0.0 {
        tangent = -tangent;
    }
    let weight = (1.0 - distance / (CURSOR_RADIUS + 18.0)).clamp(0.0, 1.0);
    normal * 600.0 * weight + tangent * 240.0 * weight
}
fn cursor_collision(
    start: Vec2,
    end: Vec2,
    p: Pointer,
    size: Vec2,
    obstacles: &[Obstacle],
) -> Option<Vec2> {
    if !cursor_is_solid(p) {
        return None;
    }
    let offset = start - p.position;
    let movement = end - start;
    let length_squared = movement.length_squared();
    let closest_t = if length_squared > 0.0001 {
        (-offset.dot(movement) / length_squared).clamp(0.0, 1.0)
    } else {
        0.0
    };
    if (offset + movement * closest_t).length_squared() >= CURSOR_RADIUS * CURSOR_RADIUS {
        return None;
    }
    // Sweep the segment, so even a step ending beyond the circle cannot tunnel.
    let mut contact = start;
    let mut remaining = movement;
    if offset.length_squared() >= CURSOR_RADIUS * CURSOR_RADIUS && length_squared > 0.0001 {
        let b = offset.dot(movement);
        let c = offset.length_squared() - CURSOR_RADIUS * CURSOR_RADIUS;
        let t =
            ((-b - (b * b - length_squared * c).max(0.0).sqrt()) / length_squared).clamp(0.0, 1.0);
        contact = start + movement * t;
        remaining = movement * (1.0 - t);
    }
    let mut normal = (contact - p.position).normalize_or_zero();
    if normal.length_squared() < 0.001 {
        normal = Vec2::X;
    }
    let slide = remaining - normal * remaining.dot(normal).min(0.0);
    let candidate = p.position + normal * (CURSOR_RADIUS + 0.5) + slide;
    let valid = |point: Vec2| {
        point.cmpge(Vec2::ZERO).all()
            && point.cmple(size).all()
            && point.distance_squared(p.position) >= CURSOR_RADIUS * CURSOR_RADIUS
            && !obstacles.iter().any(|&o| contains(o, point))
    };
    if valid(candidate) {
        return Some(candidate);
    }
    // Near an edge or icon, use a free arc instead of pushing into it.
    (0..32)
        .map(|i| {
            let angle = i as f32 * std::f32::consts::TAU / 32.0;
            p.position + Vec2::new(angle.cos(), angle.sin()) * (CURSOR_RADIUS + 0.5)
        })
        .filter(|&point| valid(point))
        .min_by(|a, b| {
            a.distance_squared(contact)
                .total_cmp(&b.distance_squared(contact))
        })
}
fn pointer_force(position: Vec2, p: Pointer) -> Vec2 {
    if !p.enabled {
        return Vec2::ZERO;
    }
    let delta = p.position - position;
    let distance = delta.length();
    if p.speed > 35.0 && distance < 220.0 {
        -delta.normalize_or_zero() * 430.0 * (1.0 - distance / 220.0)
    } else if p.still_seconds >= 10.0 && distance > 42.0 && distance < 200.0 {
        delta.normalize_or_zero() * 32.0
    } else {
        Vec2::ZERO
    }
}
impl SchoolModel for FishSchool {
    fn resize_population(&mut self, count: usize, size: Vec2) {
        self.fish.truncate(count);
        self.pending_spawn.truncate(count);
        self.pending_spawn.resize(self.fish.len(), false);
        while self.fish.len() < count {
            let position = Vec2::new(self.random() * size.x, self.random() * size.y);
            let angle = self.random() * std::f32::consts::TAU;
            let phase = self.random() * std::f32::consts::TAU;
            self.fish.push(Fish {
                position,
                velocity: Vec2::new(angle.cos(), angle.sin()) * 65.0,
                phase,
            });
            self.pending_spawn.push(true);
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
        let clearances: Vec<_> = obstacles.iter().map(|&o| icon_clearance(o, size)).collect();
        self.pending_spawn.resize(self.fish.len(), false);
        if self.pending_spawn.iter().any(|&pending| pending) {
            // Spawn zones are deliberately distinct from the much tighter
            // swimming clearances. Never introduce fish inside edge pockets.
            let spawn_zones: Vec<_> = obstacles
                .iter()
                .map(|o| {
                    let mut min = (o.min - Vec2::splat(20.0)).clamp(Vec2::ZERO, size);
                    let mut max = (o.max + Vec2::splat(20.0)).clamp(Vec2::ZERO, size);
                    for axis in 0..2 {
                        if min[axis] < SCREEN_MARGIN {
                            min[axis] = 0.0;
                        }
                        if max[axis] > size[axis] - SCREEN_MARGIN {
                            max[axis] = size[axis];
                        }
                    }
                    Obstacle { min, max }
                })
                .collect();
            for i in 0..self.fish.len() {
                if self.pending_spawn[i] {
                    if let Some(position) =
                        self.spawn_position(self.fish[i].position, size, &spawn_zones)
                    {
                        self.fish[i].position = position;
                        self.pending_spawn[i] = false;
                    }
                }
            }
        }
        // Snapshot positions; spatial buckets avoid scanning the whole school for each fish.
        let old = self.fish.clone();
        let mut grid: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
        let cell = |p: Vec2| ((p.x / 85.0) as i32, (p.y / 85.0) as i32);
        for (i, f) in old.iter().enumerate() {
            grid.entry(cell(f.position)).or_default().push(i);
        }
        // Reuse spatial buckets as distant school summaries. Prefer a group
        // over another isolated fish; never pull all schools to one global center.
        let mut schools: Vec<_> = grid
            .iter()
            .map(|(&key, members)| {
                let mut position = Vec2::ZERO;
                let mut velocity = Vec2::ZERO;
                for &j in members {
                    position += old[j].position;
                    velocity += old[j].velocity;
                }
                (
                    key,
                    position / members.len() as f32,
                    velocity / members.len() as f32,
                    members.len(),
                )
            })
            .collect();
        schools.sort_unstable_by_key(|school| school.0);
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
            let isolation = (1.0 - count / 3.0).clamp(0.0, 1.0);
            if isolation > 0.0 {
                let target = schools
                    .iter()
                    .filter_map(|school| {
                        let mut target = *school;
                        if target.0 == (cx, cy) {
                            if target.3 <= 1 {
                                return None;
                            }
                            // A cell diagonal exceeds the local sight radius.
                            // Retain its other fish, but exclude the caller.
                            let n = target.3 as f32;
                            target.3 -= 1;
                            target.1 = (target.1 * n - f.position) / target.3 as f32;
                            target.2 = (target.2 * n - f.velocity) / target.3 as f32;
                        }
                        Some(target)
                    })
                    .min_by(|a, b| {
                        // First prefer buckets with at least three fish, then
                        // pairs, then singles. Within a class, choose nearest.
                        b.3.min(3).cmp(&a.3.min(3)).then_with(|| {
                            a.1.distance_squared(f.position)
                                .total_cmp(&b.1.distance_squared(f.position))
                        })
                    });
                if let Some(school) = target {
                    let distance = f.position.distance(school.1);
                    let urgency = ((distance - 85.0) / 400.0).clamp(0.0, 1.0);
                    // Anticipate where the moving group will be; accelerate
                    // harder when far away, and soften the pull on approach.
                    let toward = school.1 + school.2 * 0.35 - f.position;
                    let desired =
                        toward.normalize_or_zero() * (120.0 + urgency * (MAX_SPEED - 120.0));
                    force += (desired - f.velocity) * (1.8 + urgency * 1.4) * isolation;
                }
            }
            // Gentle forward assistance in the school, never braking a fast
            // fish or overriding the much stronger avoidance/rejoin forces.
            let cruise_gap = (CRUISE_SPEED - f.velocity.length()).max(0.0);
            force += f.velocity.normalize_or_zero() * cruise_gap * 0.04 * (1.0 - isolation);
            force += pointer_force(f.position, pointer);
            force += cursor_avoidance(f.position, f.velocity, pointer);
            let future = f.position + f.velocity * 0.15;
            // Only the nearest threat steers the fish: neighbouring icon
            // rectangles must not add opposing avoidance forces at a seam.
            if let Some(obstacle) = clearances.iter().min_by(|a, b| {
                future
                    .distance_squared(future.clamp(a.min, a.max))
                    .total_cmp(&future.distance_squared(future.clamp(b.min, b.max)))
            }) {
                let d = future - future.clamp(obstacle.min, obstacle.max);
                if d.length_squared() < 28.0 * 28.0 {
                    let outward = if d.length_squared() > 0.01 {
                        d.normalize()
                    } else {
                        (future - (obstacle.min + obstacle.max) * 0.5).normalize_or_zero()
                    };
                    let proximity = 1.0 - d.length() / 28.0;
                    force += outward * 500.0 * proximity;
                    let mut tangent = Vec2::new(-outward.y, outward.x);
                    let mut preferred = f.velocity;
                    // At corners, prefer sliding toward open screen space.
                    if f.position.x < SCREEN_MARGIN {
                        preferred.x += 60.0;
                    }
                    if f.position.x > size.x - SCREEN_MARGIN {
                        preferred.x -= 60.0;
                    }
                    if f.position.y < SCREEN_MARGIN {
                        preferred.y += 60.0;
                    }
                    if f.position.y > size.y - SCREEN_MARGIN {
                        preferred.y -= 60.0;
                    }
                    if tangent.dot(preferred) < 0.0 {
                        tangent = -tangent;
                    }
                    force += tangent * 220.0 * proximity;
                }
            }
            let margin = SCREEN_MARGIN;
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
            let acceleration =
                NORMAL_ACCELERATION + (REJOIN_ACCELERATION - NORMAL_ACCELERATION) * isolation;
            f.velocity += force.clamp_length_max(acceleration) * dt;
            if f.velocity.length_squared() < 0.001 {
                f.velocity = Vec2::X * 35.0;
            }
            f.velocity = f.velocity.clamp_length(35.0, MAX_SPEED);
            f.position = (f.position + f.velocity * dt).clamp(Vec2::ZERO, size);
            // The selected exit must be inside the screen and outside every
            // connected icon clearance; never project out and clamp back in.
            if let Some(position) = free_projection(f.position, size, &clearances) {
                let normal = (position - f.position).normalize_or_zero();
                f.velocity -= normal * (2.0 * f.velocity.dot(normal).min(0.0));
                f.position = position;
            }
            if let Some(position) =
                cursor_collision(old[i].position, f.position, pointer, size, &clearances)
            {
                let normal = (position - pointer.position).normalize_or_zero();
                let mut tangent = Vec2::new(-normal.y, normal.x);
                if tangent.dot(f.velocity) < 0.0 {
                    tangent = -tangent;
                }
                f.position = position;
                f.velocity = tangent * f.velocity.length().clamp(35.0, MAX_SPEED);
            }
            // Clamping position alone leaves an outward velocity pinned against
            // the wall when cohesion/avoidance overpowers the soft edge force.
            for axis in 0..2 {
                if f.position[axis] <= 0.001 && f.velocity[axis] < 0.0 {
                    f.velocity[axis] = f.velocity[axis].abs().max(35.0);
                } else if f.position[axis] >= size[axis] - 0.001 && f.velocity[axis] > 0.0 {
                    f.velocity[axis] = -f.velocity[axis].abs().max(35.0);
                }
            }
            f.velocity = f.velocity.clamp_length_max(MAX_SPEED);
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
        p.still_seconds = 9.9;
        assert_eq!(pointer_force(Vec2::ZERO, p), Vec2::ZERO);
        p.still_seconds = 10.0;
        assert!(pointer_force(Vec2::ZERO, p).x > 0.0);
        // The delay does not widen the local attraction radius.
        p.position = Vec2::new(200.0, 0.0);
        assert_eq!(pointer_force(Vec2::ZERO, p), Vec2::ZERO);
        p.position = Vec2::new(42.0, 0.0);
        assert_eq!(pointer_force(Vec2::ZERO, p), Vec2::ZERO);
        p.position = Vec2::new(100.0, 0.0);
        p.enabled = false;
        assert_eq!(pointer_force(Vec2::ZERO, p), Vec2::ZERO);
    }
    #[test]
    fn cursor_collision_sweeps_and_respects_delay_and_off_switch() {
        let mut pointer = Pointer {
            position: Vec2::splat(500.0),
            speed: 0.0,
            still_seconds: 10.0,
            enabled: true,
        };
        let start = Vec2::new(400.0, 500.0);
        let end = Vec2::new(600.0, 500.0);
        let hit =
            cursor_collision(start, end, pointer, Vec2::splat(1000.0), &[]).expect("swept hit");
        assert!(hit.x < pointer.position.x);
        assert!(hit.distance(pointer.position) >= CURSOR_RADIUS);
        pointer.still_seconds = 9.9;
        assert!(cursor_collision(start, end, pointer, Vec2::splat(1000.0), &[]).is_none());
        pointer.still_seconds = 10.0;
        pointer.enabled = false;
        assert!(cursor_collision(start, end, pointer, Vec2::splat(1000.0), &[]).is_none());
        pointer.enabled = true;
        pointer.speed = 100.0;
        assert!(cursor_collision(start, end, pointer, Vec2::splat(1000.0), &[]).is_none());
    }
    #[test]
    fn attracted_fish_steers_around_cursor_without_entering_it() {
        let mut school = FishSchool::default();
        school.fish.push(Fish {
            position: Vec2::new(450.0, 500.0),
            velocity: Vec2::X * 150.0,
            phase: 0.0,
        });
        let pointer = Pointer {
            position: Vec2::splat(500.0),
            speed: 0.0,
            still_seconds: 10.0,
            enabled: true,
        };
        for _ in 0..240 {
            school.step(1.0 / 60.0, Vec2::splat(1000.0), &[], pointer);
            let fish = school.fish[0];
            assert!(fish.position.distance(pointer.position) >= CURSOR_RADIUS - 0.001);
            assert!(fish.velocity.length() <= MAX_SPEED + 0.001);
        }
    }
    #[test]
    fn cursor_near_corner_uses_an_in_screen_free_arc() {
        let pointer = Pointer {
            position: Vec2::splat(3.0),
            speed: 0.0,
            still_seconds: 10.0,
            enabled: true,
        };
        let icon = Obstacle {
            min: Vec2::new(0.0, 15.0),
            max: Vec2::new(15.0, 40.0),
        };
        let point = cursor_collision(
            Vec2::ZERO,
            Vec2::new(1.0, 1.0),
            pointer,
            Vec2::splat(1000.0),
            &[icon],
        )
        .expect("free arc");
        assert!(point.cmpge(Vec2::ZERO).all());
        assert!(!contains(icon, point));
        assert!(point.distance(pointer.position) >= CURSOR_RADIUS);
    }
    #[test]
    fn edge_column_does_not_trap_fish_against_screen_boundary() {
        let mut school = FishSchool::default();
        school.fish.push(Fish {
            position: Vec2::new(2.0, 25.0),
            velocity: Vec2::new(-65.0, 0.0),
            phase: 0.0,
        });
        let icons: Vec<_> = (0..8)
            .map(|i| Obstacle {
                min: Vec2::new(0.0, 10.0 + i as f32 * 60.0),
                max: Vec2::new(70.0, 65.0 + i as f32 * 60.0),
            })
            .collect();
        let size = Vec2::new(1000.0, 800.0);
        for _ in 0..600 {
            school.step(1.0 / 60.0, size, &icons, Pointer::default());
            for &icon in &icons {
                assert!(!contains(
                    icon_clearance(icon, size),
                    school.fish[0].position
                ));
            }
        }
        assert!(school.fish[0].position.x > 120.0);
    }
    #[test]
    fn initial_and_added_fish_spawn_outside_icons_and_edge_pockets() {
        let size = Vec2::new(800.0, 600.0);
        let icon = Obstacle {
            min: Vec2::new(50.0, 40.0),
            max: Vec2::new(120.0, 400.0),
        };
        let mut school = FishSchool::default();
        school.resize_population(240, size);
        school.fish[0].position = Vec2::new(8.0, 200.0); // Force the reported bad spawn.
        school.step(0.0, size, &[icon], Pointer::default());
        let first = school.fish[0].position;
        school.resize_population(500, size);
        school.step(0.0, size, &[icon], Pointer::default());
        assert_eq!(
            school.fish[0].position, first,
            "existing fish must not respawn"
        );
        for fish in &school.fish {
            assert!(fish.position.cmpge(Vec2::splat(95.0)).all());
            assert!(fish.position.cmple(size - Vec2::splat(95.0)).all());
            assert!(!contains(icon_clearance(icon, size), fish.position));
            assert!(fish.position.x > 140.0 || fish.position.y > 420.0);
        }
        assert!(school.pending_spawn.iter().all(|&pending| !pending));
    }
    #[test]
    fn swimming_clearance_does_not_fill_wider_edge_gaps() {
        let icon = Obstacle {
            min: Vec2::new(50.0, 40.0),
            max: Vec2::new(120.0, 400.0),
        };
        let clearance = icon_clearance(icon, Vec2::new(800.0, 600.0));
        assert_eq!(clearance.min.x, 42.0);
        assert!(!contains(clearance, Vec2::new(10.0, 200.0)));
    }
    #[test]
    fn overlapping_icon_clearances_choose_an_in_screen_free_exit() {
        let size = Vec2::new(1000.0, 800.0);
        let icons = [
            Obstacle {
                min: Vec2::ZERO,
                max: Vec2::new(70.0, 70.0),
            },
            Obstacle {
                min: Vec2::new(0.0, 60.0),
                max: Vec2::new(90.0, 130.0),
            },
        ]
        .map(|o| icon_clearance(o, size));
        let p = free_projection(Vec2::new(4.0, 65.0), size, &icons).expect("free exit");
        assert!(p.x > 90.0 || p.y > 130.0);
        assert!(p.cmpge(Vec2::ZERO).all() && p.cmple(size).all());
        assert!(icons.iter().all(|&o| !contains(o, p)));
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
    fn separated_fish_accelerates_and_rejoins_a_moving_school() {
        let mut school = FishSchool::default();
        school.fish.push(Fish {
            position: Vec2::new(400.0, 700.0),
            velocity: Vec2::new(-35.0, 0.0),
            phase: 0.0,
        });
        for i in 0..9 {
            school.fish.push(Fish {
                position: Vec2::new(900.0 + (i % 3) as f32 * 22.0, 680.0 + (i / 3) as f32 * 22.0),
                velocity: Vec2::new(80.0, 0.0),
                phase: i as f32,
            });
        }
        let mut peak_speed: f32 = 0.0;
        // Allow more catch-up time under the reduced 150px/s speed cap.
        for _ in 0..600 {
            school.step(
                1.0 / 60.0,
                Vec2::new(3000.0, 1600.0),
                &[],
                Pointer::default(),
            );
            peak_speed = peak_speed.max(school.fish[0].velocity.length());
            for fish in &school.fish {
                assert!(fish.velocity.length() <= MAX_SPEED + 0.001);
            }
        }
        assert!(
            peak_speed > 125.0,
            "a separated fish must exceed the old speed cap"
        );
        let stray = school.fish[0].position;
        let nearest = school.fish[1..]
            .iter()
            .map(|f| stray.distance(f.position))
            .fold(f32::INFINITY, f32::min);
        assert!(nearest < 85.0, "stray did not rejoin: {nearest}");
    }
    #[test]
    fn distant_fish_in_the_same_cell_are_not_ignored() {
        let mut school = FishSchool::default();
        for position in [Vec2::splat(425.0), Vec2::splat(500.0)] {
            school.fish.push(Fish {
                position,
                velocity: Vec2::new(-35.0, 0.0),
                phase: 0.0,
            });
        }
        school.step(1.0 / 30.0, Vec2::splat(2000.0), &[], Pointer::default());
        // Minimum swimming speed preserves magnitude while the fish turns.
        assert!(school.fish[0].velocity.y > 3.0);
        assert!(school.fish[0].velocity.x > -35.0);
    }
    #[test]
    fn cohesive_school_gets_gentle_forward_assistance() {
        let mut school = FishSchool::default();
        for y in -1..=1 {
            for x in -1..=1 {
                school.fish.push(Fish {
                    position: Vec2::new(600.0 + x as f32 * 35.0, 600.0 + y as f32 * 35.0),
                    velocity: Vec2::new(65.0, 0.0),
                    phase: std::f32::consts::FRAC_PI_2,
                });
            }
        }
        school.step(1.0 / 60.0, Vec2::splat(2000.0), &[], Pointer::default());
        let center = school.fish[4];
        assert!(center.velocity.x > 65.001);
        assert!(
            center.velocity.x < 66.0,
            "cruise acceleration should stay gentle"
        );
    }
    #[test]
    fn a_single_fish_has_no_remote_target() {
        let mut school = FishSchool::default();
        school.fish.push(Fish {
            position: Vec2::new(500.0, 500.0),
            velocity: Vec2::new(65.0, 0.0),
            phase: 0.0,
        });
        school.step(1.0 / 60.0, Vec2::splat(2000.0), &[], Pointer::default());
        assert!((school.fish[0].velocity.x - 65.0).abs() < 1.0);
        assert!(school.fish[0].position.is_finite());
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

#[cfg(test)]
mod edge_regression_tests {
    use super::*;
    #[test]
    fn l_shaped_monitor_gaps_do_not_hide_valid_exits() {
        let size = Vec2::splat(1000.0);
        let obstacles = [
            Obstacle {
                min: Vec2::ZERO,
                max: Vec2::new(100.0, 1000.0),
            },
            Obstacle {
                min: Vec2::new(0.0, 900.0),
                max: size,
            },
        ];
        let point = Vec2::new(50.0, 500.0);
        let exit =
            free_projection(point, size, &obstacles).expect("L-shaped union has a free exit");
        assert!(exit.x > 100.0 && exit.y < 900.0);
        assert!(!obstacles.iter().any(|&o| contains(o, exit)));
    }
    #[test]
    fn outward_velocity_at_left_wall_turns_back_into_scene() {
        let mut school = FishSchool::default();
        school.fish.push(Fish {
            position: Vec2::new(0.0, 500.0),
            velocity: Vec2::new(-150.0, 0.0),
            phase: 0.0,
        });
        school.step(1.0 / 60.0, Vec2::splat(1000.0), &[], Pointer::default());
        assert!(school.fish[0].velocity.x > 0.0);
        for _ in 0..60 {
            school.step(1.0 / 60.0, Vec2::splat(1000.0), &[], Pointer::default());
        }
        assert!(school.fish[0].position.x > 20.0);
    }
}
