//! Explorer integration is intentionally isolated; WorkerW/0x052c are undocumented.
use super::*;
use crate::settings::{ModelMode, FPS_CHOICES};
use anyhow::{bail, ensure};
use std::{
    mem::{size_of, zeroed},
    ptr::{null, null_mut},
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};
use windows_sys::Win32::UI::Controls::{
    InitCommonControlsEx, ICC_BAR_CLASSES, INITCOMMONCONTROLSEX, TBM_SETLINESIZE, TBM_SETPAGESIZE,
    TBM_SETPOS, TBM_SETRANGEMAX, TBM_SETRANGEMIN, TBS_NOTICKS, TB_THUMBTRACK,
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::{
        Diagnostics::Debug::*, LibraryLoader::*, Memory::*, Power::*, RemoteDesktop::*,
        Threading::*,
    },
    UI::{HiDpi::*, Input::KeyboardAndMouse::SetFocus, Shell::*, WindowsAndMessaging::*},
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
        configure_displays(hwnd, &[])?;
        SetWindowPos(
            hwnd,
            after,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
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
#[path = "windows_panel.rs"]
mod panel;
const PANEL_STYLE: u32 = WS_POPUP | WS_SYSMENU | WS_CLIPCHILDREN;
const PANEL_EXSTYLE: u32 = WS_EX_TOOLWINDOW | WS_EX_CONTROLPARENT;
// TBM_GETPOS equals WM_USER; windows-sys 0.52 omits this zero-offset constant.
const TBM_GETPOS: u32 = WM_USER;
const MODE_ID: i32 = 1030;
const COUNT_ID: i32 = 1010;
const FPS_ID: i32 = 1020;
const TIME_ID: i32 = 1050;
const QUIT_ID: i32 = 1099;
// id, class, label, style, x, y, width, height (96-DPI logical pixels).
const PANEL_CONTROLS: [(i32, &str, &str, u32, i32, i32, i32, i32); 17] = [
    (1040, "STATIC", "Living Wallpaper", 0, 50, 16, 236, 28),
    (
        panel::CLOSE_ID,
        "BUTTON",
        "設定を閉じる",
        BS_PUSHBUTTON as u32 | WS_TABSTOP,
        300,
        12,
        28,
        28,
    ),
    (1000, "STATIC", "シーン", 0, 20, 76, 110, 26),
    (
        MODE_ID,
        "COMBOBOX",
        "",
        CBS_DROPDOWNLIST as u32
            | CBS_OWNERDRAWFIXED as u32
            | CBS_HASSTRINGS as u32
            | WS_TABSTOP
            | WS_VSCROLL,
        168,
        68,
        152,
        160,
    ),
    (
        1002,
        "BUTTON",
        "カーソルに反応",
        BS_OWNERDRAW as u32 | WS_TABSTOP,
        20,
        118,
        300,
        36,
    ),
    (
        1003,
        "BUTTON",
        "アイコンを避ける",
        BS_OWNERDRAW as u32 | WS_TABSTOP,
        20,
        160,
        300,
        36,
    ),
    (
        1004,
        "BUTTON",
        "省エネ",
        BS_OWNERDRAW as u32 | WS_TABSTOP,
        20,
        202,
        300,
        36,
    ),
    (1005, "STATIC", "群れの大きさ", 0, 20, 254, 208, 26),
    (1045, "STATIC", "240匹", 0, 234, 254, 86, 26),
    (
        COUNT_ID,
        "msctls_trackbar32",
        "",
        TBS_NOTICKS | WS_TABSTOP,
        20,
        284,
        300,
        30,
    ),
    (1006, "STATIC", "なめらかさ", 0, 20, 330, 198, 26),
    (1046, "STATIC", "30 FPS", 0, 222, 330, 98, 26),
    (
        FPS_ID,
        "msctls_trackbar32",
        "",
        TBS_NOTICKS | WS_TABSTOP,
        20,
        360,
        300,
        30,
    ),
    (1008, "STATIC", "時間の速さ", 0, 20, 406, 198, 26),
    (1047, "STATIC", "1.00×", 0, 222, 406, 98, 26),
    (
        TIME_ID,
        "msctls_trackbar32",
        "",
        TBS_NOTICKS | WS_TABSTOP,
        20,
        436,
        300,
        30,
    ),
    (
        QUIT_ID,
        "BUTTON",
        "終了",
        BS_PUSHBUTTON as u32 | WS_TABSTOP,
        20,
        482,
        100,
        28,
    ),
];
struct TrayContext {
    tx: Sender<Command>,
    settings: Arc<Mutex<Settings>>,
    icon: NOTIFYICONDATAW,
    taskbar: u32,
    font: HFONT,
    display_ids: Vec<String>,
    last_count: usize,
    last_fps: u32,
    last_time_scale: f32,
    brush: HBRUSH,
}
unsafe fn layout_panel(hwnd: HWND, context: &mut TrayContext) {
    let dpi = GetDpiForWindow(hwnd).max(96);
    let scale = |n: i32| ((n as i64 * dpi as i64 + 48) / 96) as i32;
    let new_font = CreateFontW(
        -scale(15),
        0,
        0,
        0,
        FW_NORMAL as i32,
        0,
        0,
        0,
        DEFAULT_CHARSET as u32,
        OUT_DEFAULT_PRECIS as u32,
        CLIP_DEFAULT_PRECIS as u32,
        DEFAULT_QUALITY as u32,
        DEFAULT_PITCH as u32,
        wide(panel::FONT_FAMILY).as_ptr(),
    );
    for &(id, _, _, _, x, y, w, h) in &PANEL_CONTROLS {
        let child = GetDlgItem(hwnd, id);
        SetWindowPos(
            child,
            0,
            scale(x),
            scale(y),
            scale(w),
            scale(h),
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
        let font = if new_font != 0 {
            new_font
        } else {
            GetStockObject(DEFAULT_GUI_FONT)
        };
        SendMessageW(child, WM_SETFONT, font as usize, 1);
    }
    for (index, _) in context.display_ids.iter().enumerate() {
        let child = GetDlgItem(hwnd, 2000 + index as i32);
        SetWindowPos(
            child,
            0,
            scale(20),
            scale(520 + index as i32 * 36),
            scale(300),
            scale(32),
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
        SendMessageW(child, WM_SETFONT, new_font as usize, 0);
    }
    SetWindowPos(
        GetDlgItem(hwnd, 1999),
        0,
        scale(20),
        scale(480),
        scale(300),
        scale(28),
        SWP_NOZORDER | SWP_NOACTIVATE,
    );
    SendMessageW(GetDlgItem(hwnd, 1999), WM_SETFONT, new_font as usize, 0);
    SetWindowPos(
        GetDlgItem(hwnd, QUIT_ID),
        0,
        scale(20),
        scale(524 + context.display_ids.len() as i32 * 36),
        scale(100),
        scale(28),
        SWP_NOZORDER | SWP_NOACTIVATE,
    );
    let old_font = context.font;
    context.font = new_font;
    if old_font != 0 {
        DeleteObject(old_font);
    }
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: scale(340),
        bottom: scale(566 + context.display_ids.len() as i32 * 36),
    };
    AdjustWindowRectExForDpi(&mut rect, PANEL_STYLE, 0, PANEL_EXSTYLE, dpi);
    SetWindowPos(
        hwnd,
        0,
        0,
        0,
        rect.right - rect.left,
        rect.bottom - rect.top,
        SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
    );
    panel::apply_frame(hwnd);
}
unsafe fn create_panel_controls(hwnd: HWND, module: HINSTANCE) -> bool {
    let common = INITCOMMONCONTROLSEX {
        dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
        dwICC: ICC_BAR_CLASSES,
    };
    if InitCommonControlsEx(&common) == 0 {
        return false;
    }
    for &(id, class, label, style, _, _, _, _) in &PANEL_CONTROLS {
        let child = CreateWindowExW(
            0,
            wide(class).as_ptr(),
            wide(label).as_ptr(),
            WS_CHILD | WS_VISIBLE | style,
            0,
            0,
            0,
            0,
            hwnd,
            id as isize,
            module,
            null(),
        );
        if child == 0 || !panel::install(child, id) {
            return false;
        }
    }
    true
}
unsafe fn refresh_panel(hwnd: HWND, context: &mut TrayContext) {
    // Recreate labels on open so newly connected displays appear too.
    let current = displays();
    context.display_ids = current.iter().map(|d| d.id.clone()).collect();
    if GetDlgItem(hwnd, 1999) == 0 {
        let label = CreateWindowExW(
            0,
            wide("STATIC").as_ptr(),
            wide("モニター").as_ptr(),
            WS_CHILD | WS_VISIBLE,
            0,
            0,
            0,
            0,
            hwnd,
            1999,
            GetModuleHandleW(null()),
            null(),
        );
        panel::install(label, 1999);
    }
    for index in 0..64 {
        let old = GetDlgItem(hwnd, 2000 + index);
        if old != 0 {
            DestroyWindow(old);
        }
    }
    let settings = context.settings.lock().unwrap().clone();
    for (index, d) in current.iter().enumerate() {
        let label = d.name.clone();
        let child = CreateWindowExW(
            0,
            wide("BUTTON").as_ptr(),
            wide(&label).as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW as u32,
            0,
            0,
            0,
            0,
            hwnd,
            (2000 + index) as isize,
            GetModuleHandleW(null()),
            null(),
        );
        panel::install(child, 2000 + index as i32);
        SendMessageW(
            child,
            BM_SETCHECK,
            usize::from(!settings.disabled_displays.contains(&d.id)),
            0,
        );
    }
    layout_panel(hwnd, context);
    let s = context.settings.lock().unwrap().clone();
    for (id, checked) in [(1002, s.mouse), (1003, s.icons), (1004, s.power_saving)] {
        SendMessageW(GetDlgItem(hwnd, id), BM_SETCHECK, usize::from(checked), 0);
    }
    let mut s = s;
    s.normalize();
    for id in [1003, 1005, 1045, COUNT_ID] {
        ShowWindow(
            GetDlgItem(hwnd, id),
            if s.mode == ModelMode::Fish {
                SW_SHOW
            } else {
                SW_HIDE
            },
        );
    }

    context.last_count = s.fish_count;
    context.last_fps = s.fps;
    context.last_time_scale = s.time_scale;
    let mode = GetDlgItem(hwnd, MODE_ID);
    SendMessageW(mode, CB_RESETCONTENT, 0, 0);
    for (i, choice) in ModelMode::ALL.iter().enumerate() {
        SendMessageW(
            mode,
            CB_ADDSTRING,
            0,
            wide(choice.label()).as_ptr() as isize,
        );
        if *choice == s.mode {
            SendMessageW(mode, CB_SETCURSEL, i, 0);
        }
    }
    for (id, min, max, pos, page) in [
        (COUNT_ID, 1, 100, (s.fish_count / 10) as i32, 5),
        (
            FPS_ID,
            0,
            (FPS_CHOICES.len() - 1) as isize,
            FPS_CHOICES.iter().position(|&v| v == s.fps).unwrap() as i32,
            1,
        ),
        (TIME_ID, 0, 8, (s.time_scale * 4.0) as i32, 1),
    ] {
        let slider = GetDlgItem(hwnd, id);
        SendMessageW(slider, TBM_SETRANGEMIN, 0, min);
        SendMessageW(slider, TBM_SETRANGEMAX, 0, max);
        SendMessageW(slider, TBM_SETLINESIZE, 0, 1);
        SendMessageW(slider, TBM_SETPAGESIZE, 0, page);
        SendMessageW(slider, TBM_SETPOS, 1, pos as isize);
    }
    update_slider_labels(hwnd);
}
unsafe fn slider_values(hwnd: HWND) -> (usize, u32) {
    let count =
        SendMessageW(GetDlgItem(hwnd, COUNT_ID), TBM_GETPOS, 0, 0).clamp(1, 100) as usize * 10;
    let fps_index = SendMessageW(GetDlgItem(hwnd, FPS_ID), TBM_GETPOS, 0, 0)
        .clamp(0, (FPS_CHOICES.len() - 1) as isize) as usize;
    (count, FPS_CHOICES[fps_index])
}
unsafe fn slider_time_scale(hwnd: HWND) -> f32 {
    SendMessageW(GetDlgItem(hwnd, TIME_ID), TBM_GETPOS, 0, 0).clamp(0, 8) as f32 / 4.0
}
unsafe fn set_label_if_changed(hwnd: HWND, id: i32, value: &str) {
    let child = GetDlgItem(hwnd, id);
    let mut current = [0u16; 128];
    let len = GetWindowTextW(child, current.as_mut_ptr(), current.len() as i32).max(0) as usize;
    let value = wide(value);
    if current[..len] != value[..value.len() - 1] {
        SetWindowTextW(child, value.as_ptr());
    }
}
unsafe fn update_slider_labels(hwnd: HWND) {
    let (count, fps) = slider_values(hwnd);
    set_label_if_changed(hwnd, 1045, &format!("{count}匹"));
    set_label_if_changed(hwnd, 1046, &format!("{fps} FPS"));
    set_label_if_changed(hwnd, 1047, &format!("{:.2}×", slider_time_scale(hwnd)));
}
unsafe fn show_panel(hwnd: HWND, context: &mut TrayContext) {
    refresh_panel(hwnd, context);
    let mut point: POINT = zeroed();
    GetCursorPos(&mut point);
    let monitor = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
    let mut info: MONITORINFO = zeroed();
    info.cbSize = size_of::<MONITORINFO>() as u32;
    if GetMonitorInfoW(monitor, &mut info) != 0 {
        // Moving first lets WM_DPICHANGED lay out controls for this monitor.
        SetWindowPos(
            hwnd,
            0,
            point.x,
            point.y,
            0,
            0,
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
        let mut rect: RECT = zeroed();
        GetWindowRect(hwnd, &mut rect);
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        let x = point.x.clamp(
            info.rcWork.left,
            (info.rcWork.right - width).max(info.rcWork.left),
        );
        // Leave breathing room above the tray flyout/click position.
        let gap = (48 * GetDpiForWindow(hwnd).max(96) / 96) as i32;
        let y = (point.y - height - gap).clamp(
            info.rcWork.top,
            (info.rcWork.bottom - height).max(info.rcWork.top),
        );
        SetWindowPos(hwnd, HWND_TOP, x, y, 0, 0, SWP_NOSIZE | SWP_SHOWWINDOW);
    } else {
        ShowWindow(hwnd, SW_SHOW);
    }
    SetForegroundWindow(hwnd);
    SetFocus(GetDlgItem(hwnd, 1002));
}
unsafe extern "system" fn tray_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if msg == WM_DRAWITEM && w == MODE_ID as usize && l != 0 {
        panel::scene_item(&*(l as *const windows_sys::Win32::UI::Controls::DRAWITEMSTRUCT));
        return 1;
    }
    if msg == WM_ERASEBKGND {
        return 1;
    }
    if msg == WM_PAINT {
        let mut ps: PAINTSTRUCT = zeroed();
        let dc = BeginPaint(hwnd, &mut ps);
        panel::frame(hwnd, dc);
        EndPaint(hwnd, &ps);
        return 0;
    }
    if msg == WM_PRINTCLIENT {
        panel::frame(hwnd, w as HDC);
        return 0;
    }
    if msg == WM_NCHITTEST {
        let mut point = POINT {
            x: (l as u32 & 0xffff) as i16 as i32,
            y: ((l as u32 >> 16) & 0xffff) as i16 as i32,
        };
        ScreenToClient(hwnd, &mut point);
        let dpi = GetDpiForWindow(hwnd).max(96) as i32;
        let mut r: RECT = zeroed();
        GetClientRect(hwnd, &mut r);
        if point.y >= 0 && point.y < 56 * dpi / 96 && point.x < r.right - 48 * dpi / 96 {
            return HTCAPTION as isize;
        }
        return HTCLIENT as isize;
    }
    if msg == WM_APP + 19 {
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut TrayContext;
        if !ptr.is_null() {
            // Activate after Explorer finishes handling the tray click, allowing
            // its notification overflow to dismiss on foreground change.
            show_panel(hwnd, &mut *ptr);
        }
        return 0;
    }
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
        if matches!(msg, WM_CTLCOLORLISTBOX | WM_CTLCOLOREDIT) {
            SetTextColor(w as HDC, 0x00f5e7d5);
            SetBkColor(w as HDC, 0x00583917);
            return context.brush;
        }
        if msg == WM_CLOSE {
            ShowWindow(hwnd, SW_HIDE);
            return 0;
        }
        if msg == WM_DPICHANGED {
            layout_panel(hwnd, context);
            return 0;
        }
        if msg == WM_HSCROLL
            && (l == GetDlgItem(hwnd, COUNT_ID)
                || l == GetDlgItem(hwnd, FPS_ID)
                || l == GetDlgItem(hwnd, TIME_ID))
        {
            update_slider_labels(hwnd);
            // Preview values during drag; commit once released (keyboard and
            // page steps commit immediately), avoiding repeated disk writes.
            if (w & 0xffff) as u32 != TB_THUMBTRACK {
                let (count, fps) = slider_values(hwnd);
                if l == GetDlgItem(hwnd, COUNT_ID) && count != context.last_count {
                    context.last_count = count;
                    let _ = context.tx.send(Command::Count(count));
                }
                if l == GetDlgItem(hwnd, FPS_ID) && fps != context.last_fps {
                    context.last_fps = fps;
                    let _ = context.tx.send(Command::Fps(fps));
                }
                let time_scale = slider_time_scale(hwnd);
                if l == GetDlgItem(hwnd, TIME_ID) && time_scale != context.last_time_scale {
                    context.last_time_scale = time_scale;
                    let _ = context.tx.send(Command::TimeScale(time_scale));
                }
            }
            return 0;
        }
        if msg == WM_COMMAND {
            let id = (w & 0xffff) as i32;
            let notice = ((w >> 16) & 0xffff) as u32;
            if id == MODE_ID && notice == CBN_DROPDOWN {
                panel::style_scene_list(GetDlgItem(hwnd, MODE_ID));
                return 0;
            }
            if id == IDCANCEL || id == panel::CLOSE_ID {
                ShowWindow(hwnd, SW_HIDE);
                return 0;
            }
            if (2000..2064).contains(&id) && notice == BN_CLICKED {
                let child = GetDlgItem(hwnd, id);
                let checked = SendMessageW(child, BM_GETCHECK, 0, 0) != 0;
                SendMessageW(child, BM_SETCHECK, usize::from(!checked), 0);
                if let Some(display_id) = context.display_ids.get((id - 2000) as usize) {
                    let _ = context.tx.send(Command::Monitor(display_id.clone()));
                }
                return 0;
            }
            if (1002..=1004).contains(&id) && notice == BN_CLICKED {
                let control = GetDlgItem(hwnd, id);
                let checked = SendMessageW(control, BM_GETCHECK, 0, 0) != 0;
                SendMessageW(control, BM_SETCHECK, usize::from(!checked), 0);
            }
            let command = match (id, notice) {
                (1002, BN_CLICKED) => Some(Command::Mouse),
                (1003, BN_CLICKED) => Some(Command::Icons),
                (1004, BN_CLICKED) => Some(Command::Power),
                (MODE_ID, CBN_SELCHANGE) => {
                    let index = SendMessageW(GetDlgItem(hwnd, MODE_ID), CB_GETCURSEL, 0, 0);
                    if let Some(mode) = ModelMode::ALL.get(index as usize) {
                        for id in [1003, 1005, 1045, COUNT_ID] {
                            ShowWindow(
                                GetDlgItem(hwnd, id),
                                if *mode == ModelMode::Fish {
                                    SW_SHOW
                                } else {
                                    SW_HIDE
                                },
                            );
                        }
                    }
                    ModelMode::ALL
                        .get(index as usize)
                        .copied()
                        .map(Command::Mode)
                }
                (QUIT_ID, BN_CLICKED) => {
                    ShowWindow(hwnd, SW_HIDE);
                    Some(Command::Quit)
                }
                _ => None,
            };
            if let Some(command) = command {
                let _ = context.tx.send(command);
            }
            // A setting change does not hide or destroy the panel.
            return 0;
        }
        if msg == TRAY_MSG && (l as u32 == WM_RBUTTONUP || l as u32 == WM_LBUTTONUP) {
            if IsWindowVisible(hwnd) != 0 {
                ShowWindow(hwnd, SW_HIDE);
            } else {
                PostMessageW(hwnd, WM_APP + 19, 0, 0);
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
        let _panel_font = match panel::FontResource::load() {
            Ok(font) => font,
            Err(error) => {
                crate::log(error.to_string());
                let _ = ready_tx.send(0);
                return;
            }
        };
        let module = GetModuleHandleW(null());
        let class = wide("LivingWallpaperTray");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(tray_proc),
            hInstance: module,
            lpszClassName: class.as_ptr(),
            hbrBackground: 0,
            ..zeroed()
        };
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(
            PANEL_EXSTYLE,
            class.as_ptr(),
            wide("Living Wallpaper").as_ptr(),
            PANEL_STYLE,
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
        icon.hIcon = LoadImageW(
            module,
            2usize as *const u16,
            IMAGE_ICON,
            GetSystemMetrics(SM_CXSMICON),
            GetSystemMetrics(SM_CYSMICON),
            LR_SHARED,
        ) as HICON;
        let tip = wide("Living Wallpaper · click for settings");
        icon.szTip[..tip.len()].copy_from_slice(&tip);
        let mut context = Box::new(TrayContext {
            tx,
            settings,
            icon,
            taskbar: RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()),
            font: 0,
            display_ids: vec![],
            last_count: 0,
            last_fps: 0,
            last_time_scale: 0.0,
            brush: CreateSolidBrush(0x00583917),
        });
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, &mut *context as *mut _ as isize);
        let controls_ready = create_panel_controls(hwnd, module);
        if controls_ready {
            layout_panel(hwnd, &mut context);
            refresh_panel(hwnd, &mut context);
        }
        WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION);
        let added = controls_ready && Shell_NotifyIconW(NIM_ADD, &context.icon) != 0;
        let _ = ready_tx.send(if added { hwnd } else { 0 });
        if added {
            let mut msg: MSG = zeroed();
            while GetMessageW(&mut msg, 0, 0, 0) > 0 {
                if IsWindowVisible(hwnd) == 0 || IsDialogMessageW(hwnd, &msg) == 0 {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        }
        Shell_NotifyIconW(NIM_DELETE, &context.icon);
        WTSUnRegisterSessionNotification(hwnd);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        DestroyWindow(hwnd);
        if context.font != 0 {
            DeleteObject(context.font);
        }
        DeleteObject(context.brush);
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

#[cfg(test)]
mod panel_tests {
    use super::*;
    use windows_sys::Win32::UI::Controls::TB_ENDTRACK;
    #[test]
    fn repeated_panel_commands_keep_window_alive_and_close_does_not_quit() {
        unsafe {
            let _panel_font = panel::FontResource::load().unwrap();
            let module = GetModuleHandleW(null());
            let class = wide("LivingWallpaperPanelTest");
            let wc = WNDCLASSW {
                lpfnWndProc: Some(tray_proc),
                hInstance: module,
                lpszClassName: class.as_ptr(),
                hbrBackground: 0,
                ..zeroed()
            };
            assert_ne!(RegisterClassW(&wc), 0);
            let hwnd = CreateWindowExW(
                PANEL_EXSTYLE,
                class.as_ptr(),
                wide("Panel test").as_ptr(),
                PANEL_STYLE,
                0,
                0,
                0,
                0,
                0,
                0,
                module,
                null(),
            );
            assert_ne!(hwnd, 0);
            let (tx, rx) = std::sync::mpsc::channel();
            let mut context = Box::new(TrayContext {
                tx,
                settings: Arc::new(Mutex::new(Settings {
                    fish_count: 333,
                    fps: 24,
                    ..Settings::default()
                })),
                icon: zeroed(),
                taskbar: WM_APP + 200,
                font: 0,
                display_ids: vec![],
                last_count: 0,
                last_fps: 0,
                last_time_scale: 0.0,
                brush: CreateSolidBrush(0x00583917),
            });
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, &mut *context as *mut _ as isize);
            assert!(create_panel_controls(hwnd, module));
            layout_panel(hwnd, &mut context);
            refresh_panel(hwnd, &mut context);
            // Verify GDI actually selected the bundled face, not a fallback.
            let dc = GetDC(hwnd);
            let old = SelectObject(dc, context.font);
            let mut face = [0u16; 64];
            let len = GetTextFaceW(dc, face.len() as i32, face.as_mut_ptr());
            assert!(len > 0);
            let end = face.iter().position(|&v| v == 0).unwrap_or(face.len());
            assert_eq!(String::from_utf16_lossy(&face[..end]), panel::FONT_FAMILY);
            SelectObject(dc, old);
            ReleaseDC(hwnd, dc);
            assert_eq!(slider_values(hwnd), (330, 30));
            if let Some(id) = context.display_ids.first().cloned() {
                let control = GetDlgItem(hwnd, 2000);
                assert_ne!(control, 0);
                SendMessageW(hwnd, WM_COMMAND, 2000, control);
                assert!(matches!(rx.try_recv(),Ok(Command::Monitor(value)) if value==id));
                assert_eq!(SendMessageW(control, BM_GETCHECK, 0, 0), 0);
            }

            assert_ne!(GetDlgItem(hwnd, panel::CLOSE_ID), 0);
            assert_eq!(GetWindowLongPtrW(hwnd, GWL_STYLE) & WS_CAPTION as isize, 0);
            SendMessageW(
                hwnd,
                WM_COMMAND,
                panel::CLOSE_ID as usize,
                GetDlgItem(hwnd, panel::CLOSE_ID),
            );
            assert_ne!(IsWindow(hwnd), 0, "custom top-right close must only hide");
            assert!(rx.try_recv().is_err(), "custom close must not quit");
            if let Some(path) = std::env::var_os("LIVING_WALLPAPER_UI_CAPTURE") {
                panel::capture_for_test(hwnd, std::path::Path::new(&path));
            }
            assert_eq!(GetDlgItem(hwnd, 1007), 0, "FPS legend must be removed");
            assert_eq!(
                SendMessageW(GetDlgItem(hwnd, MODE_ID), CB_GETCOUNT, 0, 0),
                2
            );
            assert_eq!(GetDlgItem(hwnd, 1001), 0, "pause checkbox must be removed");
            assert_eq!(
                GetDlgItem(hwnd, IDCANCEL),
                0,
                "bottom close button must be removed"
            );
            SendMessageW(hwnd, WM_COMMAND, 1002, GetDlgItem(hwnd, 1002));
            SendMessageW(hwnd, WM_COMMAND, 1003, GetDlgItem(hwnd, 1003));
            assert!(matches!(rx.try_recv(), Ok(Command::Mouse)));
            assert!(matches!(rx.try_recv(), Ok(Command::Icons)));
            assert_ne!(IsWindow(hwnd), 0, "setting changes destroyed the panel");
            let toggle = GetDlgItem(hwnd, 1002);
            let initial = SendMessageW(toggle, BM_GETCHECK, 0, 0);
            // Windows reports the second rapid press as a double-click message.
            let point = (12 << 16) | 12;
            for press in [WM_LBUTTONDOWN, WM_LBUTTONDBLCLK] {
                SendMessageW(toggle, press, 1, point);
                SendMessageW(toggle, WM_LBUTTONUP, 0, point);
                assert!(
                    matches!(rx.try_recv(), Ok(Command::Mouse)),
                    "every rapid click must toggle, including double-click presses"
                );
                assert!(rx.try_recv().is_err(), "one click must send one command");
            }
            assert_eq!(SendMessageW(toggle, BM_GETCHECK, 0, 0), initial);
            let count = GetDlgItem(hwnd, COUNT_ID);
            SendMessageW(count, TBM_SETPOS, 1, 25);
            SendMessageW(hwnd, WM_HSCROLL, TB_THUMBTRACK as usize, count);
            assert!(
                rx.try_recv().is_err(),
                "drag should not repeatedly save settings"
            );
            SendMessageW(hwnd, WM_HSCROLL, TB_ENDTRACK as usize, count);
            assert!(matches!(rx.try_recv(), Ok(Command::Count(250))));
            SendMessageW(hwnd, WM_HSCROLL, TB_ENDTRACK as usize, count);
            assert!(
                rx.try_recv().is_err(),
                "duplicate release should not save again"
            );
            let fps = GetDlgItem(hwnd, FPS_ID);
            for (i, value) in FPS_CHOICES.iter().enumerate() {
                SendMessageW(fps, TBM_SETPOS, 1, i as isize);
                SendMessageW(hwnd, WM_HSCROLL, TB_ENDTRACK as usize, fps);
                assert!(matches!(rx.try_recv(),Ok(Command::Fps(n)) if n==*value));
            }
            let time_slider = GetDlgItem(hwnd, TIME_ID);
            for i in 0..=8 {
                SendMessageW(time_slider, TBM_SETPOS, 1, i);
                SendMessageW(hwnd, WM_HSCROLL, TB_ENDTRACK as usize, time_slider);
                assert!(
                    matches!(rx.try_recv(),Ok(Command::TimeScale(value)) if value==i as f32/4.0)
                );
            }
            SendMessageW(time_slider, TBM_SETPOS, 1, 6);
            SendMessageW(hwnd, WM_HSCROLL, TB_THUMBTRACK as usize, time_slider);
            assert!(rx.try_recv().is_err());
            SendMessageW(hwnd, WM_HSCROLL, TB_ENDTRACK as usize, time_slider);
            assert!(matches!(rx.try_recv(), Ok(Command::TimeScale(1.5))));
            // Drive actual control input, rather than only synthetic parent
            // notifications. Motion within one detent must still move the thumb.
            SendMessageW(time_slider, TBM_SETPOS, 0, 0);
            let dpi = GetDpiForWindow(time_slider).max(96) as i32;
            let px = |v: i32| (v * dpi + 48) / 96;
            let point = |x: i32| ((px(15) as u32) << 16 | (x as u32 & 0xffff)) as isize;
            SendMessageW(time_slider, WM_LBUTTONDOWN, 1, point(px(10)));
            SendMessageW(time_slider, WM_MOUSEMOVE, 1, point(px(22)));
            assert_eq!(SendMessageW(time_slider, TBM_GETPOS, 0, 0), 0);
            panel::assert_thumb_pixel_for_test(time_slider, px(27));
            assert!(
                rx.try_recv().is_err(),
                "pointer motion must not save settings"
            );
            // Dragging beyond each end clamps the value; losing capture commits
            // exactly once, even if a later release message arrives.
            SendMessageW(time_slider, WM_MOUSEMOVE, 1, point(px(400)));
            assert_eq!(SendMessageW(time_slider, TBM_GETPOS, 0, 0), 8);
            SendMessageW(time_slider, WM_MOUSEMOVE, 1, point(-px(40)));
            assert_eq!(SendMessageW(time_slider, TBM_GETPOS, 0, 0), 0);
            SendMessageW(time_slider, WM_CAPTURECHANGED, 0, 0);
            assert!(matches!(rx.try_recv(), Ok(Command::TimeScale(0.0))));
            SendMessageW(time_slider, WM_LBUTTONUP, 0, point(px(10)));
            assert!(rx.try_recv().is_err());
            SendMessageW(
                time_slider,
                WM_KEYDOWN,
                windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_RIGHT as usize,
                0,
            );
            assert!(matches!(rx.try_recv(), Ok(Command::TimeScale(0.25))));
            let mut value = [0u16; 32];
            let len = GetWindowTextW(GetDlgItem(hwnd, 1047), value.as_mut_ptr(), 32);
            assert_eq!(String::from_utf16_lossy(&value[..len as usize]), "0.25×");
            let mode = GetDlgItem(hwnd, MODE_ID);
            SendMessageW(
                hwnd,
                WM_COMMAND,
                MODE_ID as usize | ((CBN_SELCHANGE as usize) << 16),
                mode,
            );
            assert!(matches!(rx.try_recv(), Ok(Command::Mode(ModelMode::Fish))));
            SendMessageW(hwnd, WM_CLOSE, 0, 0);
            assert_ne!(
                IsWindow(hwnd),
                0,
                "close must hide, preserving the tray window"
            );
            assert!(
                rx.try_recv().is_err(),
                "closing settings must not quit the app"
            );
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            DestroyWindow(hwnd);
            if context.font != 0 {
                DeleteObject(context.font);
            }
            DeleteObject(context.brush);
            UnregisterClassW(class.as_ptr(), module);
        }
    }
}

unsafe extern "system" fn enumerate_display(
    monitor: HMONITOR,
    _: HDC,
    _: *mut RECT,
    data: LPARAM,
) -> BOOL {
    let mut info: MONITORINFOEXW = zeroed();
    info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
    if GetMonitorInfoW(monitor, &mut info as *mut _ as *mut MONITORINFO) != 0 {
        let end = info
            .szDevice
            .iter()
            .position(|&v| v == 0)
            .unwrap_or(info.szDevice.len());
        let r = info.monitorInfo.rcMonitor;
        (*(data as *mut Vec<Display>)).push(Display {
            id: String::from_utf16_lossy(&info.szDevice[..end]),
            name: String::from_utf16_lossy(&info.szDevice[..end]),
            x: r.left,
            y: r.top,
            width: r.right - r.left,
            height: r.bottom - r.top,
        });
    }
    1
}
pub fn displays() -> Vec<Display> {
    let mut result: Vec<Display> = Vec::new();
    unsafe {
        EnumDisplayMonitors(
            0,
            null(),
            Some(enumerate_display),
            &mut result as *mut _ as isize,
        );
    }
    let names = display_names();
    for display in &mut result {
        if let Some((_, name)) = names.iter().find(|(id, _)| id == &display.id) {
            display.name = name.clone();
        }
    }
    result.sort_by(|a, b| a.id.cmp(&b.id));
    result
}
pub fn configure_displays(hwnd: isize, disabled: &[String]) -> anyhow::Result<Vec<Obstacle>> {
    let displays = displays();
    ensure!(!displays.is_empty(), "No active displays found");
    let left = displays.iter().map(|d| d.x).min().unwrap();
    let top = displays.iter().map(|d| d.y).min().unwrap();
    let right = displays.iter().map(|d| d.x + d.width).max().unwrap();
    let bottom = displays.iter().map(|d| d.y + d.height).max().unwrap();
    let active: Vec<_> = displays
        .iter()
        .filter(|d| !disabled.contains(&d.id))
        .collect();
    unsafe {
        let parent = GetParent(hwnd);
        let mut point = POINT { x: left, y: top };
        ScreenToClient(parent, &mut point);
        ensure!(
            SetWindowPos(
                hwnd,
                0,
                point.x,
                point.y,
                right - left,
                bottom - top,
                SWP_NOZORDER | SWP_NOACTIVATE
            ) != 0,
            "Could not resize virtual wallpaper"
        );
        let region = CreateRectRgn(0, 0, 0, 0);
        for d in &active {
            let part = CreateRectRgn(
                d.x - left,
                d.y - top,
                d.x + d.width - left,
                d.y + d.height - top,
            );
            CombineRgn(region, region, part, RGN_OR);
            DeleteObject(part);
        }
        if SetWindowRgn(hwnd, region, 1) == 0 {
            DeleteObject(region);
            bail!("Could not set display visibility");
        }
        ShowWindow(
            hwnd,
            if active.is_empty() {
                SW_HIDE
            } else {
                SW_SHOWNOACTIVATE
            },
        );
    }
    Ok(super::display_gaps(&displays, disabled))
}

fn display_names() -> Vec<(String, String)> {
    use windows_sys::Win32::Devices::Display::*;
    let decode = |s: &[u16]| {
        String::from_utf16_lossy(&s[..s.iter().position(|&c| c == 0).unwrap_or(s.len())])
    };
    unsafe {
        // Hotplug can change buffer requirements between the two calls.
        for _ in 0..3 {
            let (mut paths_count, mut modes_count) = (0, 0);
            if GetDisplayConfigBufferSizes(
                QDC_ONLY_ACTIVE_PATHS,
                &mut paths_count,
                &mut modes_count,
            ) != 0
            {
                break;
            }
            let mut paths: Vec<DISPLAYCONFIG_PATH_INFO> = vec![zeroed(); paths_count as usize];
            let mut modes: Vec<DISPLAYCONFIG_MODE_INFO> = vec![zeroed(); modes_count as usize];
            let error = QueryDisplayConfig(
                QDC_ONLY_ACTIVE_PATHS,
                &mut paths_count,
                paths.as_mut_ptr(),
                &mut modes_count,
                modes.as_mut_ptr(),
                null_mut(),
            );
            if error == ERROR_INSUFFICIENT_BUFFER {
                continue;
            }
            if error != 0 {
                break;
            }
            let mut names = Vec::new();
            for path in paths.iter().take(paths_count as usize) {
                let mut source: DISPLAYCONFIG_SOURCE_DEVICE_NAME = zeroed();
                source.header = DISPLAYCONFIG_DEVICE_INFO_HEADER {
                    r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
                    size: size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32,
                    adapterId: path.sourceInfo.adapterId,
                    id: path.sourceInfo.id,
                };
                let mut target: DISPLAYCONFIG_TARGET_DEVICE_NAME = zeroed();
                target.header = DISPLAYCONFIG_DEVICE_INFO_HEADER {
                    r#type: DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
                    size: size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32,
                    adapterId: path.targetInfo.adapterId,
                    id: path.targetInfo.id,
                };
                if DisplayConfigGetDeviceInfo(&mut source.header) == 0
                    && DisplayConfigGetDeviceInfo(&mut target.header) == 0
                {
                    let name = decode(&target.monitorFriendlyDeviceName);
                    if !name.trim().is_empty() {
                        names.push((decode(&source.viewGdiDeviceName), name));
                    }
                }
            }
            return names;
        }
    }
    vec![]
}
