//! Event-driven Deep Glass painting over standard, keyboard-accessible controls.
use super::*;
use std::cell::Cell;
use windows_sys::Win32::Graphics::Dwm::*;
use windows_sys::Win32::UI::Controls::{
    GetComboBoxInfo, COMBOBOXINFO, DRAWITEMSTRUCT, MARGINS, ODS_COMBOBOXEDIT, ODS_SELECTED,
    TBM_GETPAGESIZE, TBM_GETRANGEMAX, TBM_GETRANGEMIN, TB_ENDTRACK, WM_MOUSELEAVE,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    ReleaseCapture, SetCapture, VK_DOWN, VK_END, VK_HOME, VK_LEFT, VK_NEXT, VK_PRIOR, VK_RIGHT,
    VK_UP,
};

// Private, process-local registration keeps the EXE self-contained and does
// not install or change fonts on the user's Windows account.
const FONT_BYTES: &[u8] = include_bytes!("../assets/fonts/ZenKakuGothicNew-Regular.ttf");
pub(super) const FONT_FAMILY: &str = "Zen Kaku Gothic New";
pub(super) struct FontResource(HANDLE);
impl FontResource {
    pub(super) unsafe fn load() -> anyhow::Result<Self> {
        let mut count = 0u32;
        let handle = AddFontMemResourceEx(
            FONT_BYTES.as_ptr() as _,
            FONT_BYTES.len() as u32,
            null(),
            &mut count,
        );
        ensure!(handle != 0, "Could not load bundled settings font");
        Ok(Self(handle))
    }
}
impl Drop for FontResource {
    fn drop(&mut self) {
        unsafe {
            RemoveFontMemResourceEx(self.0);
        }
    }
}

