//! Explorer integration is intentionally isolated; WorkerW/0x052c are undocumented.
use super::*;
use anyhow::{bail, ensure};
use std::{
    mem::{size_of, zeroed},
    ptr::{null, null_mut},
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::{
        Diagnostics::Debug::*, LibraryLoader::*, Memory::*, Power::*, RemoteDesktop::*,
        Threading::*,
    },
    UI::{Shell::*, WindowsAndMessaging::*},
};

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
unsafe extern "system" fn find_defview(hwnd: HWND, param: LPARAM) -> BOOL {
    let child = FindWindowExW(hwnd, 0, wide("SHELLDLL_DefView").as_ptr(), null());
    if child != 0 {
        *(param as *mut (HWND, HWND)) = (hwnd, child);
        return 0;
    }
    1
}
unsafe fn desktop() -> (HWND, HWND) {
    let mut result = (0, 0);
    EnumWindows(Some(find_defview), &mut result as *mut _ as isize);
    result
}
pub fn attach(hwnd: isize) -> anyhow::Result<()> {
    unsafe {
        let progman = FindWindowW(wide("Progman").as_ptr(), null());
        ensure!(progman != 0, "Explorer Progman was not found");
        let mut result = 0;
        ensure!(
            SendMessageTimeoutW(progman, 0x052c, 0, 0, SMTO_ABORTIFHUNG, 1000, &mut result) != 0,
            "Explorer did not respond to WorkerW request"
        );
        let (host, defview) = desktop();
        ensure!(host != 0, "Desktop icon host was not found");
        let worker = FindWindowExW(0, host, wide("WorkerW").as_ptr(), null());
        let modern =
            (GetWindowLongPtrW(progman, GWL_EXSTYLE) & WS_EX_NOREDIRECTIONBITMAP as isize) != 0;
        let (parent, after) = if modern {
            // Raised desktop (Win11 24H2+): a layered child between icons and
            // the system wallpaper. Never hide or change styles of Explorer windows.
            ensure!(
                GetParent(defview) == progman,
                "Unexpected modern desktop hierarchy"
            );
            ensure!(
                SendMessageTimeoutW(progman, 0x052c, 0xD, 1, SMTO_ABORTIFHUNG, 1000, &mut result)
                    != 0,
                "Modern desktop request timed out"
            );
            (progman, defview)
        } else {
            (worker, HWND_BOTTOM)
        };
        ensure!(
            parent != 0,
            "Background WorkerW was not found (unsupported Explorer layout); try --preview"
        );
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
        SetWindowLongPtrW(
            hwnd,
            GWL_STYLE,
            (style & !(WS_POPUP as isize)) | WS_CHILD as isize,
        );
        SetWindowLongPtrW(
            hwnd,
            GWL_EXSTYLE,
            (GetWindowLongPtrW(hwnd, GWL_EXSTYLE) & !(WS_EX_APPWINDOW as isize))
                | (WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | if modern { WS_EX_LAYERED } else { 0 })
                    as isize,
        );
        if modern {
            ensure!(
                SetLayeredWindowAttributes(hwnd, 0, 255, LWA_ALPHA) != 0,
                "Could not enable wallpaper composition"
            );
        }
        SetLastError(0);
        let previous = SetParent(hwnd, parent);
        ensure!(
            previous != 0 || GetLastError() == 0,
            "SetParent failed: {}",
            GetLastError()
        );
        // MVP targets the primary display, including a nonzero virtual-desktop origin.
        let monitor = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
        let mut info: MONITORINFO = zeroed();
        info.cbSize = size_of::<MONITORINFO>() as u32;
        GetMonitorInfoW(monitor, &mut info);
        let mut origin = POINT {
            x: info.rcMonitor.left,
            y: info.rcMonitor.top,
        };
        ScreenToClient(parent, &mut origin);
        ensure!(
            SetWindowPos(
                hwnd,
                after,
                origin.x,
                origin.y,
                info.rcMonitor.right - info.rcMonitor.left,
                info.rcMonitor.bottom - info.rcMonitor.top,
                SWP_NOACTIVATE | SWP_SHOWWINDOW | SWP_FRAMECHANGED
            ) != 0,
            "SetWindowPos failed"
        );
        crate::log(format!(
            "Wallpaper attached modern={modern} parent={parent:#x} after={after:#x}"
        ));
        Ok(())
    }
}
pub fn healthy(hwnd: isize) -> bool {
    unsafe {
        let parent = GetParent(hwnd);
        parent != 0 && IsWindow(parent) != 0
    }
}

