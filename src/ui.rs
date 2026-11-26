mod skin;
use crate::{
    app::{self, Shared},
    frame::{GraphStyle, Surface, H, MAX_MINUTES, MAX_SECONDS, W},
    i18n::Language,
};
use std::{
    cell::{Cell, OnceCell, RefCell},
    collections::HashMap,
    mem::{size_of, zeroed},
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::{LibraryLoader::GetModuleHandleW, Registry::*, Threading::CreateMutexW},
    UI::{HiDpi::*, Shell::*, WindowsAndMessaging::*},
};

const MODE: u16 = 101;
const FIRST: u16 = 102;
const SECOND: u16 = 103;
const TOGGLE: u16 = 104;
const OPEN: u16 = 105;
const QUIT: u16 = 106;
const STATUS: u16 = 107;
const BOOT: u16 = 108;
const CHANNEL: u16 = 109;
const STYLE: u16 = 110;
const NAMES: u16 = 111;
const MINUTES: u16 = 112;
const SECONDS: u16 = 113;
const LANGUAGE: u16 = 114;
const RELABEL: u32 = WM_APP + 2;
const TRAY: u32 = WM_APP + 1;
thread_local! {static STATE:OnceCell<Shared>=const{OnceCell::new()};static TASKBAR:Cell<u32>=const{Cell::new(0)};static HAS_TRAY:Cell<bool>=const{Cell::new(false)};}
fn state() -> Shared {
    STATE.with(|s| s.get().unwrap().clone())
}
thread_local! {static DPI:Cell<u32>=const{Cell::new(96)};static ICONS:RefCell<HashMap<i32,HICON>>=RefCell::new(HashMap::new());}
fn px(n: i32) -> i32 {
    (n * DPI.with(Cell::get) as i32 + 48) / 96
}
fn unpx(n: i32) -> i32 {
    let dpi = DPI.with(Cell::get) as i32;
    (n * 96 + dpi / 2) / dpi
}
fn language() -> Language {
    state().0.lock().unwrap().pref.language
}
fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
unsafe fn message(h: HWND, s: &str) {
    MessageBoxW(
        h,
        w(s).as_ptr(),
        w("Azoth Scope").as_ptr(),
        MB_OK | MB_ICONINFORMATION,
    );
}
unsafe fn autorun(on: Option<bool>) -> bool {
    let mut key = null_mut();
    let path = w(r"Software\Microsoft\Windows\CurrentVersion\Run");
    let name = w("AzothScope");
    if RegOpenKeyExW(
        HKEY_CURRENT_USER,
        path.as_ptr(),
        0,
        if on.is_some() {
            KEY_SET_VALUE
        } else {
            KEY_QUERY_VALUE
        },
        &mut key,
    ) != 0
    {
        return false;
    }
    let result = match on {
        None => {
            RegQueryValueExW(
                key,
                name.as_ptr(),
                null(),
                null_mut(),
                null_mut(),
                null_mut(),
            ) == 0
        }
        Some(false) => RegDeleteValueW(key, name.as_ptr()) == 0,
        Some(true) => std::env::current_exe().ok().is_some_and(|p| {
            let cmd = w(&format!("\"{}\" --tray", p.display()));
            RegSetValueExW(
                key,
                name.as_ptr(),
                0,
                REG_SZ,
                cmd.as_ptr().cast(),
                (cmd.len() * 2) as u32,
            ) == 0
        }),
    };
    RegCloseKey(key);
    result
}
unsafe fn child(
    h: HWND,
    class: &str,
    title: &str,
    id: u16,
    style: u32,
    pos: (i32, i32, i32, i32),
) -> HWND {
    let (x, y, width, height) = pos;
    let ctl = CreateWindowExW(
        0,
        w(class).as_ptr(),
        w(title).as_ptr(),
        WS_CHILD | WS_VISIBLE | style,
        px(x),
        px(y),
        px(x + width) - px(x),
        px(y + height) - px(y),
        h,
        id as usize as HMENU,
        GetModuleHandleW(null()),
        null(),
    );
    SendMessageW(ctl, WM_SETFONT, skin::body_font() as usize, 0);
    if class == "BUTTON" || class == "COMBOBOX" {
        skin::decorate(ctl, class == "COMBOBOX");
    }
    ctl
}
unsafe fn combo(h: HWND, id: u16, pos: (i32, i32, i32, i32), items: &[String]) {
    let ctl = child(
        h,
        "COMBOBOX",
        "",
        id,
        CBS_DROPDOWNLIST as u32 | WS_TABSTOP | WS_VSCROLL,
        pos,
    );
    for name in items {
        SendMessageW(ctl, CB_ADDSTRING, 0, w(name).as_ptr() as isize);
    }
    SendMessageW(ctl, CB_SETITEMHEIGHT, usize::MAX, px(24) as isize);
    if matches!(id, STYLE | MODE) {
        SendMessageW(ctl, CB_SETDROPPEDWIDTH, px(208) as usize, 0);
    }
}
unsafe fn controls(h: HWND) {
    let l = language();
    SetWindowTextW(
        h,
        w(l.pick(
            "Azoth Scope · 夜魔 OLED（Bar / Line）",
            "Azoth Scope · ROG Azoth OLED (Bar / Line)",
        ))
        .as_ptr(),
    );
    child(h, "STATIC", "Language", 201, 0, (631, 30, 76, 22));
    combo(
        h,
        LANGUAGE,
        (709, 24, 119, 100),
        &["简体中文".into(), "English".into()],
    );
    child(
        h,
        "STATIC",
        l.pick("历史时长", "History"),
        0,
        0,
        (527, 395, 120, 20),
    );
    combo(
        h,
        MINUTES,
        (527, 416, 130, 240),
        &(1..=MAX_MINUTES)
            .map(|v| format!("{v} {}", l.pick("分钟", "min")))
            .collect::<Vec<_>>(),
    );
    child(
        h,
        "STATIC",
        l.pick("刷新间隔", "Refresh"),
        0,
        0,
        (671, 395, 136, 20),
    );
    combo(
        h,
        SECONDS,
        (671, 416, 136, 240),
        &(1..=MAX_SECONDS)
            .map(|v| format!("{v} {}", l.pick("秒", "s")))
            .collect::<Vec<_>>(),
    );
    for (name, x, y) in [
        (l.pick("布局", "Layout"), 527, 163),
        (l.pick("数据源 1", "Source 1"), 527, 221),
        (l.pick("数据源 2", "Source 2"), 527, 279),
        (l.pick("写屏方式", "Output channel"), 527, 337),
        (l.pick("图形", "Style"), 671, 163),
    ] {
        child(h, "STATIC", name, 0, 0, (x, y, 134, 20));
    }
    combo(
        h,
        MODE,
        (527, 184, 130, 120),
        &[
            l.pick("单图", "Single graph").into(),
            l.pick("双图", "Two graphs").into(),
        ],
    );
    let names = state()
        .0
        .lock()
        .unwrap()
        .sources
        .iter()
        .map(|s| {
            if s.id == "cpu" {
                l.pick(&s.name, "CPU · Total usage").to_owned()
            } else {
                s.name.clone()
            }
        })
        .collect::<Vec<_>>();
    combo(h, FIRST, (527, 242, 280, 220), &names);
    combo(h, SECOND, (527, 300, 280, 220), &names);
    combo(
        h,
        CHANNEL,
        (527, 358, 280, 140),
        &[
            l.pick("整图 · 208×64（原版）", "Full image · 208×64 (original)")
                .into(),
            l.pick(
                "横幅 · 208×46（闪屏对照测试）",
                "Banner · 208×46 (flicker test)",
            )
            .into(),
            l.pick(
                "原生趋势（双图左右排列）",
                "Native trends (two graphs side by side)",
            )
            .into(),
        ],
    );
    combo(
        h,
        STYLE,
        (671, 184, 136, 140),
        &[
            l.pick("Bar · 柱状图", "Bar chart").into(),
            l.pick("分离折线", "Split lines").into(),
            l.pick("连续折线", "Continuous line").into(),
        ],
    );
    child(
        h,
        "BUTTON",
        l.pick("屏上显示名称", "Show labels on OLED"),
        NAMES,
        BS_AUTOCHECKBOX as u32 | WS_TABSTOP,
        (527, 471, 280, 30),
    );
    child(
        h,
        "BUTTON",
        l.pick("开始写屏", "Start OLED output"),
        TOGGLE,
        WS_TABSTOP,
        (645, 548, 183, 36),
    );
    let boot = child(
        h,
        "BUTTON",
        l.pick("登录时启动到托盘", "Start in tray at sign-in"),
        BOOT,
        BS_AUTOCHECKBOX as u32 | WS_TABSTOP,
        (25, 495, 460, 28),
    );
    SendMessageW(boot, BM_SETCHECK, autorun(None) as usize, 0);
    child(h, "STATIC", "", STATUS, 0, (26, 552, 597, 38));
    sync(h);
    status(h);
}
unsafe fn sync(h: HWND) {
    let a = state();
    let s = a.0.lock().unwrap();
    let (top, bottom) = s.indices();
    let (one, enabled, ng) = (s.pref.one, s.pref.enabled, s.sources.len());
    let ch = match s.pref.surface {
        Surface::Image => 0,
        Surface::Banner => 1,
        Surface::NativeBars => 2,
    };
    let graph = if ch != 2 {
        1
    } else {
        match s.pref.graph_style {
            GraphStyle::Bar => 0,
            GraphStyle::Line => 1,
            GraphStyle::JoinedLine => 2,
        }
    };
    let names = !s.pref.hide_names;
    let timing = s.pref.timing;
    let l = s.pref.language;
    drop(s);
    for (id, index) in [
        (MODE, (!one) as usize),
        (FIRST, top),
        (SECOND, bottom),
        (CHANNEL, ch),
        (STYLE, graph),
        (MINUTES, usize::from(timing.minutes - 1)),
        (SECONDS, usize::from(timing.seconds - 1)),
        (LANGUAGE, usize::from(l == Language::English)),
    ] {
        SendMessageW(GetDlgItem(h, id as i32), CB_SETCURSEL, index, 0);
    }
    EnableWindow(GetDlgItem(h, MODE as i32), (ng > 1) as i32);
    EnableWindow(GetDlgItem(h, SECOND as i32), (!one) as i32);
    EnableWindow(GetDlgItem(h, CHANNEL as i32), (!enabled) as i32);
    EnableWindow(GetDlgItem(h, STYLE as i32), (ch == 2) as i32);
    EnableWindow(GetDlgItem(h, NAMES as i32), (ch == 2) as i32);
    SendMessageW(GetDlgItem(h, NAMES as i32), BM_SETCHECK, names as usize, 0);
    SetWindowTextW(
        GetDlgItem(h, TOGGLE as i32),
        w(if enabled {
            l.pick("暂停写屏", "Pause OLED output")
        } else {
            l.pick("开始写屏", "Start OLED output")
        })
        .as_ptr(),
    );
    InvalidateRect(h, &preview_rect(), 0);
}
fn preview_rect() -> RECT {
    RECT {
        left: px(24),
        top: px(92),
        right: px(491),
        bottom: px(488),
    }
}
unsafe fn status(h: HWND) {
    let a = state();
    let s = a.0.lock().unwrap();
    let l = s.pref.language;
    let level = s.battery.map_or_else(|| "—".into(), |v| format!("{v}%"));
    let text = format!(
        "{}   ·   {} {level}",
        s.status.text(l),
        l.pick("键盘电量", "Keyboard battery")
    );
    drop(s);
    SetWindowTextW(GetDlgItem(h, STATUS as i32), w(&text).as_ptr());
}
unsafe fn icon_at(size: i32) -> HICON {
    ICONS.with(|cache| {
        *cache.borrow_mut().entry(size).or_insert_with(|| {
            LoadImageW(
                GetModuleHandleW(null()),
                std::ptr::without_provenance(1), // MAKEINTRESOURCEW(1), not a string pointer
                IMAGE_ICON,
                size,
                size,
                0,
            ) as HICON
        })
    })
}
unsafe fn window_icons(h: HWND) {
    for (kind, metric) in [(ICON_SMALL, SM_CXSMICON), (ICON_BIG, SM_CXICON)] {
        SendMessageW(
            h,
            WM_SETICON,
            kind as usize,
            icon_at(GetSystemMetricsForDpi(metric, DPI.with(Cell::get))) as isize,
        );
    }
}
unsafe fn tray(h: HWND, action: u32) -> bool {
    let mut d: NOTIFYICONDATAW = zeroed();
    d.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
    d.hWnd = h;
    d.uID = 1;
    d.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
    d.uCallbackMessage = TRAY;
    let shell = FindWindowW(w("Shell_TrayWnd").as_ptr(), null());
    let dpi = GetDpiForWindow(shell);
    d.hIcon = icon_at(GetSystemMetricsForDpi(
        SM_CXSMICON,
        if dpi == 0 { GetDpiForSystem() } else { dpi },
    ));
    let t = w("Azoth Scope · ROG Azoth OLED");
    d.szTip[..t.len()].copy_from_slice(&t);
    Shell_NotifyIconW(action, &d) != 0
}
unsafe fn show(h: HWND) {
    ShowWindow(h, SW_RESTORE);
    SetForegroundWindow(h);
}
unsafe fn menu(h: HWND) {
    let p = CreatePopupMenu();
    let a = state();
    let enabled = a.0.lock().unwrap().pref.enabled;
    let l = language();
    for (id, text) in [
        (OPEN, l.pick("打开设置", "Open settings")),
        (
            TOGGLE,
            if enabled {
                l.pick("暂停写屏", "Pause OLED output")
            } else {
                l.pick("继续写屏", "Resume OLED output")
            },
        ),
        (QUIT, l.pick("退出", "Exit")),
    ] {
        AppendMenuW(p, MF_STRING, id as usize, w(text).as_ptr());
    }
    let mut pt = zeroed();
    GetCursorPos(&mut pt);
    SetForegroundWindow(h);
    let id = TrackPopupMenu(p, TPM_RETURNCMD | TPM_RIGHTBUTTON, pt.x, pt.y, 0, h, null());
    DestroyMenu(p);
    if id != 0 {
        PostMessageW(h, WM_COMMAND, id as usize, 0);
    }
    PostMessageW(h, WM_NULL, 0, 0);
}
unsafe fn change(h: HWND, id: u16) {
    let a = state();
    let val = |i: u16| SendMessageW(GetDlgItem(h, i as i32), CB_GETCURSEL, 0, 0) as usize;
    let needs_prompt = {
        let s = a.0.lock().unwrap();
        id == TOGGLE && !s.pref.accepted
    };
    let l = language();
    if needs_prompt && MessageBoxW(h,w(l.pick("开始后将按所选间隔更新 OLED。整图模式会覆盖键盘自定义图片槽，请先保留原素材。暂停后可从 Armoury Crate 重新应用原画面。现在开始？","Start updating the keyboard OLED? Full image mode overwrites the custom image slot. Keep a copy of your original. Pause output before restoring it in Armoury Crate.")).as_ptr(),w(l.pick("启用键盘 OLED 输出","Enable OLED output")).as_ptr(),MB_YESNO|MB_ICONQUESTION|MB_DEFBUTTON2)!=IDYES{return}
    let mut s = a.0.lock().unwrap();
    let (first, second) = s.indices();
    match id {
        MODE => s.pref.one = val(MODE) == 0,
        LANGUAGE => {
            s.pref.language = match val(LANGUAGE) {
                0 => Language::Chinese,
                1 => Language::English,
                _ => return,
            };
        }
        MINUTES | SECONDS => {
            let i = val(id);
            let max = if id == MINUTES {
                MAX_MINUTES
            } else {
                MAX_SECONDS
            };
            if i >= usize::from(max) {
                return;
            }
            if id == MINUTES {
                s.pref.timing.minutes = i as u8 + 1;
            } else {
                s.pref.timing.seconds = i as u8 + 1;
            }
        }
        NAMES if s.pref.surface == Surface::NativeBars => {
            s.pref.hide_names = SendMessageW(GetDlgItem(h, NAMES as i32), BM_GETCHECK, 0, 0) == 0;
        }
        STYLE if s.pref.surface == Surface::NativeBars => {
            s.pref.graph_style = match val(STYLE) {
                0 => GraphStyle::Bar,
                1 => GraphStyle::Line,
                2 => GraphStyle::JoinedLine,
                _ => return,
            }
        }
        CHANNEL if !s.pref.enabled => {
            s.pref.surface = match val(CHANNEL) {
                1 => Surface::Banner,
                2 => Surface::NativeBars,
                _ => Surface::Image,
            };
        }
        FIRST | SECOND => {
            let i = val(id);
            if i >= s.sources.len() {
                return;
            }
            // Preserve two distinct sources. Selecting the other row's source swaps the rows.
            if i != first && i != second {
                s.history[i].clear();
            }
            if id == FIRST {
                if i == second {
                    s.pref.second = s.sources[first].id.clone();
                }
                s.pref.first = s.sources[i].id.clone();
            } else {
                if i == first {
                    s.pref.first = s.sources[second].id.clone();
                }
                s.pref.second = s.sources[i].id.clone();
            }
        }
        TOGGLE => {
            s.pref.accepted = true;
            s.pref.enabled = !s.pref.enabled;
        }
        _ => return,
    }
    if id != LANGUAGE {
        s.draw();
    }
    let pref = s.pref.clone();
    drop(s);
    if id != LANGUAGE {
        a.1.notify_all();
    }
    if let Err(e) = pref.save() {
        message(
            h,
            &format!(
                "{}{}",
                pref.language
                    .pick("设置未保存：", "Settings could not be saved: "),
                pref.language.detail(&e)
            ),
        );
    }
    if id == LANGUAGE {
        PostMessageW(h, RELABEL, 0, 0);
    } else {
        sync(h);
    }
}
unsafe extern "system" fn proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_COMMAND => {
            let id = wp as u16;
            match id {
                MODE | FIRST | SECOND | CHANNEL | STYLE | MINUTES | SECONDS | LANGUAGE
                    if wp >> 16 == CBN_SELCHANGE as usize =>
                {
                    change(h, id)
                }
                TOGGLE => change(h, id),
                NAMES => change(h, id),
                BOOT => {
                    let on = SendMessageW(GetDlgItem(h, BOOT as i32), BM_GETCHECK, 0, 0) != 0;
                    if !autorun(Some(on)) {
                        message(
                            h,
                            language().pick(
                                "无法更新当前用户的登录启动设置。",
                                "Could not change the sign-in setting for this user.",
                            ),
                        );
                        SendMessageW(
                            GetDlgItem(h, BOOT as i32),
                            BM_SETCHECK,
                            autorun(None) as usize,
                            0,
                        );
                    }
                }
                OPEN => show(h),
                QUIT => {
                    DestroyWindow(h);
                }
                _ => {}
            }
            0
        }
        RELABEL => {
            // Run after the selection notification has returned. Recreate only
            // child controls, never the window, sampler or device session.
            loop {
                let child = GetWindow(h, GW_CHILD);
                if child.is_null() {
                    break;
                }
                DestroyWindow(child);
            }
            controls(h);
            InvalidateRect(h, null(), 0);
            windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus(GetDlgItem(
                h,
                LANGUAGE as i32,
            ));
            0
        }
        WM_PAINT => {
            skin::dashboard(h);
            0
        }
        WM_ERASEBKGND => 1,
        WM_CTLCOLORSTATIC | WM_CTLCOLORBTN => skin::static_colors(wp as HDC, lp as HWND),
        WM_CTLCOLORLISTBOX => skin::list_colors(wp as HDC),
        WM_TIMER => {
            if IsWindowVisible(h) != 0 {
                status(h);
                InvalidateRect(h, &preview_rect(), 0);
            }
            0
        }
        WM_CLOSE => {
            if HAS_TRAY.with(Cell::get) {
                ShowWindow(h, SW_HIDE);
            } else {
                DestroyWindow(h);
            }
            0
        }
        WM_DESTROY => {
            tray(h, NIM_DELETE);
            PostQuitMessage(0);
            0
        }
        TRAY => {
            match lp as u32 {
                WM_LBUTTONDBLCLK => show(h),
                WM_RBUTTONUP | WM_CONTEXTMENU => menu(h),
                _ => {}
            }
            0
        }
        _ if msg != 0 && msg == TASKBAR.with(Cell::get) => {
            let ok = tray(h, NIM_ADD);
            HAS_TRAY.with(|x| x.set(ok));
            if !ok {
                show(h);
            }
            0
        }
        WM_DPICHANGED => {
            DPI.with(|v| v.set((wp as u32) & 0xffff));
            let r = &*(lp as *const RECT);
            SetWindowPos(
                h,
                null_mut(),
                r.left,
                r.top,
                r.right - r.left,
                r.bottom - r.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            window_icons(h);
            if HAS_TRAY.with(Cell::get) {
                tray(h, NIM_MODIFY);
            }
            if !GetDlgItem(h, MODE as i32).is_null() {
                PostMessageW(h, RELABEL, 0, 0);
            }
            0
        }
        WM_DISPLAYCHANGE | WM_SETTINGCHANGE => {
            if HAS_TRAY.with(Cell::get) {
                tray(h, NIM_MODIFY);
            }
            DefWindowProcW(h, msg, wp, lp)
        }
        _ => DefWindowProcW(h, msg, wp, lp),
    }
}
pub unsafe fn run() {
    SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    DPI.with(|v| v.set(GetDpiForSystem().max(96)));
    let class = w("AzothScopeSettings");
    let guard = CreateMutexW(null(), 0, w(r"Local\AzothScope.M701.Settings").as_ptr());
    if !guard.is_null() && GetLastError() == ERROR_ALREADY_EXISTS {
        let h = FindWindowW(class.as_ptr(), null());
        if !h.is_null() {
            show(h);
        }
        CloseHandle(guard);
        return;
    }
    let (shared, workers) = app::start();
    STATE.with(|s| {
        let _ = s.set(shared.clone());
    });
    let instance = GetModuleHandleW(null());
    let wc = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(proc),
        hInstance: instance,
        hCursor: LoadCursorW(null_mut(), IDC_ARROW),
        hIcon: icon_at(px(32)),
        hbrBackground: (COLOR_BTNFACE + 1) as HBRUSH,
        lpszClassName: class.as_ptr(),
        ..zeroed()
    };
    RegisterClassW(&wc);
    TASKBAR.with(|t| t.set(RegisterWindowMessageW(w("TaskbarCreated").as_ptr())));
    let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_CLIPCHILDREN;
    let mut r = RECT {
        left: 0,
        top: 0,
        right: px(skin::WIDTH),
        bottom: px(skin::HEIGHT),
    };
    AdjustWindowRectExForDpi(&mut r, style, 0, 0, DPI.with(Cell::get));
    let h = CreateWindowExW(
        0,
        class.as_ptr(),
        w("Azoth Scope").as_ptr(),
        style,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        r.right - r.left,
        r.bottom - r.top,
        null_mut(),
        null_mut(),
        instance,
        null(),
    );
    if h.is_null() {
        message(
            null_mut(),
            language().pick(
                "无法创建设置窗口。",
                "Could not create the settings window.",
            ),
        );
    } else {
        let dark = 1i32;
        windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute(
            h,
            20,
            (&dark as *const i32).cast(),
            4,
        );
        let dpi = GetDpiForWindow(h);
        if dpi > 0 && dpi != DPI.with(Cell::get) {
            DPI.with(|v| v.set(dpi));
            let mut size = RECT {
                left: 0,
                top: 0,
                right: px(skin::WIDTH),
                bottom: px(skin::HEIGHT),
            };
            AdjustWindowRectExForDpi(&mut size, style, 0, 0, dpi);
            SetWindowPos(
                h,
                null_mut(),
                0,
                0,
                size.right - size.left,
                size.bottom - size.top,
                SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
        window_icons(h);
        controls(h);
        let ok = tray(h, NIM_ADD);
        HAS_TRAY.with(|x| x.set(ok));
        SetTimer(h, 1, 1000, None);
        if !ok || !std::env::args().any(|s| s == "--tray") {
            show(h);
        }
        let mut msg = zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            if IsDialogMessageW(h, &msg) == 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
    shared.0.lock().unwrap().stop = true;
    shared.1.notify_all();
    for t in workers {
        let _ = t.join();
    }
    if !guard.is_null() {
        CloseHandle(guard);
    }
    UnregisterClassW(class.as_ptr(), instance);
    ICONS.with(|cache| {
        for (_, icon) in cache.borrow_mut().drain() {
            if !icon.is_null() {
                DestroyIcon(icon);
            }
        }
    });
}
use windows_sys::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