pub(super) const CLOSE_ID: i32 = 1049;
const TEXT: u32 = rgb(225, 239, 249);
const MUTED: u32 = rgb(177, 207, 228);
const ACCENT: u32 = rgb(63, 175, 245);
const EDGE: u32 = rgb(54, 108, 150);
const fn rgb(r: u32, g: u32, b: u32) -> u32 {
    r | (g << 8) | (b << 16)
}
fn scale(hwnd: HWND, n: i32) -> i32 {
    unsafe { ((n as i64 * GetDpiForWindow(hwnd).max(96) as i64 + 48) / 96) as i32 }
}
unsafe fn line(dc: HDC, a: POINT, b: POINT, color: u32, width: i32) {
    let pen = CreatePen(PS_SOLID, width.max(1), color);
    let old = SelectObject(dc, pen);
    MoveToEx(dc, a.x, a.y, null_mut());
    LineTo(dc, b.x, b.y);
    SelectObject(dc, old);
    DeleteObject(pen);
}
unsafe fn round(dc: HDC, r: RECT, color: u32, border: u32, radius: i32) {
    let brush = CreateSolidBrush(color);
    let pen = CreatePen(PS_SOLID, 1, border);
    let old_b = SelectObject(dc, brush);
    let old_p = SelectObject(dc, pen);
    RoundRect(dc, r.left, r.top, r.right, r.bottom, radius, radius);
    SelectObject(dc, old_p);
    SelectObject(dc, old_b);
    DeleteObject(pen);
    DeleteObject(brush);
}
// Supersample just the small switch, leaving text at native resolution.
// Repaint only on UI events; no animation loop or new rendering dependency.
unsafe fn smooth_toggle(
    dc: HDC,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    inset: i32,
    knob: i32,
    progress: f32,
    fill: u32,
    border: u32,
) {
    const AA: i32 = 4;
    let memory = CreateCompatibleDC(dc);
    let bitmap = CreateCompatibleBitmap(dc, (w + 2) * AA, (h + 2) * AA);
    let old = if memory != 0 && bitmap != 0 {
        SelectObject(memory, bitmap)
    } else {
        0
    };
    let target = if old != 0 { memory } else { dc };
    let factor = if old != 0 { AA } else { 1 };
    let ox = if old != 0 { AA } else { x };
    let oy = if old != 0 { AA } else { y };
    if old != 0 {
        StretchBlt(
            memory,
            0,
            0,
            (w + 2) * AA,
            (h + 2) * AA,
            dc,
            x - 1,
            y - 1,
            w + 2,
            h + 2,
            SRCCOPY,
        );
    }
    round(
        target,
        RECT {
            left: ox,
            top: oy,
            right: ox + w * factor,
            bottom: oy + h * factor,
        },
        fill,
        border,
        h * factor,
    );
    let kx =
        ((inset as f32 + (w - knob - 2 * inset) as f32 * progress) * factor as f32).round() as i32;
    round(
        target,
        RECT {
            left: ox + kx,
            top: oy + inset * factor,
            right: ox + kx + knob * factor,
            bottom: oy + (inset + knob) * factor,
        },
        rgb(233, 246, 255),
        rgb(233, 246, 255),
        knob * factor,
    );
    if old != 0 {
        let mode = SetStretchBltMode(dc, HALFTONE as i32);
        let mut origin: POINT = zeroed();
        SetBrushOrgEx(dc, 0, 0, &mut origin);
        StretchBlt(
            dc,
            x - 1,
            y - 1,
            w + 2,
            h + 2,
            memory,
            0,
            0,
            (w + 2) * AA,
            (h + 2) * AA,
            SRCCOPY,
        );
        SetBrushOrgEx(dc, origin.x, origin.y, null_mut());
        SetStretchBltMode(dc, mode);
        SelectObject(memory, old);
    }
    if bitmap != 0 {
        DeleteObject(bitmap);
    }
    if memory != 0 {
        DeleteDC(memory);
    }
}
unsafe fn smooth_drawing(dc: HDC, r: RECT, draw: impl FnOnce(HDC)) {
    let w = r.right - r.left;
    let h = r.bottom - r.top;
    let memory = CreateCompatibleDC(dc);
    let bitmap = CreateCompatibleBitmap(dc, w.max(1) * 4, h.max(1) * 4);
    if memory != 0 && bitmap != 0 {
        let old = SelectObject(memory, bitmap);
        StretchBlt(memory, 0, 0, w * 4, h * 4, dc, r.left, r.top, w, h, SRCCOPY);
        let saved = SaveDC(memory);
        SetMapMode(memory, MM_ANISOTROPIC);
        SetWindowExtEx(memory, w, h, null_mut());
        SetViewportExtEx(memory, w * 4, h * 4, null_mut());
        SetWindowOrgEx(memory, r.left, r.top, null_mut());
        draw(memory);
        RestoreDC(memory, saved);
        let mode = SetStretchBltMode(dc, HALFTONE as i32);
        let mut origin: POINT = zeroed();
        SetBrushOrgEx(dc, 0, 0, &mut origin);
        StretchBlt(dc, r.left, r.top, w, h, memory, 0, 0, w * 4, h * 4, SRCCOPY);
        SetBrushOrgEx(dc, origin.x, origin.y, null_mut());
        SetStretchBltMode(dc, mode);
        SelectObject(memory, old);
    } else {
        draw(dc);
    }
    if bitmap != 0 {
        DeleteObject(bitmap);
    }
    if memory != 0 {
        DeleteDC(memory);
    }
}
pub(super) unsafe fn style_scene_list(hwnd: HWND) {
    let mut info: COMBOBOXINFO = zeroed();
    info.cbSize = size_of::<COMBOBOXINFO>() as u32;
    if GetComboBoxInfo(hwnd, &mut info) != 0 {
        let list = info.hwndList;
        SetWindowLongPtrW(
            list,
            GWL_STYLE,
            GetWindowLongPtrW(list, GWL_STYLE) & !(WS_BORDER as isize),
        );
        SetWindowPos(
            list,
            0,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
        let mut rect: RECT = zeroed();
        GetWindowRect(list, &mut rect);
        let region = CreateRoundRectRgn(
            0,
            0,
            rect.right - rect.left,
            rect.bottom - rect.top,
            scale(hwnd, 14),
            scale(hwnd, 14),
        );
        if SetWindowRgn(list, region, 1) == 0 {
            DeleteObject(region);
        }
    }
}
pub(super) unsafe fn scene_item(item: &DRAWITEMSTRUCT) {
    if item.itemState & ODS_COMBOBOXEDIT != 0 {
        InvalidateRect(item.hwndItem, null(), 0);
        return;
    }
    let dc = item.hDC;
    let r = item.rcItem;
    let brush = CreateSolidBrush(rgb(12, 36, 59));
    FillRect(dc, &r, brush);
    DeleteObject(brush);
    let inset = scale(item.hwndItem, 4);
    let tile = RECT {
        left: r.left + inset,
        top: r.top + inset,
        right: r.right - inset,
        bottom: r.bottom - inset,
    };
    if item.itemState & ODS_SELECTED != 0 {
        smooth_drawing(dc, r, |target| {
            round(
                target,
                tile,
                rgb(29, 76, 112),
                EDGE,
                scale(item.hwndItem, 12),
            )
        });
    }
    let font = SendMessageW(item.hwndItem, WM_GETFONT, 0, 0);
    let old = SelectObject(dc, font);
    if let Some(mode) = ModelMode::ALL.get(item.itemID as usize) {
        text(
            dc,
            mode.label(),
            RECT {
                left: r.left + scale(item.hwndItem, 14),
                ..r
            },
            TEXT,
            DT_LEFT,
        );
    }
    SelectObject(dc, old);
}
unsafe fn text(dc: HDC, label: &str, mut rect: RECT, color: u32, align: u32) {
    SetBkMode(dc, TRANSPARENT as i32);
    SetTextColor(dc, color);
    let s = wide(label);
    DrawTextW(
        dc,
        s.as_ptr(),
        (s.len() - 1) as i32,
        &mut rect,
        DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX | align,
    );
}
fn gradient_color(y: i32, height: i32) -> (u32, u32, u32) {
    let t = (y as f32 / height.max(1) as f32).clamp(0., 1.);
    let color = |a: f32, b: f32| (a + (b - a) * t) as u32;
    (color(21., 8.), color(61., 27.), color(99., 49.))
}
unsafe fn backdrop(dc: HDC, rect: RECT, offset: i32, height: i32) {
    let a = gradient_color(offset, height);
    let b = gradient_color(offset + rect.bottom, height);
    let vertices = [
        TRIVERTEX {
            x: rect.left,
            y: rect.top,
            Red: (a.0 << 8) as u16,
            Green: (a.1 << 8) as u16,
            Blue: (a.2 << 8) as u16,
            Alpha: 0,
        },
        TRIVERTEX {
            x: rect.right,
            y: rect.bottom,
            Red: (b.0 << 8) as u16,
            Green: (b.1 << 8) as u16,
            Blue: (b.2 << 8) as u16,
            Alpha: 0,
        },
    ];
    let mesh = GRADIENT_RECT {
        UpperLeft: 0,
        LowerRight: 1,
    };
    GradientFill(
        dc,
        vertices.as_ptr(),
        2,
        &mesh as *const _ as _,
        1,
        GRADIENT_FILL_RECT_V,
    );
}
pub(super) unsafe fn frame(hwnd: HWND, dc: HDC) {
    let mut r: RECT = zeroed();
    GetClientRect(hwnd, &mut r);
    backdrop(dc, r, 0, r.bottom);
    let s = |n| scale(hwnd, n);
    // DWM owns the outer border and anti-aliased corners. Drawing a second
    // GDI outline here produces mismatched radii and jagged bright fragments.
    for y in [56, 112, 158, 200, 242, 318, 394, 470] {
        line(
            dc,
            POINT { x: s(20), y: s(y) },
            POINT {
                x: r.right - s(20),
                y: s(y),
            },
            rgb(36, 76, 108),
            s(1),
        );
    }
    let icon = LoadImageW(
        GetModuleHandleW(null()),
        1usize as *const u16,
        IMAGE_ICON,
        s(28),
        s(28),
        LR_SHARED,
    ) as HICON;
    if icon != 0 {
        DrawIconEx(dc, s(16), s(16), icon, s(28), s(28), 0, 0, DI_NORMAL);
    }
}
pub(super) unsafe fn apply_frame(hwnd: HWND) {
    // Window regions disable DWM corner rounding, even with an explicit hint.
    SetWindowRgn(hwnd, 0, 0);
    let corner = DWMWCP_ROUND;
    let _ = DwmSetWindowAttribute(
        hwnd,
        DWMWA_WINDOW_CORNER_PREFERENCE as u32,
        &corner as *const _ as _,
        4,
    );
    let border = EDGE;
    let _ = DwmSetWindowAttribute(hwnd, DWMWA_BORDER_COLOR as u32, &border as *const _ as _, 4);
    let margins = MARGINS {
        cxLeftWidth: 1,
        cxRightWidth: 1,
        cyTopHeight: 1,
        cyBottomHeight: 1,
    };
    let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);
    let mode = GetDlgItem(hwnd, MODE_ID);
    SendMessageW(mode, CB_SETITEMHEIGHT, usize::MAX, scale(hwnd, 26) as isize);
    SendMessageW(mode, CB_SETITEMHEIGHT, 0, scale(hwnd, 40) as isize);
    InvalidateRect(hwnd, null(), 1);
}
pub(super) unsafe fn install(hwnd: HWND, id: i32) -> bool {
    let state = Box::into_raw(Box::new(ControlState {
        id,
        drag_x: Cell::new(None),
        grab_offset: Cell::new(0),
        wheel_delta: Cell::new(0),
        checked: Cell::new(false),
        animation_start: Cell::new(None),
        animation_from: Cell::new(0.0),
    }));
    if SetWindowSubclass(hwnd, Some(control_proc), 1, state as usize) == 0 {
        drop(Box::from_raw(state));
        return false;
    }
    true
}
struct ControlState {
    id: i32,
    // Cells permit synchronous parent notifications without holding a mutable
    // reference across a reentrant window procedure.
    drag_x: Cell<Option<i32>>,
    grab_offset: Cell<i32>,
    wheel_delta: Cell<i32>,
    checked: Cell<bool>,
    animation_start: Cell<Option<Instant>>,
    animation_from: Cell<f32>,
}
const TOGGLE_TIMER: usize = 7;
const TOGGLE_DURATION: f32 = 0.18;
fn toggle_progress(from: f32, checked: bool, elapsed: f32) -> f32 {
    let t = (elapsed / TOGGLE_DURATION).clamp(0.0, 1.0);
    let eased = t * t * (3.0 - 2.0 * t);
    from + (if checked { 1.0 } else { 0.0 } - from) * eased
}
fn toggle_position(state: &ControlState) -> f32 {
    state
        .animation_start
        .get()
        .map_or(if state.checked.get() { 1.0 } else { 0.0 }, |start| {
            toggle_progress(
                state.animation_from.get(),
                state.checked.get(),
                start.elapsed().as_secs_f32(),
            )
        })
}
unsafe fn slider_geometry(hwnd: HWND) -> (i32, i32, i32, i32) {
    let mut r: RECT = zeroed();
    GetClientRect(hwnd, &mut r);
    let left = scale(hwnd, 10);
    let right = (r.right - left).max(left + 1);
    let min = SendMessageW(hwnd, TBM_GETRANGEMIN, 0, 0) as i32;
    let max = SendMessageW(hwnd, TBM_GETRANGEMAX, 0, 0) as i32;
    (left, right, min, max.max(min + 1))
}
unsafe fn slider_center(hwnd: HWND, state: &ControlState) -> i32 {
    let (left, right, min, max) = slider_geometry(hwnd);
    state.drag_x.get().unwrap_or_else(|| {
        let pos = SendMessageW(hwnd, TBM_GETPOS, 0, 0) as i32;
        left + ((pos - min).clamp(0, max - min) * (right - left) + (max - min) / 2) / (max - min)
    })
}
unsafe fn notify_slider(hwnd: HWND, code: u32) {
    let pos = SendMessageW(hwnd, TBM_GETPOS, 0, 0) as usize;
    SendMessageW(
        GetParent(hwnd),
        WM_HSCROLL,
        code as usize | (pos << 16),
        hwnd,
    );
}
unsafe fn drag_slider(hwnd: HWND, state: &ControlState, mouse_x: i32) {
    let (left, right, min, max) = slider_geometry(hwnd);
    let x = (mouse_x - state.grab_offset.get()).clamp(left, right);
    if state.drag_x.get() == Some(x) {
        return;
    }
    state.drag_x.set(Some(x));
    let pos = min + ((x - left) * (max - min) + (right - left) / 2) / (right - left);
    SendMessageW(hwnd, TBM_SETPOS, 0, pos as isize);
    notify_slider(hwnd, TB_THUMBTRACK);
    InvalidateRect(hwnd, null(), 0);
}
unsafe fn finish_drag(hwnd: HWND, state: &ControlState) {
    if state.drag_x.take().is_some() {
        // Snap once on release; the thumb follows individual pixels while held.
        InvalidateRect(hwnd, null(), 0);
        notify_slider(hwnd, TB_ENDTRACK);
    }
}
unsafe fn slider_shapes(hwnd: HWND, dc: HDC, state: &ControlState, r: RECT, focused: bool) {
    let s = |n| scale(hwnd, n);
    let (left, right, _, _) = slider_geometry(hwnd);
    let y = r.bottom / 2;
    let half = s(3);
    let x = slider_center(hwnd, state);
    round(
        dc,
        RECT {
            left: left,
            top: y - half,
            right: right,
            bottom: y + half,
        },
        rgb(46, 79, 108),
        rgb(46, 79, 108),
        s(6),
    );
    if x > left {
        round(
            dc,
            RECT {
                left: left,
                top: y - half,
                right: x,
                bottom: y + half,
            },
            ACCENT,
            ACCENT,
            s(6),
        );
    }
    let radius = s(8);
    round(
        dc,
        RECT {
            left: x - radius - s(2),
            top: y - radius - s(2),
            right: x + radius + s(2),
            bottom: y + radius + s(2),
        },
        rgb(23, 91, 141),
        if focused {
            rgb(142, 214, 255)
        } else {
            rgb(33, 139, 213)
        },
        s(22),
    );
    round(
        dc,
        RECT {
            left: x - radius,
            top: y - radius,
            right: x + radius,
            bottom: y + radius,
        },
        rgb(231, 247, 255),
        ACCENT,
        s(18),
    );
}
unsafe fn smooth_slider(hwnd: HWND, dc: HDC, state: &ControlState, r: RECT, focused: bool) {
    const AA: i32 = 4;
    let memory = CreateCompatibleDC(dc);
    let bitmap = CreateCompatibleBitmap(dc, r.right.max(1) * AA, r.bottom.max(1) * AA);
    if memory != 0 && bitmap != 0 {
        let old = SelectObject(memory, bitmap);
        StretchBlt(
            memory,
            0,
            0,
            r.right * AA,
            r.bottom * AA,
            dc,
            0,
            0,
            r.right,
            r.bottom,
            SRCCOPY,
        );
        let saved = SaveDC(memory);
        SetMapMode(memory, MM_ANISOTROPIC);
        SetWindowExtEx(memory, r.right, r.bottom, null_mut());
        SetViewportExtEx(memory, r.right * AA, r.bottom * AA, null_mut());
        slider_shapes(hwnd, memory, state, r, focused);
        RestoreDC(memory, saved);
        let mode = SetStretchBltMode(dc, HALFTONE as i32);
        let mut origin: POINT = zeroed();
        SetBrushOrgEx(dc, 0, 0, &mut origin);
        StretchBlt(
            dc,
            0,
            0,
            r.right,
            r.bottom,
            memory,
            0,
            0,
            r.right * AA,
            r.bottom * AA,
            SRCCOPY,
        );
        SetBrushOrgEx(dc, origin.x, origin.y, null_mut());
        SetStretchBltMode(dc, mode);
        SelectObject(memory, old);
    } else {
        slider_shapes(hwnd, dc, state, r, focused);
    }
    if bitmap != 0 {
        DeleteObject(bitmap);
    }
    if memory != 0 {
        DeleteDC(memory);
    }
}
unsafe fn paint_control(hwnd: HWND, dc: HDC, state: &ControlState) {
    let id = state.id;
    let mut r: RECT = zeroed();
    GetClientRect(hwnd, &mut r);
    let parent = GetParent(hwnd);
    let mut parent_rect: RECT = zeroed();
    GetClientRect(parent, &mut parent_rect);
    let mut origin = POINT { x: 0, y: 0 };
    ClientToScreen(hwnd, &mut origin);
    ScreenToClient(parent, &mut origin);
    backdrop(dc, r, origin.y, parent_rect.bottom);
    let font = SendMessageW(hwnd, WM_GETFONT, 0, 0);
    let old_font = SelectObject(
        dc,
        if font != 0 {
            font
        } else {
            GetStockObject(DEFAULT_GUI_FONT)
        },
    );
    let s = |n| scale(hwnd, n);
    let mut buf = [0u16; 128];
    let len = GetWindowTextW(hwnd, buf.as_mut_ptr(), 128);
    let label = String::from_utf16_lossy(&buf[..len.max(0) as usize]);
    let focused = windows_sys::Win32::UI::Input::KeyboardAndMouse::GetFocus() == hwnd;
    match id {
        1002..=1004 => {
            text(
                dc,
                &label,
                RECT {
                    right: r.right - s(64),
                    ..r
                },
                TEXT,
                DT_LEFT,
            );
            let h = s(24);
            let w = s(44);
            let y = (r.bottom - h) / 2;
            let x = r.right - w - s(2);
            let progress = toggle_position(state);
            let blend = |a: f32, b: f32| (a + (b - a) * progress).round() as u32;
            let fill = rgb(blend(31.0, 28.0), blend(65.0, 139.0), blend(94.0, 224.0));
            smooth_toggle(
                dc,
                x,
                y,
                w,
                h,
                s(3),
                s(18),
                progress,
                fill,
                if focused { rgb(157, 218, 250) } else { fill },
            );
        }
        COUNT_ID | FPS_ID | TIME_ID => {
            smooth_slider(hwnd, dc, state, r, focused);
        }
        MODE_ID => {
            smooth_drawing(dc, r, |dc| {
                round(
                    dc,
                    RECT {
                        left: 1,
                        top: 1,
                        right: r.right - 1,
                        bottom: r.bottom - 1,
                    },
                    rgb(23, 57, 88),
                    if focused { ACCENT } else { EDGE },
                    s(14),
                )
            });
            let index = SendMessageW(hwnd, CB_GETCURSEL, 0, 0);
            let value = ModelMode::ALL
                .get(index as usize)
                .copied()
                .unwrap_or_default()
                .label();
            text(
                dc,
                value,
                RECT {
                    left: s(13),
                    right: r.right - s(28),
                    ..r
                },
                TEXT,
                DT_LEFT,
            );
            let x = r.right - s(18);
            let y = r.bottom / 2;
            smooth_drawing(
                dc,
                RECT {
                    left: x - s(6),
                    top: y - s(5),
                    right: x + s(6),
                    bottom: y + s(5),
                },
                |dc| {
                    line(
                        dc,
                        POINT {
                            x: x - s(4),
                            y: y - s(2),
                        },
                        POINT { x, y: y + s(2) },
                        MUTED,
                        s(1),
                    );
                    line(
                        dc,
                        POINT { x, y: y + s(2) },
                        POINT {
                            x: x + s(4),
                            y: y - s(2),
                        },
                        MUTED,
                        s(1),
                    );
                },
            );
        }
        CLOSE_ID => {
            if focused {
                round(
                    dc,
                    RECT {
                        left: 1,
                        top: 1,
                        right: r.right - 1,
                        bottom: r.bottom - 1,
                    },
                    rgb(29, 65, 96),
                    EDGE,
                    s(10),
                );
            }
            let x = r.right / 2;
            let y = r.bottom / 2;
            let h = s(5);
            line(
                dc,
                POINT { x: x - h, y: y - h },
                POINT { x: x + h, y: y + h },
                TEXT,
                s(1),
            );
            line(
                dc,
                POINT { x: x + h, y: y - h },
                POINT { x: x - h, y: y + h },
                TEXT,
                s(1),
            );
        }
        QUIT_ID => {
            if focused {
                round(
                    dc,
                    RECT {
                        left: 1,
                        top: 1,
                        right: r.right - 1,
                        bottom: r.bottom - 1,
                    },
                    rgb(25, 55, 81),
                    EDGE,
                    s(10),
                );
            }
            let x = s(12);
            let y = r.bottom / 2;
            let pen = CreatePen(PS_SOLID, s(1), MUTED);
            let old = SelectObject(dc, pen);
            let brush = SelectObject(dc, GetStockObject(NULL_BRUSH));
            Arc(
                dc,
                x - s(7),
                y - s(7),
                x + s(7),
                y + s(7),
                x - s(3),
                y - s(7),
                x + s(3),
                y - s(7),
            );
            SelectObject(dc, brush);
            SelectObject(dc, old);
            DeleteObject(pen);
            line(
                dc,
                POINT { x, y: y - s(9) },
                POINT { x, y: y - s(1) },
                MUTED,
                s(1),
            );
            text(dc, &label, RECT { left: s(30), ..r }, MUTED, DT_LEFT);
        }
        1045 | 1046 | 1047 => {
            text(dc, &label, r, TEXT, DT_RIGHT);
        }
        _ => text(
            dc,
            &label,
            r,
            if id == 1040 { TEXT } else { MUTED },
            DT_LEFT,
        ),
    }
    SelectObject(dc, old_font);
}
unsafe extern "system" fn control_proc(
    hwnd: HWND,
    msg: u32,
    w: WPARAM,
    l: LPARAM,
    id: usize,
    data: usize,
) -> LRESULT {
    if msg == WM_NCDESTROY {
        KillTimer(hwnd, TOGGLE_TIMER);
        RemoveWindowSubclass(hwnd, Some(control_proc), id);
        let result = DefSubclassProc(hwnd, msg, w, l);
        drop(Box::from_raw(data as *mut ControlState));
        return result;
    }
    let state = &*(data as *const ControlState);
    if state.id == MODE_ID {
        match msg {
            WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => {
                SetFocus(hwnd);
                let open = SendMessageW(hwnd, CB_GETDROPPEDSTATE, 0, 0) == 0;
                SendMessageW(hwnd, CB_SHOWDROPDOWN, usize::from(open), 0);
                InvalidateRect(hwnd, null(), 0);
                return 0;
            }
            WM_LBUTTONUP | WM_MOUSEMOVE | WM_MOUSELEAVE => return 0,
            WM_SETFOCUS | WM_KILLFOCUS => {
                InvalidateRect(hwnd, null(), 0);
                return 0;
            }
            _ => {}
        }
    }
    if (1002..=1004).contains(&state.id) {
        // A rapid second press arrives as WM_LBUTTONDBLCLK. Treat it as
        // another press so each release produces one BN_CLICKED notification.
        if msg == WM_LBUTTONDBLCLK {
            return DefSubclassProc(hwnd, WM_LBUTTONDOWN, w, l);
        }
        match msg {
            BM_GETCHECK => return isize::from(state.checked.get()),
            BM_SETCHECK => {
                let checked = w != 0;
                if checked != state.checked.get() {
                    let from = toggle_position(state);
                    state.checked.set(checked);
                    if IsWindowVisible(hwnd) != 0 {
                        state.animation_from.set(from);
                        state.animation_start.set(Some(Instant::now()));
                        if SetTimer(hwnd, TOGGLE_TIMER, 16, None) == 0 {
                            state.animation_start.set(None);
                        }
                    } else {
                        state.animation_start.set(None);
                        KillTimer(hwnd, TOGGLE_TIMER);
                    }
                    InvalidateRect(hwnd, null(), 0);
                }
                return 0;
            }
            WM_TIMER if w == TOGGLE_TIMER => {
                if state.animation_start.get().map_or(true, |start| {
                    start.elapsed().as_secs_f32() >= TOGGLE_DURATION
                }) || IsWindowVisible(hwnd) == 0
                {
                    state.animation_start.set(None);
                    KillTimer(hwnd, TOGGLE_TIMER);
                }
                InvalidateRect(hwnd, null(), 0);
                return 0;
            }
            _ => {}
        }
    }
    if matches!(state.id, 1045..=1047) && msg == WM_SETTEXT {
        // STATIC's class procedure paints immediately on WM_SETTEXT. Store
        // accessible window text without that native white-background drawing.
        let result = DefWindowProcW(hwnd, msg, w, l);
        InvalidateRect(hwnd, null(), 0);
        return result;
    }
    if matches!(state.id, COUNT_ID | FPS_ID | TIME_ID) {
        match msg {
            WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => {
                SetFocus(hwnd);
                SetCapture(hwnd);
                let x = l as u16 as i16 as i32;
                let center = slider_center(hwnd, state);
                state
                    .grab_offset
                    .set(if (x - center).abs() <= scale(hwnd, 10) {
                        x - center
                    } else {
                        0
                    });
                drag_slider(hwnd, state, x);
                return 0;
            }
            WM_MOUSEMOVE if state.drag_x.get().is_some() => {
                drag_slider(hwnd, state, l as u16 as i16 as i32);
                return 0;
            }
            WM_MOUSEMOVE | WM_MOUSELEAVE | WM_KEYUP => return 0,
            WM_MOUSEWHEEL => {
                let delta = state.wheel_delta.get() + (w >> 16) as u16 as i16 as i32;
                state.wheel_delta.set(delta % 120);
                if delta / 120 != 0 {
                    let (_, _, min, max) = slider_geometry(hwnd);
                    let current = SendMessageW(hwnd, TBM_GETPOS, 0, 0) as i32;
                    SendMessageW(
                        hwnd,
                        TBM_SETPOS,
                        0,
                        (current + delta / 120).clamp(min, max) as isize,
                    );
                    notify_slider(hwnd, TB_ENDTRACK);
                }
                return 0;
            }
            WM_LBUTTONUP if state.drag_x.get().is_some() => {
                drag_slider(hwnd, state, l as u16 as i16 as i32);
                finish_drag(hwnd, state);
                ReleaseCapture();
                return 0;
            }
            WM_CAPTURECHANGED | WM_CANCELMODE => {
                finish_drag(hwnd, state);
                if msg == WM_CANCELMODE {
                    ReleaseCapture();
                }
                return 0;
            }
            TBM_SETPOS => {
                // Native trackbars also paint synchronously on state changes.
                // Keep their range/value/accessibility, but own all drawing.
                let result = DefSubclassProc(hwnd, msg, 0, l);
                InvalidateRect(hwnd, null(), 0);
                return result;
            }
            WM_SETFOCUS | WM_KILLFOCUS => {
                InvalidateRect(hwnd, null(), 0);
                return 0;
            }
            WM_KEYDOWN => {
                let (_, _, min, max) = slider_geometry(hwnd);
                let current = SendMessageW(hwnd, TBM_GETPOS, 0, 0) as i32;
                let page = SendMessageW(hwnd, TBM_GETPAGESIZE, 0, 0) as i32;
                let next = match w as u16 {
                    VK_LEFT | VK_UP => current - 1,
                    VK_RIGHT | VK_DOWN => current + 1,
                    VK_PRIOR => current - page,
                    VK_NEXT => current + page,
                    VK_HOME => min,
                    VK_END => max,
                    _ => return DefSubclassProc(hwnd, msg, w, l),
                }
                .clamp(min, max);
                SendMessageW(hwnd, TBM_SETPOS, 0, next as isize);
                notify_slider(hwnd, TB_ENDTRACK);
                return 0;
            }
            _ => {}
        }
    }
    if msg == WM_ERASEBKGND || msg == WM_NCPAINT {
        return 1;
    }
    if msg == WM_PRINTCLIENT {
        paint_control(hwnd, w as HDC, state);
        return 0;
    }
    if msg == WM_PAINT {
        let mut ps: PAINTSTRUCT = zeroed();
        let dc = BeginPaint(hwnd, &mut ps);
        let mut r: RECT = zeroed();
        GetClientRect(hwnd, &mut r);
        let memory = CreateCompatibleDC(dc);
        let bitmap = CreateCompatibleBitmap(dc, r.right.max(1), r.bottom.max(1));
        if memory != 0 && bitmap != 0 {
            let old = SelectObject(memory, bitmap);
            paint_control(hwnd, memory, state);
            BitBlt(dc, 0, 0, r.right, r.bottom, memory, 0, 0, SRCCOPY);
            SelectObject(memory, old);
        } else {
            paint_control(hwnd, dc, state);
        }
        if bitmap != 0 {
            DeleteObject(bitmap);
        }
        if memory != 0 {
            DeleteDC(memory);
        }
        EndPaint(hwnd, &ps);
        return 0;
    }
    let result = DefSubclassProc(hwnd, msg, w, l);
    if matches!(
        msg,
        WM_SETFOCUS | WM_KILLFOCUS | BM_SETCHECK | TBM_SETPOS | CB_SETCURSEL
    ) {
        InvalidateRect(hwnd, null(), 0);
    }
    result
}