// A timed-out cross-process message may still write later. In that rare case keep
// its tiny remote allocation alive and disable further icon reads for this run.
static ICON_READ_FAILED: AtomicBool = AtomicBool::new(false);
struct Process(HANDLE);
impl Drop for Process {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
pub fn icons(wallpaper: isize) -> anyhow::Result<Vec<Obstacle>> {
    unsafe {
        ensure!(
            !ICON_READ_FAILED.load(Ordering::Relaxed),
            "Icon polling disabled after an Explorer timeout; restart app"
        );
        let (_, defview) = desktop();
        ensure!(defview != 0, "Desktop view unavailable");
        let list = FindWindowExW(defview, 0, wide("SysListView32").as_ptr(), null());
        ensure!(list != 0, "Explorer icon ListView unavailable");
        if IsWindowVisible(list) == 0 {
            return Ok(vec![]);
        }
        let mut count = 0;
        ensure!(
            SendMessageTimeoutW(
                list,
                0x1004,
                0,
                0,
                SMTO_ABORTIFHUNG | SMTO_BLOCK,
                50,
                &mut count
            ) != 0,
            "Icon count query timed out"
        );
        ensure!(count <= 2048, "Unexpected desktop icon count");
        let mut pid = 0;
        GetWindowThreadProcessId(list, &mut pid);
        let process = Process(OpenProcess(
            PROCESS_VM_OPERATION | PROCESS_VM_READ | PROCESS_VM_WRITE,
            0,
            pid,
        ));
        ensure!(
            process.0 != 0,
            "Cannot access Explorer icon rectangles (permission denied)"
        );
        let remote = VirtualAllocEx(
            process.0,
            null(),
            size_of::<RECT>(),
            MEM_COMMIT | MEM_RESERVE,
            PAGE_READWRITE,
        );
        ensure!(
            !remote.is_null(),
            "Cannot allocate Explorer rectangle buffer"
        );
        let deadline = Instant::now();
        let mut obstacles = Vec::new();
        for i in 0..count {
            if deadline.elapsed().as_millis() > 80 {
                break;
            }
            let rect: RECT = zeroed();
            if WriteProcessMemory(
                process.0,
                remote,
                &rect as *const _ as _,
                size_of::<RECT>(),
                null_mut(),
            ) == 0
            {
                VirtualFreeEx(process.0, remote, 0, MEM_RELEASE);
                bail!("Icon buffer write failed");
            }
            let mut success = 0;
            if SendMessageTimeoutW(
                list,
                0x100e,
                i,
                remote as isize,
                SMTO_ABORTIFHUNG | SMTO_BLOCK,
                50,
                &mut success,
            ) == 0
            {
                ICON_READ_FAILED.store(true, Ordering::Relaxed);
                bail!("Explorer icon query timed out; polling disabled to avoid remote-buffer use-after-free");
            }
            let mut rect: RECT = zeroed();
            if success != 0
                && ReadProcessMemory(
                    process.0,
                    remote,
                    &mut rect as *mut _ as _,
                    size_of::<RECT>(),
                    null_mut(),
                ) != 0
            {
                let mut a = POINT {
                    x: rect.left,
                    y: rect.top,
                };
                let mut b = POINT {
                    x: rect.right,
                    y: rect.bottom,
                };
                ClientToScreen(list, &mut a);
                ClientToScreen(list, &mut b);
                ScreenToClient(wallpaper, &mut a);
                ScreenToClient(wallpaper, &mut b);
                if b.x > 0 && b.y > 0 {
                    obstacles.push(Obstacle {
                        min: Vec2::new(a.x as f32, a.y as f32),
                        max: Vec2::new(b.x as f32, b.y as f32),
                    });
                }
            }
        }
        VirtualFreeEx(process.0, remote, 0, MEM_RELEASE);
        Ok(obstacles)
    }
}
static LOCKED: AtomicBool = AtomicBool::new(false);
static SLEEPING: AtomicBool = AtomicBool::new(false);
pub fn environment(wallpaper: isize) -> Environment {
    unsafe {
        let mut p: POINT = zeroed();
        let cursor = if GetCursorPos(&mut p) != 0 {
            ScreenToClient(wallpaper, &mut p);
            Some(Vec2::new(p.x as f32, p.y as f32))
        } else {
            None
        };
        let mut power: SYSTEM_POWER_STATUS = zeroed();
        let battery = GetSystemPowerStatus(&mut power) != 0 && power.ACLineStatus == 0;
        let foreground = GetForegroundWindow();
        let mut fullscreen = false;
        if foreground != 0 && foreground != wallpaper && foreground != GetParent(wallpaper) {
            let mut class = [0u16; 64];
            GetClassNameW(foreground, class.as_mut_ptr(), 64);
            let class = String::from_utf16_lossy(&class)
                .trim_end_matches('\0')
                .to_owned();
            if class != "Progman" && class != "WorkerW" && class != "Shell_TrayWnd" {
                let monitor = MonitorFromWindow(wallpaper, MONITOR_DEFAULTTOPRIMARY);
                let mut info: MONITORINFO = zeroed();
                info.cbSize = size_of::<MONITORINFO>() as u32;
                let mut r: RECT = zeroed();
                if GetMonitorInfoW(monitor, &mut info) != 0
                    && GetWindowRect(foreground, &mut r) != 0
                {
                    fullscreen = r.left <= info.rcMonitor.left
                        && r.top <= info.rcMonitor.top
                        && r.right >= info.rcMonitor.right
                        && r.bottom >= info.rcMonitor.bottom;
                }
            }
        }
        Environment {
            cursor,
            battery,
            fullscreen,
            suspended: LOCKED.load(Ordering::Relaxed) || SLEEPING.load(Ordering::Relaxed),
        }
    }
}

const TRAY_MSG: u32 = WM_APP + 17;
pub struct TrayGuard {
    hwnd: HWND,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Drop for TrayGuard {
    fn drop(&mut self) {
        unsafe {
            PostMessageW(self.hwnd, WM_APP + 18, 0, 0);
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
pub struct InstanceGuard(HANDLE);
impl Drop for InstanceGuard {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
pub fn single_instance() -> anyhow::Result<InstanceGuard> {
    unsafe {
        let handle = CreateMutexW(null(), 0, wide("Local\\LivingWallpaper.MVP").as_ptr());
        ensure!(handle != 0, "Cannot create instance mutex");
        if GetLastError() == ERROR_ALREADY_EXISTS {
            CloseHandle(handle);
            bail!("Living Wallpaper is already running. Use its tray icon to quit first.");
        }
        Ok(InstanceGuard(handle))
    }
}
struct TrayContext {
    tx: Sender<Command>,
    settings: Arc<Mutex<Settings>>,
    icon: NOTIFYICONDATAW,
    taskbar: u32,
}
unsafe fn add_item(menu: HMENU, id: usize, text: &str, checked: bool) {
    AppendMenuW(
        menu,
        MF_STRING | if checked { MF_CHECKED } else { MF_UNCHECKED },
        id,
        wide(text).as_ptr(),
    );
}
unsafe extern "system" fn tray_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if msg == WM_APP + 18 {
        PostQuitMessage(0);
        return 0;
    }
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut TrayContext;
    if !ptr.is_null() {
        let context = &mut *ptr;
        if msg == context.taskbar {
            Shell_NotifyIconW(NIM_ADD, &context.icon);
            return 0;
        }
        if msg == WM_WTSSESSION_CHANGE {
            if w == WTS_SESSION_LOCK as usize {
                LOCKED.store(true, Ordering::Relaxed);
            }
            if w == WTS_SESSION_UNLOCK as usize {
                LOCKED.store(false, Ordering::Relaxed);
            }
            return 0;
        }
        if msg == WM_POWERBROADCAST {
            if w == 4 {
                SLEEPING.store(true, Ordering::Relaxed);
            }
            if w == 7 || w == 18 {
                SLEEPING.store(false, Ordering::Relaxed);
            }
            return 1;
        }
        if msg == TRAY_MSG && (l as u32 == WM_RBUTTONUP || l as u32 == WM_LBUTTONUP) {
            let settings = context.settings.lock().unwrap().clone();
            let menu = CreatePopupMenu();
            add_item(menu, 0, "Living Wallpaper · Fish model", false);
            AppendMenuW(menu, MF_SEPARATOR, 0, null());
            add_item(menu, 1, "Pause", settings.paused);
            add_item(menu, 2, "Mouse interaction", settings.mouse);
            add_item(menu, 3, "Avoid desktop icons", settings.icons);
            add_item(menu, 4, "Power saving", settings.power_saving);
            AppendMenuW(menu, MF_SEPARATOR, 0, null());
            for (id, n) in [(10, 120), (11, 240), (12, 500), (13, 1000)] {
                add_item(menu, id, &format!("Fish: {n}"), settings.fish_count == n);
            }
            for (id, n) in [(20, 15), (21, 30), (22, 60)] {
                add_item(menu, id, &format!("FPS: {n}"), settings.fps == n);
            }
            AppendMenuW(menu, MF_SEPARATOR, 0, null());
            add_item(menu, 99, "Quit", false);
            let mut point: POINT = zeroed();
            GetCursorPos(&mut point);
            SetForegroundWindow(hwnd);
            let id = TrackPopupMenu(
                menu,
                TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON,
                point.x,
                point.y,
                0,
                hwnd,
                null(),
            );
            DestroyMenu(menu);
            PostMessageW(hwnd, WM_NULL, 0, 0);
            let command = match id {
                1 => Some(Command::Pause),
                2 => Some(Command::Mouse),
                3 => Some(Command::Icons),
                4 => Some(Command::Power),
                10 => Some(Command::Count(120)),
                11 => Some(Command::Count(240)),
                12 => Some(Command::Count(500)),
                13 => Some(Command::Count(1000)),
                20 => Some(Command::Fps(15)),
                21 => Some(Command::Fps(30)),
                22 => Some(Command::Fps(60)),
                99 => Some(Command::Quit),
                _ => None,
            };
            if let Some(command) = command {
                let _ = context.tx.send(command);
            }
            return 0;
        }
    }
    DefWindowProcW(hwnd, msg, w, l)
}
pub fn start_tray(
    tx: Sender<Command>,
    settings: Arc<Mutex<Settings>>,
) -> anyhow::Result<TrayGuard> {
    let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
    let thread = std::thread::spawn(move || unsafe {
        let module = GetModuleHandleW(null());
        let class = wide("LivingWallpaperTray");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(tray_proc),
            hInstance: module,
            lpszClassName: class.as_ptr(),
            ..zeroed()
        };
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            class.as_ptr(),
            wide("Living Wallpaper settings").as_ptr(),
            WS_POPUP,
            0,
            0,
            0,
            0,
            0,
            0,
            module,
            null(),
        );
        if hwnd == 0 {
            let _ = ready_tx.send(0);
            return;
        }
        let mut icon: NOTIFYICONDATAW = zeroed();
        icon.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
        icon.hWnd = hwnd;
        icon.uID = 1;
        icon.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        icon.uCallbackMessage = TRAY_MSG;
        icon.hIcon = LoadIconW(0, IDI_APPLICATION);
        let tip = wide("Living Wallpaper · click for settings");
        icon.szTip[..tip.len()].copy_from_slice(&tip);
        let mut context = Box::new(TrayContext {
            tx,
            settings,
            icon,
            taskbar: RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()),
        });
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, &mut *context as *mut _ as isize);
        WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION);
        let added = Shell_NotifyIconW(NIM_ADD, &context.icon) != 0;
        let _ = ready_tx.send(if added { hwnd } else { 0 });
        if added {
            let mut msg: MSG = zeroed();
            while GetMessageW(&mut msg, 0, 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        Shell_NotifyIconW(NIM_DELETE, &context.icon);
        WTSUnRegisterSessionNotification(hwnd);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        DestroyWindow(hwnd);
    });
    let hwnd = ready_rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap_or(0);
    ensure!(hwnd != 0, "Could not create system tray icon");
    Ok(TrayGuard {
        hwnd,
        thread: Some(thread),
    })
}
