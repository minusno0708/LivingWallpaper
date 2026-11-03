#![cfg_attr(
    all(windows, not(debug_assertions), not(test)),
    windows_subsystem = "windows"
)]
mod debug_options;
mod plant;
mod platform;
mod renderer;
mod scene_time;
mod settings;
mod simulation;
use anyhow::Context;
use glam::Vec2;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use simulation::{Pointer, SchoolModel};
use std::{
    io::Write,
    sync::{mpsc, Arc, Mutex},
    time::{Duration, Instant},
};
use winit::{
    event::{ElementState, Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    keyboard::{Key, NamedKey},
    window::WindowBuilder,
};

fn log(message: impl AsRef<str>) {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("LivingWallpaper");
    let _ = std::fs::create_dir_all(&base);
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(base.join("app.log"))
    {
        let _ = writeln!(f, "{:?} {}", std::time::SystemTime::now(), message.as_ref());
    }
}
fn main() {
    std::panic::set_hook(Box::new(|info| {
        log(format!("PANIC {info}"));
    }));
    if let Err(error) = run() {
        log(format!("ERROR {error:#}"));
        eprintln!("{error:#}");
        #[cfg(windows)]
        if !std::env::args().any(|a| a == "--smoke-test") {
            unsafe {
                use windows_sys::Win32::UI::WindowsAndMessaging::*;
                let message:Vec<u16>=format!("{error:#}\n\nTry --preview to test rendering. Log: %LOCALAPPDATA%\\LivingWallpaper\\app.log").encode_utf16().chain(Some(0)).collect();
                let title: Vec<u16> = "Living Wallpaper".encode_utf16().chain(Some(0)).collect();
                MessageBoxW(0, message.as_ptr(), title.as_ptr(), MB_OK | MB_ICONERROR);
            }
        }
        std::process::exit(1);
    }
}
fn create_model(mode: settings::ModelMode) -> Option<Box<dyn SchoolModel>> {
    match mode {
        settings::ModelMode::Fish => Some(Box::<simulation::FishSchool>::default()),
        settings::ModelMode::Plant => None,
    }
}
fn run() -> anyhow::Result<()> {
    let _instance = platform::single_instance()?;
    let args: Vec<_> = std::env::args().collect();
    debug_options::configure(debug_options::parse(&args)?);
    let preview = args.iter().any(|a| a == "--preview");
    let smoke = args.iter().any(|a| a == "--smoke-test");
    let event_loop = EventLoop::new()?;
    let mut builder = WindowBuilder::new()
        .with_title("Living Wallpaper")
        .with_inner_size(winit::dpi::PhysicalSize::new(1100, 700));
    if !preview {
        builder = builder.with_decorations(false).with_visible(false);
    }
    let window = Arc::new(builder.build(&event_loop)?);
    let hwnd = match window.window_handle()?.as_raw() {
        RawWindowHandle::Win32(h) => h.hwnd.get(),
        _ => 0,
    };
    if !preview {
        platform::attach(hwnd).context("Windows wallpaper integration failed")?;
    }
    let settings = Arc::new(Mutex::new(settings::Settings::load()));
    let mut display_gaps = if preview {
        vec![]
    } else {
        platform::configure_displays(hwnd, &settings.lock().unwrap().disabled_displays)?
    };
    let mut display_snapshot = platform::displays();
    let mut renderer = pollster::block_on(renderer::Renderer::new(window.clone()))?;
    log(format!(
        "START preview={preview} GPU={}",
        renderer.adapter_name
    ));
    let (tx, rx) = mpsc::channel();
    let _tray = platform::start_tray(tx, settings.clone())?;
    if debug_options::enabled() {
        settings.lock().unwrap().mode = settings::ModelMode::Plant;
    }
    let mut model = create_model(settings.lock().unwrap().mode);
    let plant_path = if debug_options::enabled() {
        plant::Garden::path().with_file_name("plant-debug-state.json")
    } else {
        plant::Garden::path()
    };
    let mut garden = if preview {
        plant::Garden::default()
    } else {
        plant::Garden::load(&plant_path)
            .context("Could not read plant-state.json; the saved garden was not overwritten")?
    };
    let mut garden_dirty = false;
    let mut last_growth = Instant::now();
    let mut last_plant_save = Instant::now();
    let mut pointer = Pointer::default();
    let mut previous_cursor = None;
    let mut obstacles = Vec::new();
    let mut last_icons = Instant::now() - Duration::from_secs(3);
    let start = Instant::now();
    let mut scene_clock = scene_time::SceneClock::default();
    let mut last_frame = Instant::now();
    let mut next_frame = Instant::now();
    let mut last_health = Instant::now();
    let mut frames = 0u64;
    let mut last_icon_error = String::new();
    let smoke_frames = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let smoke_result = smoke_frames.clone();
    event_loop.run(move |event,target| {
        if let Event::WindowEvent { window_id,event }=&event { if *window_id==window.id() {
            match event {
                WindowEvent::CloseRequested=>target.exit(),
                WindowEvent::Resized(s)=>renderer.resize(s.width,s.height),
                WindowEvent::KeyboardInput { event,.. } if event.state==ElementState::Pressed && event.logical_key==Key::Named(NamedKey::Escape)=>target.exit(),
                _=>{}
            }
        }}
        if let Event::AboutToWait=event {
            let now=Instant::now();
            // Integrate before commands, using the multiplier that applied during
            // the elapsed interval. Visibility and rendering pauses do not enter here.
            let previous=settings.lock().unwrap().clone();
            if previous.mode==settings::ModelMode::Plant {
                garden.advance(now.duration_since(last_growth).as_secs_f64()*debug_options::effective_scale(&previous),1.0);
                garden_dirty |= debug_options::effective_scale(&previous)>0.0 && !garden.plants.is_empty();
            }
            last_growth=now;
            if !preview && garden_dirty && last_plant_save.elapsed()>=Duration::from_secs(30) {
                match garden.save(&plant_path) {Ok(())=>garden_dirty=false,Err(e)=>log(format!("Garden save failed: {e}"))}
                last_plant_save=now;
            }
            for command in rx.try_iter() {
                use platform::Command::*; let mut s=settings.lock().unwrap();
                match command { Mouse=>s.mouse=!s.mouse,Icons=>s.icons=!s.icons,Power=>s.power_saving=!s.power_saving,Count(n)=>s.fish_count=n,Fps(n)=>s.fps=n,TimeScale(value)=>{if debug_options::active(s.mode){debug_options::set_speed(value as u32);}else{s.time_scale=settings::normalize_time_scale(value);}},Monitor(id)=>{if s.disabled_displays.contains(&id) {s.disabled_displays.retain(|value| value!=&id);} else {s.disabled_displays.push(id);} if !preview {match platform::configure_displays(hwnd,&s.disabled_displays) {Ok(gaps)=>display_gaps=gaps,Err(e)=>log(e.to_string())}}},Mode(mode)=>{if s.mode!=mode {s.mode=mode;model=create_model(mode);}},ResetGarden=>{let result=if preview {garden=plant::Garden::default();Ok(())}else{garden.reset(&plant_path)};match result {Ok(())=>{garden_dirty=false;},Err(e)=>log(format!("Garden reset failed: {e}"))}},Quit=>target.exit() }
                if let Err(e)=s.save() { log(format!("Settings save failed: {e}")); }
                next_frame=now;
            }
            if smoke && start.elapsed()>Duration::from_secs(8) { smoke_frames.store(frames,std::sync::atomic::Ordering::Relaxed); log(format!("SMOKE {} frames={frames} icons={} GPU={}",if frames>0 {"PASS"} else {"FAIL"},obstacles.len(),renderer.adapter_name)); target.exit(); }
            if !preview && last_health.elapsed()>Duration::from_secs(2) {
                last_health=now;
                let current_displays=platform::displays();
                if current_displays!=display_snapshot {
                    display_snapshot=current_displays;
                    match platform::configure_displays(hwnd,&settings.lock().unwrap().disabled_displays) {Ok(gaps)=>display_gaps=gaps,Err(e)=>log(e.to_string())}
                }
                if !platform::healthy(hwnd) { log("Explorer wallpaper host lost; exiting. Restart app after Explorer is ready."); target.exit(); }
            }
            let s=settings.lock().unwrap().clone();
            if now>=next_frame {
                let size=window.inner_size(); let bounds=Vec2::new(size.width as f32,size.height as f32);
                let env=platform::environment(hwnd);
                let paused=(!preview && display_snapshot.iter().all(|d|s.disabled_displays.contains(&d.id))) || s.fps==0 || debug_options::effective_scale(&s)==0.0 || size.width==0 || size.height==0 || (s.power_saving && (env.suspended || (!preview && env.fullscreen)));
                let fps=if paused {2} else if s.power_saving && env.battery {s.fps.min(15)} else {s.fps};
                let dt=now.duration_since(last_frame).as_secs_f32(); last_frame=now;
                if let Some(cursor)=env.cursor {
                    let distance=previous_cursor.map(|p:Vec2| p.distance(cursor)).unwrap_or(0.0);
                    pointer.speed=distance/dt.max(0.001);
                    pointer.still_seconds=if distance<2.0 {pointer.still_seconds+dt.min(0.5)} else {0.0};
                    pointer.position=cursor; previous_cursor=Some(cursor);
                    pointer.enabled=s.mouse && cursor.cmpge(Vec2::ZERO).all() && cursor.cmple(bounds).all();
                } else { pointer.enabled=false; }
                let plan=scene_clock.advance(dt,s.time_scale,paused);
                if !paused {
                    if model.is_some() && s.icons && !preview && last_icons.elapsed()>Duration::from_secs(2) {
                        last_icons=now;
                        match platform::icons(hwnd) { Ok(rects)=>{ obstacles=rects; },Err(e)=>{obstacles.clear();let message=e.to_string();if message!=last_icon_error {log(format!("Icon avoidance unavailable: {message}"));last_icon_error=message;} } }
                    }
                    let mut scene_obstacles=display_gaps.clone();
                    if s.icons {scene_obstacles.extend_from_slice(&obstacles);}
                    let rendered=if let Some(model)=model.as_mut() {
                        model.resize_population(s.fish_count,bounds);
                        for _ in 0..plan.steps {model.step(plan.step_dt,bounds,&scene_obstacles,pointer);}
                        renderer.render(model.fish(),scene_clock.seconds())
                    } else {
                        let regions:Vec<plant::Region>=if preview {vec![plant::Region {id:"preview".into(),min:Vec2::ZERO,max:bounds}]} else {
                            let left=display_snapshot.iter().map(|d|d.x).min().unwrap_or(0);
                            let top=display_snapshot.iter().map(|d|d.y).min().unwrap_or(0);
                            display_snapshot.iter().filter(|d|!s.disabled_displays.contains(&d.id)).map(|d|{
                                let min=Vec2::new((d.x-left) as f32,(d.y-top) as f32);
                                plant::Region {id:d.id.clone(),min,max:Vec2::new(min.x+d.width as f32,(d.work_bottom-top) as f32)}
                            }).collect()
                        };
                        let rain=if env.water_held && pointer.enabled {Some(pointer.position)}else{None};
                        if let Some(cursor)=rain {
                            if let Some(region)=regions.iter().find(|r|cursor.cmpge(r.min).all()&&cursor.cmple(r.max).all()) {
                                garden.water(region,cursor.x,dt as f64*debug_options::effective_scale(&s));
                                garden_dirty=true;
                            }
                        }
                        for region in &regions {
                            if let Some(cloud)=plant::cloud_origin(region,garden.elapsed).filter(|p| p.x>=region.min.x && p.x<=region.max.x) {
                                garden.cloud_water(region,cloud.x,dt as f64*debug_options::effective_scale(&s));
                                garden_dirty=true;
                            }
                        }
                        let vertices=plant::vertices(&garden,&regions,rain);
                        renderer.render_plant(&vertices,(garden.elapsed%3600.0) as f32)
                    };
                    match rendered {
                        Ok(())=>frames+=1,
                        Err(wgpu::SurfaceError::Lost|wgpu::SurfaceError::Outdated)=>renderer.resize(size.width,size.height),
                        Err(wgpu::SurfaceError::OutOfMemory)=>{log("GPU out of memory");target.exit();},
                        Err(wgpu::SurfaceError::Timeout)=>{},
                    }
                }
                next_frame=Instant::now()+Duration::from_secs_f64(1.0/fps as f64);
            }
            // Bounded wait also delivers settings sent by the tray's native message thread.
            target.set_control_flow(ControlFlow::WaitUntil(next_frame.min(now+Duration::from_millis(100))));
        }
        if let Event::LoopExiting=event {
            if !preview && garden_dirty {if let Err(e)=garden.save(&plant_path){log(format!("Garden save failed: {e}"));}}
            log(format!("STOP frames={frames}"));
        }
    })?;
    anyhow::ensure!(
        !smoke || smoke_result.load(std::sync::atomic::Ordering::Relaxed) > 0,
        "Smoke test produced no frames"
    );
    Ok(())
}