#[cfg(test)]
pub(super) unsafe fn capture_for_test(hwnd: HWND, path: &std::path::Path) {
    let mut r: RECT = zeroed();
    GetClientRect(hwnd, &mut r);
    let dc = GetDC(hwnd);
    let memory = CreateCompatibleDC(dc);
    let bitmap = CreateCompatibleBitmap(dc, r.right, r.bottom);
    assert_ne!(memory, 0);
    assert_ne!(bitmap, 0);
    let old = SelectObject(memory, bitmap);
    frame(hwnd, memory);
    for &(id, _, _, _, _, _, _, _) in &PANEL_CONTROLS {
        let child = GetDlgItem(hwnd, id);
        let mut rect: RECT = zeroed();
        GetClientRect(child, &mut rect);
        let mut p = POINT { x: 0, y: 0 };
        ClientToScreen(child, &mut p);
        ScreenToClient(hwnd, &mut p);
        let saved = SaveDC(memory);
        SetViewportOrgEx(memory, p.x, p.y, null_mut());
        IntersectClipRect(memory, 0, 0, rect.right, rect.bottom);
        SendMessageW(child, WM_PRINTCLIENT, memory as usize, PRF_CLIENT as isize);
        RestoreDC(memory, saved);
    }
    SelectObject(memory, old);
    let mut info: BITMAPINFO = zeroed();
    info.bmiHeader = BITMAPINFOHEADER {
        biSize: size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: r.right,
        biHeight: -r.bottom,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB,
        ..zeroed()
    };
    let mut bits = vec![0u8; (r.right * r.bottom * 4) as usize];
    assert_eq!(
        GetDIBits(
            dc,
            bitmap,
            0,
            r.bottom as u32,
            bits.as_mut_ptr() as _,
            &mut info,
            DIB_RGB_COLORS
        ),
        r.bottom
    );
    assert!(bits.iter().any(|&v| v > 0));
    let mut output = Vec::new();
    output.extend_from_slice(b"BM");
    output.extend_from_slice(&(54 + bits.len() as u32).to_le_bytes());
    output.extend_from_slice(&[0; 4]);
    output.extend_from_slice(&54u32.to_le_bytes());
    output.extend_from_slice(&40u32.to_le_bytes());
    output.extend_from_slice(&r.right.to_le_bytes());
    output.extend_from_slice(&(-r.bottom).to_le_bytes());
    output.extend_from_slice(&1u16.to_le_bytes());
    output.extend_from_slice(&32u16.to_le_bytes());
    output.extend_from_slice(&[0; 24]);
    output.extend_from_slice(&bits);
    std::fs::write(path, output).unwrap();
    DeleteObject(bitmap);
    DeleteDC(memory);
    ReleaseDC(hwnd, dc);
}

