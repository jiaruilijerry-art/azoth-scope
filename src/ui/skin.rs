//! Native control input/accessibility, custom dashboard paint. Coordinates are
//! design units at 96 DPI, rasterized directly at the monitor's native DPI.
use super::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetFocus, IsWindowEnabled};

pub const WIDTH: i32 = 852;
pub const HEIGHT: i32 = 592;
const fn color(hex: u32) -> u32 {
    ((hex & 0xff) << 16) | (hex & 0xff00) | (hex >> 16)
}
pub const BASE: u32 = color(0x10161c);
pub const PANEL: u32 = color(0x182128);
const FIELD: u32 = color(0x212e36);
const EDGE: u32 = color(0x334149);
const INK: u32 = color(0xeff5f4);
pub const SUB: u32 = color(0xacbbbf);
const MINT: u32 = color(0x78edbf);
const ICE: u32 = color(0x9eb9ff);
const GLASS: u32 = color(0x080e11);

struct Fonts {
    body: HFONT,
    title: HFONT,
    section: HFONT,
    micro: HFONT,
    value: HFONT,
}
impl Drop for Fonts {
    fn drop(&mut self) {
        for f in [self.body, self.title, self.section, self.micro, self.value] {
            unsafe {
                DeleteObject(f);
            }
        }
    }
}
thread_local! {static FONTS:RefCell<HashMap<u32,Fonts>>=RefCell::new(HashMap::new());}
unsafe fn font(index: u8) -> HFONT {
    FONTS.with(|cell| {
        let face = |size: i32, weight, name| {
            CreateFontW(
                -px(size),
                0,
                0,
                0,
                weight,
                0,
                0,
                0,
                DEFAULT_CHARSET as u32,
                0,
                0,
                CLEARTYPE_QUALITY as u32,
                0,
                w(name).as_ptr(),
            )
        };
        let mut fonts = cell.borrow_mut();
        let f = fonts.entry(DPI.with(Cell::get)).or_insert_with(|| Fonts {
            body: face(14, 400, "Segoe UI"),
            title: face(30, 600, "Segoe UI"),
            section: face(20, 600, "Segoe UI"),
            micro: face(12, 500, "Consolas"),
            value: face(33, 600, "Segoe UI"),
        });
        match index {
            1 => f.title,
            2 => f.section,
            3 => f.micro,
            4 => f.value,
            _ => f.body,
        }
    })
}
pub unsafe fn body_font() -> HFONT {
    font(0)
}
fn rect(x: i32, y: i32, width: i32, height: i32) -> RECT {
    RECT {
        left: px(x),
        top: px(y),
        right: px(x + width),
        bottom: px(y + height),
    }
}
unsafe fn fill(dc: HDC, r: &RECT, c: u32) {
    SetDCBrushColor(dc, c);
    FillRect(dc, r, GetStockObject(DC_BRUSH));
}
unsafe fn box_(dc: HDC, r: RECT, radius: i32, inside: u32, border: u32) {
    let b = SelectObject(dc, GetStockObject(DC_BRUSH));
    let p = SelectObject(dc, GetStockObject(DC_PEN));
    SetDCBrushColor(dc, inside);
    SetDCPenColor(dc, border);
    RoundRect(dc, r.left, r.top, r.right, r.bottom, px(radius), px(radius));
    SelectObject(dc, b);
    SelectObject(dc, p);
}
unsafe fn txt(dc: HDC, s: &str, r: RECT, ink: u32, size: u8, align: u32) {
    let old = SelectObject(dc, font(size));
    SetBkMode(dc, TRANSPARENT as i32);
    SetTextColor(dc, ink);
    let mut r = r;
    DrawTextW(
        dc,
        w(s).as_ptr(),
        -1,
        &mut r,
        DT_NOPREFIX | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | align,
    );
    SelectObject(dc, old);
}
unsafe fn stroke(dc: HDC, color: u32, points: &[(i32, i32)]) {
    let old = SelectObject(dc, GetStockObject(DC_PEN));
    SetDCPenColor(dc, color);
    if let Some((x, y)) = points.first() {
        MoveToEx(dc, px(*x), px(*y), null_mut());
        for (x, y) in &points[1..] {
            LineTo(dc, px(*x), px(*y));
        }
    }
    SelectObject(dc, old);
}

