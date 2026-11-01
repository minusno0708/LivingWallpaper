//! Persistent garden state. Growth uses running application time, not frame count.
use glam::Vec2;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
const MAX_PLANTS: usize = 512;
const WATER_SECONDS: f64 = 8.0 * 3600.0;
#[derive(Clone, Debug)]
pub struct Region {
    pub id: String,
    pub min: Vec2,
    pub max: Vec2,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub enum Kind {
    Grass,
    Flower,
    Tree,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Plant {
    pub display: String,
    pub x: f32,
    pub kind: Kind,
    pub age: f64,
    pub water: f64,
    pub seed: u32,
    generation: u64,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Garden {
    version: u32,
    pub plants: Vec<Plant>,
    pub elapsed: f64,
    rng: u32,
    #[serde(skip)]
    rain_cooldown: f64,
}
impl Default for Garden {
    fn default() -> Self {
        Self {
            version: 1,
            plants: vec![],
            elapsed: 0.0,
            rng: 0x43a2b9c7,
            rain_cooldown: 0.0,
        }
    }
}
/// Integrate the drying curve analytically so a hidden garden and a rendered
/// garden grow identically even with different update intervals.
fn growth(dt: f64, water: f64) -> (f64, f64) {
    let water = water.clamp(0.0, 1.0);
    let wet = dt.min(((water - 0.25) * WATER_SECONDS).max(0.0));
    let low = water.min(0.25);
    let transition = (dt - wet).min(low * WATER_SECONDS);
    let boost = (low * transition - transition * transition / (2.0 * WATER_SECONDS)) / 0.25;
    (
        wet + (dt - wet) * 0.05 + boost * 0.95,
        (water - dt / WATER_SECONDS).max(0.0),
    )
}
impl Garden {
    fn random(&mut self) -> u32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        self.rng
    }
    fn insert(&mut self, id: &str, x: f32, kind: Kind, water: f64) {
        if self.plants.len() >= MAX_PLANTS {
            return;
        }
        let x = x.clamp(0.01, 0.99);
        let spacing = if kind == Kind::Tree { 0.13 } else { 0.013 };
        if self.plants.iter().any(|p| {
            p.display == id
                && (p.kind == Kind::Tree) == (kind == Kind::Tree)
                && (p.x - x).abs() < spacing
        }) {
            return;
        }
        let seed = self.random();
        self.plants.push(Plant {
            display: id.into(),
            x,
            kind,
            age: 0.0,
            water,
            seed,
            generation: 0,
        });
    }
    pub fn advance(&mut self, real_seconds: f64, scale: f32) {
        if !real_seconds.is_finite() || real_seconds <= 0.0 || !scale.is_finite() {
            return;
        }
        let dt = real_seconds * (scale.clamp(0.0, 2.0) as f64);
        if dt <= 0.0 {
            return;
        }
        self.elapsed += dt;
        self.rain_cooldown = (self.rain_cooldown - dt).max(0.0);
        let mut seeds = vec![];
        for p in &mut self.plants {
            let (elapsed, water) = growth(dt, p.water);
            p.age += elapsed;
            p.water = water;
            if p.kind != Kind::Tree {
                // The first seeds appear while the first flower bed is establishing.
                let generation = (p.age / 600.0).floor() as u64;
                if generation > p.generation {
                    p.generation = generation;
                    seeds.push((p.display.clone(), p.x, p.water, p.seed));
                }
            }
        }
        for (id, x, water, seed) in seeds {
            let direction = if self.random() % 2 == 0 { -1.0 } else { 1.0 };
            self.insert(
                &id,
                x + direction * (0.018 + (seed % 17) as f32 * 0.001),
                if seed % 3 == 0 {
                    Kind::Flower
                } else {
                    Kind::Grass
                },
                water,
            );
        }
    }
    pub fn water(&mut self, region: &Region, x: f32, scene_dt: f64) {
        if !scene_dt.is_finite() || scene_dt <= 0.0 {
            return;
        }
        let normalized =
            ((x - region.min.x) / (region.max.x - region.min.x).max(1.0)).clamp(0.0, 1.0);
        let radius = (95.0 / (region.max.x - region.min.x).max(1.0)).clamp(0.03, 0.3);
        for p in &mut self.plants {
            if p.display == region.id && (p.x - normalized).abs() < radius {
                p.water = (p.water + scene_dt * 0.3).min(1.0);
            }
        }
        if self.rain_cooldown <= 0.0 {
            for i in -2..=2 {
                let kind = if (self.random() % 3) == 0 {
                    Kind::Flower
                } else {
                    Kind::Grass
                };
                self.insert(&region.id, normalized + i as f32 * 0.018, kind, 1.0);
            }
            self.insert(&region.id, normalized, Kind::Tree, 1.0);
            self.rain_cooldown = 1.0;
        }
    }
    pub fn reset(&mut self, path: &Path) -> anyhow::Result<()> {
        let empty = Self::default();
        empty.save(path)?;
        *self = empty;
        Ok(())
    }
    pub fn path() -> PathBuf {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("LivingWallpaper/plant-state.json")
    }
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        anyhow::ensure!(
            std::fs::metadata(path)?.len() <= 2 * 1024 * 1024,
            "Plant save is too large"
        );
        let mut garden: Self = serde_json::from_slice(&std::fs::read(path)?)?;
        anyhow::ensure!(garden.version == 1, "Unsupported plant save version");
        anyhow::ensure!(
            garden.plants.len() <= MAX_PLANTS
                && garden.elapsed.is_finite()
                && (0.0..=1.0e12).contains(&garden.elapsed),
            "Invalid plant state"
        );
        for p in &garden.plants {
            anyhow::ensure!(
                p.display.len() <= 256
                    && p.x.is_finite()
                    && (0.0..=1.0).contains(&p.x)
                    && p.age.is_finite()
                    && (0.0..=1.0e12).contains(&p.age)
                    && p.water.is_finite()
                    && (0.0..=1.0).contains(&p.water),
                "Invalid plant data"
            );
        }
        if garden.rng == 0 {
            garden.rng = 0x43a2b9c7;
        }
        garden.rain_cooldown = 0.0;
        Ok(garden)
    }
    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let temporary = path.with_extension("json.tmp");
        std::fs::write(&temporary, serde_json::to_vec(self)?)?;
        std::fs::rename(temporary, path)?;
        Ok(())
    }
}
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    position: [f32; 2],
    color: [f32; 4],
}
fn triangle(out: &mut Vec<Vertex>, a: Vec2, b: Vec2, c: Vec2, color: [f32; 4]) {
    for p in [a, b, c] {
        out.push(Vertex {
            position: p.to_array(),
            color,
        });
    }
}
fn line(out: &mut Vec<Vertex>, a: Vec2, b: Vec2, width: f32, color: [f32; 4]) {
    let n = Vec2::new(-(b - a).y, (b - a).x).normalize_or_zero() * width * 0.5;
    triangle(out, a - n, a + n, b + n, color);
    triangle(out, a - n, b + n, b - n, color);
}
fn ellipse(out: &mut Vec<Vertex>, center: Vec2, radius: Vec2, angle: f32, color: [f32; 4]) {
    let point = |t: f32| {
        let p = Vec2::new(t.cos(), t.sin()) * radius;
        center
            + Vec2::new(
                p.x * angle.cos() - p.y * angle.sin(),
                p.x * angle.sin() + p.y * angle.cos(),
            )
    };
    for i in 0..16 {
        triangle(
            out,
            center,
            point(i as f32 * std::f32::consts::TAU / 16.0),
            point((i + 1) as f32 * std::f32::consts::TAU / 16.0),
            color,
        );
    }
}
fn soil_y(r: &Region, x: f32) -> f32 {
    r.max.y - 20.0 - (x * 5.0 + 0.7).sin() * 3.0
}
#[derive(Debug, PartialEq)]
enum GrowthStage {
    Buried,
    Emerging,
    Cotyledons,
    Young,
    Established,
}
fn growth_stage(age: f64) -> GrowthStage {
    if age < 55.0 {
        GrowthStage::Buried
    } else if age < 60.0 {
        GrowthStage::Emerging
    } else if age < 180.0 {
        GrowthStage::Cotyledons
    } else if age < 300.0 {
        GrowthStage::Young
    } else {
        GrowthStage::Established
    }
}
fn draw_seedling(out: &mut Vec<Vertex>, p: &Plant, r: &Region, time: f32) -> bool {
    let stage = growth_stage(p.age);
    if stage == GrowthStage::Established {
        return false;
    }
    if stage == GrowthStage::Buried {
        return true;
    }
    let base = Vec2::new(r.min.x + p.x * (r.max.x - r.min.x), soil_y(r, p.x));
    let height = ((p.age - 55.0) / 5.0).clamp(0.0, 1.0) as f32 * 5.0
        + ((p.age - 60.0) / 240.0).clamp(0.0, 1.0) as f32 * 7.0;
    let tip = base
        + Vec2::new(
            (time * 1.2 + (p.seed % 100) as f32).sin() * height * 0.04,
            -height,
        );
    line(out, base, tip, 1.1, [0.29, 0.57, 0.31, 1.0]);
    let opening = ((p.age - 60.0) / 15.0).clamp(0.0, 1.0) as f32;
    if opening > 0.0 {
        for side in [-1.0, 1.0] {
            ellipse(
                out,
                tip + Vec2::new(side * 3.1 * opening, -0.3),
                Vec2::new(4.0 * opening, 1.8 * opening),
                -side * 0.25,
                [0.38, 0.66, 0.36, 1.0],
            );
        }
    }
    if stage == GrowthStage::Young {
        let opening = ((p.age - 180.0) / 120.0).clamp(0.0, 1.0) as f32;
        for side in [-1.0, 1.0] {
            ellipse(
                out,
                base.lerp(tip, 0.55) + Vec2::new(side * 3.0 * opening, 0.0),
                Vec2::new(3.8 * opening, 1.2 * opening),
                -side * 0.6,
                [0.23, 0.51, 0.3, 1.0],
            );
        }
    }
    true
}
fn rain_origin(region: &Region, cursor: Vec2) -> Vec2 {
    Vec2::new(
        cursor.x,
        region.min.y + 48.0_f32.min((region.max.y - region.min.y) * 0.12),
    )
}
fn soil_water(garden: &Garden, region: &Region, x: f32) -> f32 {
    garden
        .plants
        .iter()
        .filter(|p| p.display == region.id)
        .map(|p| {
            let distance = ((p.x - x) * (region.max.x - region.min.x)).abs();
            let fade = (1.0 - distance / 95.0).clamp(0.0, 1.0);
            p.water as f32 * fade * fade * (3.0 - 2.0 * fade)
        })
        .fold(0.0, f32::max)
}
fn soil_color(wet: f32) -> [f32; 4] {
    [
        0.08 * (1.0 - wet * 0.65),
        0.12 * (1.0 - wet * 0.58),
        0.09 * (1.0 - wet * 0.45),
        1.0,
    ]
}
pub fn vertices(garden: &Garden, regions: &[Region], rain: Option<Vec2>) -> Vec<Vertex> {
    let mut out = vec![];
    let time = (garden.elapsed % 3600.0) as f32;
    for r in regions {
        // A quiet blue-green sky and a thin, gently uneven strip of ground.
        for (position, color) in [
            (r.min, [0.015, 0.04, 0.055, 1.0]),
            (Vec2::new(r.max.x, r.min.y), [0.015, 0.04, 0.055, 1.0]),
            (r.max, [0.055, 0.12, 0.095, 1.0]),
            (r.min, [0.015, 0.04, 0.055, 1.0]),
            (r.max, [0.055, 0.12, 0.095, 1.0]),
            (Vec2::new(r.min.x, r.max.y), [0.055, 0.12, 0.095, 1.0]),
        ] {
            out.push(Vertex {
                position: position.to_array(),
                color,
            });
        }
        for i in 0..64 {
            let a = i as f32 / 64.0;
            let b = (i + 1) as f32 / 64.0;
            let x1 = r.min.x + a * (r.max.x - r.min.x);
            let x2 = r.min.x + b * (r.max.x - r.min.x);
            let p = Vec2::new(x1, soil_y(r, a));
            let q = Vec2::new(x2, soil_y(r, b));
            let wet_a = soil_water(garden, r, a);
            let wet_b = soil_water(garden, r, b);
            let ca = soil_color(wet_a);
            let cb = soil_color(wet_b);
            for (position, color) in [
                (p, ca),
                (q, cb),
                (Vec2::new(x2, r.max.y), cb),
                (p, ca),
                (Vec2::new(x2, r.max.y), cb),
                (Vec2::new(x1, r.max.y), ca),
            ] {
                out.push(Vertex {
                    position: position.to_array(),
                    color,
                });
            }
            let wet = (wet_a + wet_b) * 0.5;
            line(
                &mut out,
                p,
                q,
                1.5,
                [0.2 - wet * 0.08, 0.34 - wet * 0.07, 0.22 + wet * 0.06, 1.0],
            );
        }
        // Original simple green trees, with a fixed number of branches and crowns.
        for p in garden
            .plants
            .iter()
            .filter(|p| p.display == r.id && p.kind == Kind::Tree)
        {
            if draw_seedling(&mut out, p, r, time) {
                continue;
            }
            let growth = ((p.age - 300.0) / (86400.0 - 300.0)).clamp(0.0, 1.0) as f32;
            let height = (r.max.y - r.min.y)
                * 0.32
                * growth.sqrt()
                * (0.85 + (p.seed % 100) as f32 * 0.0015)
                + 12.0;
            let base = Vec2::new(r.min.x + p.x * (r.max.x - r.min.x), soil_y(r, p.x));
            let top = base
                + Vec2::new(
                    (time * 0.55 + p.seed as f32).sin() * height * 0.012,
                    -height,
                );
            line(
                &mut out,
                base,
                top,
                1.2 + growth * 7.0,
                [0.27, 0.25, 0.16, 1.0],
            );
            for branch in 0..7 {
                let k = branch as f32;
                let side = if branch % 2 == 0 { -1.0 } else { 1.0 };
                let root = base.lerp(top, 0.3 + k * 0.09);
                let end = root + Vec2::new(side * height * (0.25 - k * 0.018), -height * 0.17);
                line(
                    &mut out,
                    root,
                    end,
                    0.8 + growth * 3.0,
                    [0.3, 0.28, 0.18, 1.0],
                );
                if growth > 0.02 {
                    for leaf in 0..3 {
                        let offset =
                            Vec2::new((leaf as f32 - 1.0) * height * 0.065, -height * 0.035);
                        ellipse(
                            &mut out,
                            end + offset,
                            Vec2::new(height * 0.15, height * 0.12),
                            side * 0.2,
                            [0.12 + k * 0.008, 0.33 + leaf as f32 * 0.025, 0.22, 0.96],
                        );
                    }
                }
            }
            ellipse(
                &mut out,
                top,
                Vec2::new(5.0 + height * 0.12, 3.0 + height * 0.09),
                0.0,
                [0.19, 0.42, 0.28, 1.0],
            );
        }
        for p in garden
            .plants
            .iter()
            .filter(|p| p.display == r.id && p.kind != Kind::Tree)
        {
            if draw_seedling(&mut out, p, r, time) {
                continue;
            }
            let growth = ((p.age - 300.0) / (3600.0 - 300.0)).clamp(0.0, 1.0) as f32;
            let height = 12.0 + (20.0 + (p.seed % 30) as f32) * growth.sqrt();
            let base = Vec2::new(r.min.x + p.x * (r.max.x - r.min.x), soil_y(r, p.x));
            let tip = base
                + Vec2::new(
                    (time * 1.2 + (p.seed % 100) as f32).sin() * height * 0.07,
                    -height,
                );
            line(&mut out, base, tip, 1.0 + growth, [0.24, 0.51, 0.3, 1.0]);
            for side in [-1.0, 1.0] {
                ellipse(
                    &mut out,
                    base.lerp(tip, 0.5) + Vec2::new(side * height * 0.13, 0.0),
                    Vec2::new(height * 0.24, height * 0.065),
                    -side * 0.6,
                    [0.25, 0.52, 0.32, 1.0],
                );
            }
            if p.kind == Kind::Flower && p.age > 1200.0 {
                // Flowers renew in overlapping generations; green foliage stays.
                let cycle = p.age % 21600.0;
                let bloom = if p.age < 21600.0 {
                    ((p.age - 1200.0) / 2400.0).clamp(0.0, 1.0) as f32
                } else {
                    0.65 + 0.35 * (cycle / 3600.0).min(1.0) as f32
                };
                let colors = [
                    [0.85, 0.64, 0.57, 1.0],
                    [0.79, 0.77, 0.55, 1.0],
                    [0.6, 0.64, 0.82, 1.0],
                ];
                for petal in 0..5 {
                    let angle = petal as f32 * std::f32::consts::TAU / 5.0;
                    ellipse(
                        &mut out,
                        tip + Vec2::new(angle.cos(), angle.sin()) * 3.5 * bloom,
                        Vec2::new(4.0 * bloom, 2.7 * bloom),
                        angle,
                        colors[(p.seed as usize + (p.age / 21600.0) as usize) % 3],
                    );
                }
                ellipse(
                    &mut out,
                    tip,
                    Vec2::splat(2.0 * bloom),
                    0.0,
                    [0.87, 0.68, 0.27, 1.0],
                );
            }
        }
        if let Some(cursor) = rain.filter(|p| p.cmpge(r.min).all() && p.cmple(r.max).all()) {
            let origin = rain_origin(r, cursor);
            let ground = soil_y(r, (cursor.x - r.min.x) / (r.max.x - r.min.x));
            for i in 0..70 {
                let k = i as f32;
                let phase = (time * 280.0 / (ground - origin.y).max(1.0) + k * 0.618).fract();
                let x = cursor.x + (k * 13.7).sin() * 70.0;
                let y = origin.y + phase * (ground - origin.y).max(0.0);
                if x >= r.min.x && x <= r.max.x && y < ground {
                    line(
                        &mut out,
                        Vec2::new(x, y),
                        Vec2::new(x - 0.7, (y + 9.0).min(ground)),
                        0.55,
                        [0.3, 0.48, 0.53, 0.28],
                    );
                }
            }
            for i in 0..5 {
                ellipse(
                    &mut out,
                    origin + Vec2::new((i as f32 - 2.0) * 18.0, -18.0 - (i % 2) as f32 * 5.0),
                    Vec2::new(24.0, 13.0),
                    0.0,
                    [0.22, 0.34, 0.37, 0.8],
                );
            }
        }
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    fn region() -> Region {
        Region {
            id: "test".into(),
            min: Vec2::ZERO,
            max: Vec2::new(1000.0, 700.0),
        }
    }
    #[test]
    fn rain_is_at_top_of_the_cursors_monitor_and_soil_tracks_moisture() {
        let mut region = region();
        region.min.y = -500.0;
        let cursor = Vec2::new(450.0, 300.0);
        assert_eq!(rain_origin(&region, cursor), Vec2::new(450.0, -452.0));
        assert_eq!(rain_origin(&region, Vec2::new(450.0, 0.0)).y, -452.0);
        let mut garden = Garden::default();
        assert_eq!(soil_water(&garden, &region, 0.45), 0.0);
        garden.water(&region, 450.0, 1.0);
        let wet = soil_water(&garden, &region, 0.45);
        assert!(wet > 0.9);
        assert_eq!(soil_water(&garden, &region, 0.95), 0.0);
        assert!(soil_color(wet)[0] < soil_color(0.0)[0]);
        garden.advance(8.0 * 3600.0, 1.0);
        assert_eq!(soil_water(&garden, &region, 0.45), 0.0);
    }
    #[test]
    fn germination_waits_one_minute_and_opens_cotyledons_before_true_leaves() {
        assert_eq!(growth_stage(0.0), GrowthStage::Buried);
        assert_eq!(growth_stage(54.9), GrowthStage::Buried);
        assert_eq!(growth_stage(59.0), GrowthStage::Emerging);
        assert_eq!(growth_stage(60.0), GrowthStage::Cotyledons);
        assert_eq!(growth_stage(180.0), GrowthStage::Young);
        assert_eq!(growth_stage(300.0), GrowthStage::Established);
        let mut garden = Garden::default();
        garden.water(&region(), 500.0, 1.0);
        let bare = vertices(&Garden::default(), &[region()], None).len();
        assert_eq!(vertices(&garden, &[region()], None).len(), bare);
        garden.advance(60.0, 1.0);
        assert_eq!(growth_stage(garden.plants[0].age), GrowthStage::Cotyledons);
        garden.advance(10.0, 1.0);
        assert!(vertices(&garden, &[region()], None).len() > bare);
    }
    #[test]
    fn wet_and_dry_growth_and_hidden_time_are_frame_independent() {
        let (a, w) = growth(100.0, 1.0);
        assert!((a - 100.0).abs() < 1e-6);
        assert!(w < 1.0);
        assert!((growth(100.0, 0.0).0 - 5.0).abs() < 1e-6);
        let mut m = 1.0;
        let mut age = 0.0;
        for _ in 0..100 {
            let (t, next) = growth(360.0, m);
            age += t;
            m = next;
        }
        let whole = growth(36000.0, 1.0);
        assert!((age - whole.0).abs() < 1e-6);
        assert!((m - whole.1).abs() < 1e-6);
    }
    #[test]
    fn zero_scale_stops_and_rain_restores_water() {
        let mut g = Garden::default();
        g.water(&region(), 500.0, 1.0);
        assert!(!g.plants.is_empty());
        g.advance(3600.0, 0.0);
        assert_eq!(g.elapsed, 0.0);
        g.advance(3600.0, 2.0);
        assert_eq!(g.elapsed, 7200.0);
        for p in &mut g.plants {
            p.water = 0.0;
        }
        g.water(&region(), 500.0, 4.0);
        assert!(g.plants.iter().any(|p| p.water > 0.9));
    }
    #[test]
    fn seeds_spread_without_clearing_existing_plants() {
        let mut g = Garden::default();
        g.water(&region(), 500.0, 1.0);
        let count = g.plants.len();
        g.advance(3601.0, 1.0);
        assert!(g.plants.len() > count);
        assert!(g.plants.iter().any(|p| p.kind == Kind::Tree));
    }
    #[test]
    fn wet_tree_reaches_maturity_in_24_hours_and_population_is_bounded() {
        let mut garden = Garden::default();
        garden.water(&region(), 500.0, 1.0);
        for _ in 0..24 {
            for p in &mut garden.plants {
                p.water = 1.0;
            }
            garden.advance(3600.0, 1.0);
        }
        assert!(
            (garden
                .plants
                .iter()
                .find(|p| p.kind == Kind::Tree)
                .unwrap()
                .age
                - 86400.0)
                .abs()
                < 0.001
        );
        for i in 0..1000 {
            garden.insert(&format!("test-{i}"), 0.5, Kind::Grass, 1.0);
        }
        assert_eq!(garden.plants.len(), MAX_PLANTS);
        assert!(vertices(&garden, &[region()], None)
            .iter()
            .all(|v| v.position.iter().all(|f| f.is_finite())));
    }
    #[test]
    fn save_load_has_no_offline_catchup_and_rejects_bad_data() {
        let mut g = Garden::default();
        g.water(&region(), 500.0, 1.0);
        g.advance(100.0, 1.0);
        let root = std::env::temp_dir().join(format!(
            "living-wallpaper-plant-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("state.json");
        g.save(&path).unwrap();
        g.save(&path).unwrap();
        let loaded = Garden::load(&path).unwrap();
        assert_eq!(loaded.elapsed, g.elapsed);
        assert_eq!(loaded.plants[0].age, g.plants[0].age);
        g.reset(&path).unwrap();
        assert!(g.plants.is_empty());
        assert_eq!(g.elapsed, 0.0);
        assert!(Garden::load(&path).unwrap().plants.is_empty());

        std::fs::write(&path, b"{}").unwrap();
        assert!(Garden::load(&path).is_err());
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}