#[cfg(test)]
pub(super) unsafe fn assert_thumb_pixel_for_test(hwnd: HWND, x: i32) {
    let mut rect: RECT = zeroed();
    GetClientRect(hwnd, &mut rect);
    let dc = GetDC(hwnd);
    let memory = CreateCompatibleDC(dc);
    let bitmap = CreateCompatibleBitmap(dc, rect.right, rect.bottom);
    assert_ne!(memory, 0);
    assert_ne!(bitmap, 0);
    let old = SelectObject(memory, bitmap);
    SendMessageW(hwnd, WM_PRINTCLIENT, memory as usize, PRF_CLIENT as isize);
    assert_eq!(
        GetPixel(memory, x, rect.bottom / 2),
        rgb(231, 247, 255),
        "thumb must follow the pointer between discrete value steps"
    );
    SelectObject(memory, old);
    DeleteObject(bitmap);
    DeleteDC(memory);
    ReleaseDC(hwnd, dc);
}

#[cfg(test)]
mod animation_tests {
    use super::*;
    #[test]
    fn toggle_animation_reverses_continuously_and_finishes() {
        assert_eq!(toggle_progress(0.0, true, 0.0), 0.0);
        assert_eq!(toggle_progress(0.0, true, TOGGLE_DURATION), 1.0);
        let midpoint = toggle_progress(0.0, true, TOGGLE_DURATION / 2.0);
        assert!((midpoint - 0.5).abs() < 0.001);
        assert_eq!(toggle_progress(midpoint, false, 0.0), midpoint);
        assert_eq!(toggle_progress(midpoint, false, TOGGLE_DURATION), 0.0);
    }
}
