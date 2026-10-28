use std::{
    mem::size_of,
    ptr,
    sync::{Mutex, OnceLock},
    time::Instant,
};

use chrono::{Local, TimeZone};
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, HWND, LPARAM, LRESULT, RECT, TRUE,
        WPARAM,
    },
    Graphics::{
        Dwm::{DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute},
        Gdi::{
            BeginPaint, CreateFontW, CreateSolidBrush, DEFAULT_CHARSET, DT_END_ELLIPSIS, DT_LEFT,
            DT_SINGLELINE, DT_VCENTER, DeleteObject, DrawTextW, EndPaint, FillRect, InvalidateRect,
            OUT_DEFAULT_PRECIS, PAINTSTRUCT, SelectObject, SetBkMode, SetTextColor, TRANSPARENT,
        },
    },
    System::{
        LibraryLoader::GetModuleHandleW,
        Threading::{
            CreateMutexW, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
            QueryFullProcessImageNameW,
        },
    },
    UI::{
        HiDpi::{
            DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForWindow,
            SetProcessDpiAwarenessContext,
        },
        WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DispatchMessageW, EnumWindows, GWL_EXSTYLE, GWL_STYLE,
            GetClientRect, GetForegroundWindow, GetMessageW, GetWindowLongPtrW, GetWindowRect,
            GetWindowThreadProcessId, HWND_TOPMOST, IsIconic, IsWindowVisible, LWA_ALPHA, MSG,
            PostMessageW, PostQuitMessage, RegisterClassW, SW_HIDE, SWP_NOACTIVATE, SWP_SHOWWINDOW,
            SetLayeredWindowAttributes, SetTimer, SetWindowPos, ShowWindow, TranslateMessage,
            WM_APP, WM_DESTROY, WM_ERASEBKGND, WM_PAINT, WM_TIMER, WNDCLASSW, WS_EX_LAYERED,
            WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
        },
    },
};

use crate::quota::{self, QuotaWindow, Quotas};

const CLASS_NAME: &str = "CodexWindowMeter.Overlay";
const TRACK_TIMER: usize = 1;
const UPDATE_MESSAGE: u32 = WM_APP + 1;

struct UiState {
    overlay: isize,
    quotas: Option<Quotas>,
    last_update: Option<Instant>,
}

static STATE: OnceLock<Mutex<UiState>> = OnceLock::new();

struct InstanceMutex(HANDLE);

impl InstanceMutex {
    fn acquire() -> Result<Option<Self>, String> {
        let name = wide("Local\\CodexWindowMeter.Instance");
        let handle = unsafe { CreateMutexW(ptr::null(), 1, name.as_ptr()) };
        if handle.is_null() {
            return Err(format!("CreateMutexW: {}", std::io::Error::last_os_error()));
        }
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            unsafe { CloseHandle(handle) };
            return Ok(None);
        }
        Ok(Some(Self(handle)))
    }
}