pub unsafe fn dashboard(h: HWND) {
    let mut ps = zeroed();
    let screen = BeginPaint(h, &mut ps);
    let dc = CreateCompatibleDC(screen);
    let bmp = CreateCompatibleBitmap(screen, px(WIDTH), px(HEIGHT));
    let old = SelectObject(dc, bmp);
    let a = state();
    let s = a.0.lock().unwrap();
    let l = s.pref.language;
    let timing = s.pref.timing;
    let pix = s.picture.clone();
    let battery = s.battery;
    let (i, j) = s.indices();
    let dual = !s.pref.one;
    let metrics = [i, j].map(|i| {
        (
            s.sources[i].short.clone(),
            s.sources[i].name.clone(),
            s.history[i].back().and_then(|(_, v)| *v),
        )
    });
    let (chip, chip_color) = match s.status {
        crate::i18n::OutputStatus::Paused => (l.pick("仅预览", "PREVIEW"), SUB),
        crate::i18n::OutputStatus::Retry(_) => (l.pick("重试中", "RETRYING"), color(0xffbe78)),
        crate::i18n::OutputStatus::Waiting => (l.pick("等待中", "WAITING"), SUB),
        _ => (l.pick("已连接", "LIVE"), MINT),
    };
    drop(s);
    fill(dc, &rect(0, 0, WIDTH, HEIGHT), BASE);
    DrawIconEx(
        dc,
        px(24),
        px(18),
        icon_at(px(42)),
        px(42),
        px(42),
        0,
        null_mut(),
        DI_NORMAL,
    );
    txt(dc, "Azoth Scope", rect(77, 10, 255, 38), INK, 1, 0);
    txt(dc, "ROG AZOTH   /   M701", rect(79, 47, 260, 18), SUB, 3, 0);
    fill(dc, &rect(24, 76, 804, 1), EDGE);
    box_(dc, rect(24, 92, 466, 255), 16, PANEL, EDGE);
    txt(dc, "01   /   OLED", rect(47, 106, 205, 19), MINT, 3, 0);
    txt(
        dc,
        l.pick("屏幕预览", "Display preview"),
        rect(47, 127, 275, 29),
        INK,
        2,
        0,
    );
    box_(dc, rect(362, 124, 103, 28), 26, BASE, EDGE);
    let power = battery.map_or_else(|| "—".into(), |v| format!("{v}%"));
    let accent = if battery.is_some_and(|v| v <= 15) {
        color(0xffbe78)
    } else {
        SUB
    };
    box_(dc, rect(275, 125, 77, 26), 10, BASE, EDGE);
    box_(dc, rect(284, 133, 16, 10), 2, BASE, accent);
    fill(dc, &rect(300, 136, 2, 4), accent);
    if let Some(v) = battery {
        fill(
            dc,
            &rect(286, 135, (12 * i32::from(v) + 99) / 100, 6),
            accent,
        );
    }
    txt(dc, &power, rect(304, 126, 42, 24), accent, 3, DT_CENTER);
    box_(dc, rect(374, 135, 7, 7), 7, chip_color, chip_color);
    txt(dc, chip, rect(386, 126, 70, 24), chip_color, 3, DT_CENTER);
    box_(dc, rect(44, 166, 426, 138), 12, GLASS, EDGE);
    let mut bi: BITMAPINFO = zeroed();
    bi.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
    bi.bmiHeader.biWidth = W as i32;
    bi.bmiHeader.biHeight = -(H as i32);
    bi.bmiHeader.biPlanes = 1;
    bi.bmiHeader.biBitCount = 32;
    let mut rgb = vec![0u32; W * H - pix.len()];
    rgb.extend(pix.iter().map(|&v| v as u32 * 0x010101));
    StretchDIBits(
        dc,
        px(49),
        px(171),
        px(416),
        px(128),
        0,
        0,
        W as i32,
        H as i32,
        rgb.as_ptr().cast(),
        &bi,
        DIB_RGB_COLORS,
        SRCCOPY,
    );
    txt(
        dc,
        &format!("-{} {}", timing.minutes, l.pick("分钟", "min")),
        rect(48, 311, 100, 20),
        SUB,
        3,
        0,
    );
    txt(
        dc,
        &format!("{} s  /  0–100%", timing.seconds),
        rect(170, 311, 172, 20),
        SUB,
        3,
        DT_CENTER,
    );
    txt(
        dc,
        l.pick("现在", "NOW"),
        rect(365, 311, 102, 20),
        SUB,
        3,
        DT_RIGHT,
    );
    for (n, (short, name, value)) in metrics.iter().enumerate().take(if dual { 2 } else { 1 }) {
        let x = 24 + n as i32 * 239;
        let width = if dual { 227 } else { 466 };
        let accent = if n == 0 { MINT } else { ICE };
        box_(dc, rect(x, 360, width, 127), 14, PANEL, EDGE);
        fill(dc, &rect(x + 18, 379, 3, 13), accent);
        txt(dc, short, rect(x + 28, 374, width - 45, 23), SUB, 3, 0);
        let v = value.map_or_else(|| "—".to_owned(), |v| format!("{v}%"));
        txt(dc, &v, rect(x + 17, 394, width - 36, 44), INK, 4, 0);
        let rail = width - 36;
        fill(dc, &rect(x + 18, 443, rail, 3), EDGE);
        if let Some(v) = value {
            fill(
                dc,
                &rect(x + 18, 443, rail * i32::from(*v) / 100, 3),
                accent,
            );
        }
        let desc = if short == "CPU" {
            l.pick("处理器总占用率", "Total processor usage")
        } else {
            name.split_once('·')
                .map_or(name.as_str(), |(_, v)| v.trim())
        };
        txt(dc, desc, rect(x + 17, 453, width - 34, 23), SUB, 0, 0);
    }
    box_(dc, rect(506, 92, 322, 432), 16, PANEL, EDGE);
    txt(dc, "02   /   CONFIG", rect(527, 106, 280, 19), MINT, 3, 0);
    txt(
        dc,
        l.pick("显示设置", "Display settings"),
        rect(527, 127, 275, 29),
        INK,
        2,
        0,
    );
    fill(dc, &rect(24, 537, 804, 1), EDGE);
    BitBlt(screen, 0, 0, px(WIDTH), px(HEIGHT), dc, 0, 0, SRCCOPY);
    SelectObject(dc, old);
    DeleteObject(bmp);
    DeleteDC(dc);
    EndPaint(h, &ps);
}

