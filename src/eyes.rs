//! CPU scheduling for the Eyes scene; all timing is in scene seconds.
use crate::simulation::Pointer;
use glam::Vec2;
#[derive(Clone, Copy, Debug)]
pub struct Eye {
    pub center: Vec2,
    pub size: Vec2,
    pub gaze: Vec2,
    pub openness: f32,
    pub seed: f32,
    age: f32,
    life: f32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum Event {
    Normal,
    Together,
    Giant,
    Cursor,
}
#[derive(Clone, Copy, Debug)]
enum Phase {
    Normal,
    Closing(Event),
    Black(Event),
    Special(Event),
}
pub struct EyesScene {
    pub eyes: Vec<Eye>,
    phase: Phase,
    timer: f32,
    spawn: f32,
    seed: u32,
    gaze_mode: u32,
    gaze: Vec2,
    gaze_timer: f32,
    cursor_used: bool,
    dart_timer: f32,
}
impl Default for EyesScene {
    fn default() -> Self {
        Self {
            eyes: vec![],
            phase: Phase::Normal,
            timer: 18.0,
            spawn: 0.0,
            seed: 0x9182abcd,
            gaze_mode: 0,
            gaze: Vec2::ZERO,
            gaze_timer: 0.0,
            cursor_used: false,
            dart_timer: 0.0,
        }
    }
}
impl EyesScene {
    fn random(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        self.seed as f32 / u32::MAX as f32
    }
    fn eye(&mut self, center: Vec2, size: Vec2, life: f32) {
        let seed = self.random();
        self.eyes.push(Eye {
            center,
            size,
            gaze: Vec2::ZERO,
            openness: 0.0,
            seed,
            age: 0.0,
            life,
        });
    }
    fn random_eye(&mut self, regions: &[(Vec2, Vec2)], life: f32) {
        if regions.is_empty() {
            return;
        }
        let index = (self.random() * regions.len() as f32) as usize % regions.len();
        let (min, max) = regions[index];
        let width = (100.0 + self.random() * 140.0).min((max.x - min.x) * 0.65);
        let size = Vec2::new(width, width * 0.48);
        let inset = size * 0.6;
        let room = (max - min - inset * 2.0).max(Vec2::ZERO);
        let center = min + inset + Vec2::new(self.random(), self.random()) * room;
        self.eye(center, size, life);
    }
    fn begin_close(&mut self, event: Event) {
        self.phase = Phase::Closing(event);
        for e in &mut self.eyes {
            e.life = e.life.min(e.age + 0.35);
        }
    }
    pub fn step(&mut self, dt: f32, regions: &[(Vec2, Vec2)], pointer: Pointer) {
        let dt = dt.clamp(0.0, 0.1);
        if regions.is_empty() {
            self.eyes.clear();
            return;
        }
        if !pointer.enabled || pointer.speed > 2.0 || pointer.still_seconds < 10.0 {
            self.cursor_used = false;
        }
        self.timer -= dt;
        self.spawn -= dt;
        self.gaze_timer -= dt;
        if self.gaze_timer <= 0.0 {
            self.gaze_mode = (self.random() * 3.0) as u32;
            self.gaze = Vec2::new(self.random() * 2.0 - 1.0, self.random() * 1.2 - 0.6);
            self.gaze_timer = 2.0 + self.random() * 4.0;
        }
        self.dart_timer -= dt;
        if self.gaze_mode == 2 && self.dart_timer <= 0.0 {
            self.gaze = Vec2::new(self.random() * 1.5 - 0.75, self.random() * 1.0 - 0.5);
            self.dart_timer = 0.15 + self.random() * 0.65;
        }
        match self.phase {
            Phase::Normal => {
                if pointer.enabled
                    && pointer.still_seconds >= 10.0
                    && pointer.speed <= 2.0
                    && !self.cursor_used
                {
                    self.cursor_used = true;
                    self.begin_close(Event::Cursor);
                } else if self.timer <= 0.0 {
                    let event = if self.random() < 0.6 {
                        Event::Together
                    } else {
                        Event::Giant
                    };
                    self.begin_close(event);
                } else if self.spawn <= 0.0 && self.eyes.len() < 14 {
                    let life = 3.0 + self.random() * 6.0;
                    self.random_eye(regions, life);
                    self.spawn = 0.3 + self.random() * 1.1;
                }
            }
            Phase::Closing(event) => {
                if self.eyes.is_empty() {
                    self.phase = Phase::Black(event);
                    self.timer = 1.0 + self.random() * 2.0;
                }
            }
            Phase::Black(event) => {
                if self.timer <= 0.0 {
                    let event = if event == Event::Cursor
                        && (!pointer.enabled || pointer.speed > 2.0 || pointer.still_seconds < 10.0)
                    {
                        Event::Normal
                    } else {
                        event
                    };
                    let life = 4.0 + self.random() * 3.0;
                    match event {
                        Event::Together => {
                            for _ in 0..12 {
                                self.random_eye(regions, life);
                            }
                        }
                        Event::Giant => {
                            let index =
                                (self.random() * regions.len() as f32) as usize % regions.len();
                            let (min, max) = regions[index];
                            let size = max - min;
                            self.eye(
                                (min + max) * 0.5,
                                Vec2::new(size.x * 0.96, size.y * 0.92),
                                life,
                            );
                        }
                        Event::Cursor => {
                            for i in 0..8 {
                                let angle = i as f32 * std::f32::consts::TAU / 8.0;
                                let center =
                                    pointer.position + Vec2::new(angle.cos(), angle.sin()) * 180.0;
                                if regions.iter().any(|(min, max)| {
                                    center.cmpge(*min + Vec2::new(65.0, 35.0)).all()
                                        && center.cmple(*max - Vec2::new(65.0, 35.0)).all()
                                }) {
                                    self.eye(center, Vec2::new(120.0, 58.0), life);
                                }
                            }
                        }
                        Event::Normal => {}
                    }
                    self.phase = Phase::Special(event);
                    self.timer = life;
                }
            }
            Phase::Special(event) => {
                if event == Event::Cursor
                    && (!pointer.enabled || pointer.speed > 2.0 || pointer.still_seconds < 10.0)
                {
                    self.begin_close(Event::Normal);
                } else if self.timer <= 0.0 && self.eyes.is_empty() {
                    self.phase = Phase::Normal;
                    self.timer = 14.0 + self.random() * 16.0;
                    self.spawn = 0.0;
                }
            }
        }
        for eye in &mut self.eyes {
            eye.age += dt;
            let opening = (eye.age / 0.22).min(((eye.life - eye.age) / 0.35).clamp(0.0, 1.0));
            eye.openness = opening * opening * (3.0 - 2.0 * opening);
            let cursor = matches!(self.phase, Phase::Special(Event::Cursor));
            let desired = if cursor || (self.gaze_mode == 1 && pointer.enabled) {
                (pointer.position - eye.center) / eye.size * 1.4
            } else if self.gaze_mode == 2 {
                self.gaze
            } else {
                Vec2::ZERO
            };
            let desired = desired.clamp_length_max(0.75);
            eye.gaze = eye.gaze.lerp(desired, 1.0 - (-dt * 9.0).exp());
        }
        self.eyes.retain(|eye| eye.age < eye.life);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn events_wait_for_closed_eyes_and_black_interval() {
        let mut scene = EyesScene::default();
        let regions = [(Vec2::ZERO, Vec2::new(1920.0, 1080.0))];
        scene.step(0.016, &regions, Pointer::default());
        assert!(!scene.eyes.is_empty());
        scene.begin_close(Event::Together);
        for _ in 0..30 {
            scene.step(0.016, &regions, Pointer::default());
        }
        assert!(scene.eyes.is_empty());
        assert!(matches!(scene.phase, Phase::Black(_)));
        for _ in 0..200 {
            scene.step(0.016, &regions, Pointer::default());
        }
        assert!(matches!(scene.phase, Phase::Special(Event::Together)));
        assert_eq!(scene.eyes.len(), 12);
        assert!(scene.eyes.iter().all(|e| e.age == scene.eyes[0].age));
    }
    #[test]
    fn cursor_event_requires_stillness_and_cancels_on_motion() {
        let mut s = EyesScene::default();
        let regions = [(Vec2::ZERO, Vec2::splat(1000.0))];
        let mut p = Pointer {
            position: Vec2::splat(500.0),
            enabled: true,
            still_seconds: 10.0,
            ..Default::default()
        };
        s.step(0.016, &regions, p);
        assert!(matches!(s.phase, Phase::Closing(Event::Cursor)));
        for _ in 0..230 {
            s.step(0.016, &regions, p);
        }
        assert!(matches!(s.phase, Phase::Special(Event::Cursor)));
        assert_eq!(s.eyes.len(), 8);
        p.speed = 100.0;
        s.step(0.016, &regions, p);
        assert!(matches!(s.phase, Phase::Closing(Event::Normal)));
    }
}