impl Drop for InstanceMutex {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

pub fn diagnose() -> String {
    let mut windows = Vec::<String>::new();
    unsafe {
        EnumWindows(
            Some(diagnose_window),
            (&mut windows as *mut Vec<String>) as LPARAM,
        )
    };
    let target = find_codex_window();
    format!(
        "target={target:?}, foreground={:?}, foreground_match={}\n{}",
        unsafe { GetForegroundWindow() },
        is_codex_foreground(target),
        windows.join("\n")
    )
}

unsafe extern "system" fn diagnose_window(window: HWND, result: LPARAM) -> i32 {
    if let Some(path) = process_path(window)
        && (path.contains("codex") || path.contains("chatgpt"))
    {
        let style = unsafe { GetWindowLongPtrW(window, GWL_STYLE) as u32 };
        let extended_style = unsafe { GetWindowLongPtrW(window, GWL_EXSTYLE) as u32 };
        let visible = unsafe { IsWindowVisible(window) };
        unsafe { &mut *(result as *mut Vec<String>) }.push(format!(
            "window={window:?} visible={visible} style={style:#x} ex={extended_style:#x} path={path}"
        ));
    }
    TRUE
}

pub fn run() -> Result<(), String> {
    let Some(_instance) = InstanceMutex::acquire()? else {
        return Ok(());
    };
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    let class_name = wide(CLASS_NAME);
    let instance = unsafe { GetModuleHandleW(ptr::null()) };
    let mut window_class: WNDCLASSW = unsafe { std::mem::zeroed() };
    window_class.lpfnWndProc = Some(window_proc);
    window_class.hInstance = instance;
    window_class.lpszClassName = class_name.as_ptr();
    if unsafe { RegisterClassW(&window_class) } == 0 {
        return Err(format!(
            "RegisterClassW: {}",
            std::io::Error::last_os_error()
        ));
    }
    let overlay = unsafe {
        CreateWindowExW(
            WS_EX_LAYERED | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_TRANSPARENT,
            class_name.as_ptr(),
            wide("Codex Window Meter").as_ptr(),
            WS_POPUP,
            0,
            0,
            1,
            1,
            ptr::null_mut(),
            ptr::null_mut(),
            instance,
            ptr::null(),
        )
    };
    if overlay.is_null() {
        return Err(format!(
            "CreateWindowExW: {}",
            std::io::Error::last_os_error()
        ));
    }
    STATE
        .set(Mutex::new(UiState {
            overlay: overlay as isize,
            quotas: None,
            last_update: None,
        }))
        .map_err(|_| "state initialized twice".to_string())?;
    unsafe {
        SetLayeredWindowAttributes(overlay, 0, 255, LWA_ALPHA);
        if SetTimer(overlay, TRACK_TIMER, 250, None) == 0 {
            return Err(format!("SetTimer: {}", std::io::Error::last_os_error()));
        }
    }
    quota::start(move |update| {
        if let Some(state) = STATE.get()
            && let Ok(mut state) = state.lock()
        {
            if let Some(quotas) = update {
                state.quotas = Some(quotas);
                state.last_update = Some(Instant::now());
            }
            unsafe { PostMessageW(state.overlay as HWND, UPDATE_MESSAGE, 0, 0) };
        }
    });
    track(overlay);
    let mut message: MSG = unsafe { std::mem::zeroed() };
    loop {
        let result = unsafe { GetMessageW(&mut message, ptr::null_mut(), 0, 0) };
        if result == -1 {
            return Err(format!("GetMessageW: {}", std::io::Error::last_os_error()));
        }
        if result == 0 {
            return Ok(());
        }
        unsafe {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match message {
            WM_TIMER if wparam == TRACK_TIMER => {
                track(window);
                0
            }
            UPDATE_MESSAGE => {
                InvalidateRect(window, ptr::null(), 0);
                0
            }
            WM_PAINT => {
                paint(window);
                0
            }
            WM_ERASEBKGND => 1,
            WM_DESTROY => {
                PostQuitMessage(0);
                0
            }
            _ => DefWindowProcW(window, message, wparam, lparam),
        }
    }
}

fn track(overlay: HWND) {
    let target = find_codex_window();
    if target.is_null() || unsafe { IsIconic(target) } != 0 || !is_codex_foreground(target) {
        unsafe { ShowWindow(overlay, SW_HIDE) };
        return;
    }
    let mut bounds: RECT = unsafe { std::mem::zeroed() };
    let frame_result = unsafe {
        DwmGetWindowAttribute(
            target,
            DWMWA_EXTENDED_FRAME_BOUNDS as u32,
            (&mut bounds as *mut RECT).cast(),
            size_of::<RECT>() as u32,
        )
    };
    if frame_result != 0 && unsafe { GetWindowRect(target, &mut bounds) } == 0 {
        unsafe { ShowWindow(overlay, SW_HIDE) };
        return;
    }
    let scale = unsafe { GetDpiForWindow(target) }.max(96) as f32 / 96.0;
    let has_two_windows = STATE
        .get()
        .and_then(|state| state.lock().ok())
        .and_then(|state| {
            state
                .quotas
                .as_ref()
                .map(|quotas| quotas.short.is_some() && quotas.weekly.is_some())
        })
        .unwrap_or(false);
    let Some((relative_left, width)) = layout(bounds.right - bounds.left, scale, has_two_windows)
    else {
        unsafe { ShowWindow(overlay, SW_HIDE) };
        return;
    };
    let height = (30.0 * scale).round() as i32;
    let top = bounds.top + (5.0 * scale).round() as i32;
    unsafe {
        SetWindowPos(
            overlay,
            HWND_TOPMOST,
            bounds.left + relative_left,
            top,
            width,
            height,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
    }
}

fn layout(total_width: i32, scale: f32, two_windows: bool) -> Option<(i32, i32)> {
    let left_reserve = (330.0 * scale).round() as i32;
    let right_reserve = (155.0 * scale).round() as i32;
    let preferred_width = ((if two_windows { 460.0 } else { 260.0 }) * scale).round() as i32;
    let available_width = total_width - left_reserve - right_reserve;
    if available_width < (170.0 * scale).round() as i32 {
        return None;
    }
    let width = preferred_width.min(available_width);
    Some((total_width - right_reserve - width, width))
}

fn find_codex_window() -> HWND {
    let mut result: HWND = ptr::null_mut();
    unsafe { EnumWindows(Some(visit_window), (&mut result as *mut HWND) as LPARAM) };
    result
}

unsafe extern "system" fn visit_window(window: HWND, result: LPARAM) -> i32 {
    unsafe {
        if IsWindowVisible(window) == 0 || IsIconic(window) != 0 {
            return TRUE;
        }
        let extended_style = GetWindowLongPtrW(window, GWL_EXSTYLE) as u32;
        if extended_style & WS_EX_TOOLWINDOW != 0 {
            return TRUE;
        }
        let mut cloaked = 0u32;
        if DwmGetWindowAttribute(
            window,
            DWMWA_CLOAKED as u32,
            (&mut cloaked as *mut u32).cast(),
            size_of::<u32>() as u32,
        ) == 0
            && cloaked != 0
        {
            return TRUE;
        }
        if process_path(window).as_deref().is_some_and(|path| {
            path.contains("\\openai.codex_") && path.ends_with("\\app\\chatgpt.exe")
        }) {
            *(result as *mut HWND) = window;
            return 0;
        }
        TRUE
    }
}

fn process_path(window: HWND) -> Option<String> {
    let mut process_id = 0;
    unsafe { GetWindowThreadProcessId(window, &mut process_id) };
    if process_id == 0 {
        return None;
    }
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id) };
    if process.is_null() {
        return None;
    }
    let mut buffer = [0u16; 1024];
    let mut length = buffer.len() as u32;
    let success =
        unsafe { QueryFullProcessImageNameW(process, 0, buffer.as_mut_ptr(), &mut length) };
    unsafe { CloseHandle(process) };
    (success != 0)
        .then(|| String::from_utf16_lossy(&buffer[..length as usize]).to_ascii_lowercase())
}

fn is_codex_foreground(target: HWND) -> bool {
    let foreground = unsafe { GetForegroundWindow() };
    if foreground == target {
        return true;
    }
    let mut target_process = 0;
    let mut foreground_process = 0;
    unsafe {
        GetWindowThreadProcessId(target, &mut target_process);
        GetWindowThreadProcessId(foreground, &mut foreground_process);
    }
    target_process != 0 && target_process == foreground_process
}

fn paint(window: HWND) {
    let mut paint: PAINTSTRUCT = unsafe { std::mem::zeroed() };
    let context = unsafe { BeginPaint(window, &mut paint) };
    if context.is_null() {
        return;
    }
    let mut client: RECT = unsafe { std::mem::zeroed() };
    unsafe { GetClientRect(window, &mut client) };
    fill(context, &client, rgb(29, 29, 29));
    let scale = unsafe { GetDpiForWindow(window) }.max(96) as f32 / 96.0;
    let font = unsafe {
        CreateFontW(
            -(13.0 * scale).round() as i32,
            0,
            0,
            0,
            400,
            0,
            0,
            0,
            DEFAULT_CHARSET.into(),
            OUT_DEFAULT_PRECIS.into(),
            0,
            0,
            0,
            wide("Microsoft YaHei UI").as_ptr(),
        )
    };
    let previous_font = if font.is_null() {
        ptr::null_mut()
    } else {
        unsafe { SelectObject(context, font) }
    };
    let (quotas, stale) = STATE
        .get()
        .and_then(|state| state.lock().ok())
        .map(|state| {
            (
                state.quotas.clone(),
                state
                    .last_update
                    .is_none_or(|updated| updated.elapsed().as_secs() > 120),
            )
        })
        .unwrap_or((None, true));
    match quotas {
        Some(quotas) => {
            let windows: Vec<(&str, QuotaWindow)> = [
                quotas.short.map(|value| ("5小时", value)),
                quotas.weekly.map(|value| ("1周", value)),
            ]
            .into_iter()
            .flatten()
            .collect();
            let panel_width = client.right / windows.len().max(1) as i32;
            for (index, (label, quota)) in windows.iter().enumerate() {
                draw_panel(
                    context,
                    index as i32 * panel_width,
                    panel_width,
                    client.bottom,
                    scale,
                    label,
                    quota,
                    stale,
                );
            }
        }
        None => {
            draw_text(
                context,
                "正在读取 Codex 额度…",
                RECT {
                    left: 12,
                    top: 0,
                    right: client.right - 8,
                    bottom: client.bottom,
                },
                rgb(220, 220, 220),
            );
        }
    }
    if !font.is_null() {
        unsafe {
            SelectObject(context, previous_font);
            DeleteObject(font);
        }
    }
    unsafe { EndPaint(window, &paint) };
}

fn draw_panel(
    context: windows_sys::Win32::Graphics::Gdi::HDC,
    left: i32,
    width: i32,
    height: i32,
    scale: f32,
    label: &str,
    quota: &QuotaWindow,
    stale: bool,
) {
    let reset = quota
        .reset_at
        .and_then(|timestamp| Local.timestamp_opt(timestamp, 0).single())
        .map(|value| value.format("%m/%d %H:%M").to_string())
        .unwrap_or_else(|| "--".to_string());
    let title = if stale {
        format!("{label} {}% · 正在重连", quota.remaining)
    } else {
        format!("{label} {}% · {reset}", quota.remaining)
    };
    let padding = (10.0 * scale).round() as i32;
    draw_text(
        context,
        &title,
        RECT {
            left: left + padding,
            top: 0,
            right: left + width - padding,
            bottom: height - 8,
        },
        if stale {
            rgb(160, 160, 160)
        } else {
            rgb(235, 235, 235)
        },
    );
    let bar = RECT {
        left: left + padding,
        top: height - 6,
        right: left + width - padding,
        bottom: height - 3,
    };
    fill(context, &bar, rgb(75, 75, 75));
    let filled_width =
        ((bar.right - bar.left) as f32 * quota.remaining as f32 / 100.0).round() as i32;
    let filled = RECT {
        right: bar.left + filled_width,
        ..bar
    };
    let color = if stale {
        rgb(130, 130, 130)
    } else if quota.remaining <= 10 {
        rgb(230, 76, 76)
    } else if quota.remaining <= 20 {
        rgb(235, 180, 55)
    } else {
        rgb(126, 100, 240)
    };
    fill(context, &filled, color);
}

fn draw_text(
    context: windows_sys::Win32::Graphics::Gdi::HDC,
    text: &str,
    mut bounds: RECT,
    color: u32,
) {
    let text = wide(text);
    unsafe {
        SetBkMode(context, TRANSPARENT as i32);
        SetTextColor(context, color);
        DrawTextW(
            context,
            text.as_ptr(),
            -1,
            &mut bounds,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
        );
    }
}

fn fill(context: windows_sys::Win32::Graphics::Gdi::HDC, bounds: &RECT, color: u32) {
    let brush = unsafe { CreateSolidBrush(color) };
    unsafe {
        FillRect(context, bounds, brush);
        DeleteObject(brush);
    }
}

fn rgb(red: u8, green: u8, blue: u8) -> u32 {
    red as u32 | (green as u32) << 8 | (blue as u32) << 16
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maximized_frame_fits_overlay() {
        assert_eq!(layout(1280, 1.0, true), Some((665, 460)));
    }

    #[test]
    fn narrow_window_hides_overlay() {
        assert_eq!(layout(500, 1.0, true), None);
    }
}