pub unsafe fn static_colors(dc: HDC, ctl: HWND) -> LRESULT {
    let id = GetDlgCtrlID(ctl);
    let bg = if id == STATUS as i32 || id == 201 {
        BASE
    } else {
        PANEL
    };
    SetTextColor(dc, SUB);
    SetBkColor(dc, bg);
    SetDCBrushColor(dc, bg);
    GetStockObject(DC_BRUSH) as LRESULT
}
pub unsafe fn list_colors(dc: HDC) -> LRESULT {
    SetTextColor(dc, INK);
    SetBkColor(dc, FIELD);
    SetDCBrushColor(dc, FIELD);
    GetStockObject(DC_BRUSH) as LRESULT
}

pub unsafe fn decorate(h: HWND, is_combo: bool) {
    SetWindowSubclass(h, Some(face_proc), 1, usize::from(is_combo));
}
unsafe extern "system" fn face_proc(
    h: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    id: usize,
    combo: usize,
) -> LRESULT {
    match msg {
        WM_PAINT | WM_PRINTCLIENT => {
            let mut ps = zeroed();
            let dc = if msg == WM_PAINT {
                BeginPaint(h, &mut ps)
            } else {
                wp as HDC
            };
            let mut r = zeroed();
            GetClientRect(h, &mut r);
            let ww = unpx(r.right);
            let hh = unpx(r.bottom);
            let enabled = IsWindowEnabled(h) != 0;
            let focus = GetFocus() == h;
            let cid = GetDlgCtrlID(h) as u16;
            let bg = if matches!(cid, BOOT | LANGUAGE | TOGGLE) {
                BASE
            } else {
                PANEL
            };
            fill(dc, &r, bg);
            let mut buf = [0u16; 256];
            let len = GetWindowTextW(h, buf.as_mut_ptr(), 256);
            let label = String::from_utf16_lossy(&buf[..len as usize]);
            if combo != 0 {
                box_(
                    dc,
                    r,
                    8,
                    if enabled { FIELD } else { PANEL },
                    if focus { MINT } else { EDGE },
                );
                txt(
                    dc,
                    &label,
                    rect(10, 0, ww - 32, hh),
                    if enabled { INK } else { SUB },
                    0,
                    0,
                );
                stroke(
                    dc,
                    SUB,
                    &[
                        (ww - 18, hh / 2 - 2),
                        (ww - 14, hh / 2 + 2),
                        (ww - 10, hh / 2 - 2),
                    ],
                );
            } else if cid == TOGGLE {
                let pressed = SendMessageW(h, BM_GETSTATE, 0, 0) & BST_PUSHED as isize != 0;
                box_(
                    dc,
                    r,
                    12,
                    if pressed { color(0x4acb9a) } else { MINT },
                    if focus { INK } else { MINT },
                );
                txt(dc, &label, rect(4, 0, ww - 8, hh), BASE, 0, DT_CENTER);
            } else {
                let on = SendMessageW(h, BM_GETCHECK, 0, 0) != 0;
                let y = (hh - 16) / 2;
                let c = if on && enabled { MINT } else { EDGE };
                box_(dc, rect(1, y, 30, 16), 16, c, if focus { INK } else { c });
                box_(
                    dc,
                    rect(if on { 16 } else { 4 }, y + 3, 10, 10),
                    10,
                    if on { BASE } else { SUB },
                    c,
                );
                txt(
                    dc,
                    &label,
                    rect(39, 0, ww - 40, hh),
                    if enabled { INK } else { SUB },
                    0,
                    0,
                );
            }
            if msg == WM_PAINT {
                EndPaint(h, &ps);
            }
            0
        }
        WM_ERASEBKGND => 1,
        WM_NCDESTROY => {
            RemoveWindowSubclass(h, Some(face_proc), id);
            DefSubclassProc(h, msg, wp, lp)
        }
        WM_ENABLE | WM_SETFOCUS | WM_KILLFOCUS | WM_SETTEXT | BM_SETCHECK | CB_SETCURSEL
        | WM_LBUTTONUP | WM_KEYUP => {
            let r = DefSubclassProc(h, msg, wp, lp);
            InvalidateRect(h, null(), 0);
            r
        }
        _ => DefSubclassProc(h, msg, wp, lp),
    }
}
