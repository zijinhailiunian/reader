use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use serde::Serialize;
use tauri::Manager;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use crate::app::window_state::{
    capture, monitor_rects, sanitize, sanitize_geometry, InitialGeometry, WindowState,
    WindowStateStore,
};

/// 本轮关闭请求是否已被前端接管。
///
/// 外壳在拦截关闭请求后启动看门狗: 宽限期内前端若已 hide(关闭到托盘)或
/// destroy(退出), 就在这里置位, 看门狗据此退让; 否则替前端结束窗口, 避免
/// "窗口关不掉"。
///
/// 用显式回执而不是轮询窗口可见性: 隐藏到托盘后用户可能立刻从托盘重新打开
/// 窗口, 轮询会把那次重开误判成"前端没响应"并销毁窗口。
pub static CLOSE_REQUEST_HANDLED: AtomicBool = AtomicBool::new(false);

/// 前端回执: 这一轮关闭请求已经处理完(窗口已隐藏或已销毁)。
#[tauri::command]
pub fn acknowledge_close_request() {
    CLOSE_REQUEST_HANDLED.store(true, Ordering::SeqCst);
}

/// 窗口模式状态, 供前端决定是否使用紧凑布局。
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowMode {
    pub mini_mode: bool,
    pub always_on_top: bool,
}

/// 读取当前状态(短暂持锁即释放)。
fn read_state(app: &tauri::AppHandle) -> Result<WindowState, String> {
    let store = app
        .try_state::<Mutex<WindowStateStore>>()
        .ok_or_else(|| "窗口状态未初始化".to_string())?;
    // `inner()` 返回与 AppHandle 同生命周期的引用, 避免把 MutexGuard 绑在
    // try_state 返回的临时 State 上(那样会借用局部变量)。
    let guard = store
        .inner()
        .lock()
        .map_err(|_| "窗口状态已损坏".to_string())?;
    Ok(guard.load().unwrap_or_default())
}

/// 写回状态(短暂持锁即释放)。
fn write_state(app: &tauri::AppHandle, state: WindowState) -> Result<(), String> {
    let store = app
        .try_state::<Mutex<WindowStateStore>>()
        .ok_or_else(|| "窗口状态未初始化".to_string())?;
    let mut guard = store
        .inner()
        .lock()
        .map_err(|_| "窗口状态已损坏".to_string())?;
    guard.save(state);
    Ok(())
}

/// 查询当前窗口模式(前端启动时同步紧凑布局)。
#[tauri::command]
pub fn get_window_mode(app: tauri::AppHandle) -> Result<WindowMode, String> {
    let mini_mode = read_state(&app)
        .map(|state| state.mini_mode)
        .unwrap_or(false);
    Ok(WindowMode {
        mini_mode,
        always_on_top: mini_mode,
    })
}

/// 切换迷你模式。
///
/// 进入时把当前常规几何先记下来(退出后能回到原位)并自动置顶; 退出时回到常规
/// 几何。两套几何分别保存, 因此来回切换不会丢失各自被调整过的尺寸。
///
/// 注意两点:
/// 1. 全程**不持锁**做窗口操作。`set_size`/`unmaximize` 会触发 `Resized`
///    事件, 其回调要拿同一把锁; 若在这里持锁调用它们就会自锁。
/// 2. 模式标志必须在窗口操作**之前**落盘。否则 `Resized` 回调读到的还是旧的
///    `mini_mode`, 会把新的迷你尺寸错记进常规几何, 退出迷你模式后就回不去了。
#[tauri::command]
pub fn toggle_mini_mode(app: tauri::AppHandle) -> Result<WindowMode, String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "主窗口不存在".to_string())?;

    let mut state = read_state(&app)?;
    let monitors = monitor_rects(&window);
    let entering_mini = !state.mini_mode;
    let min_size = InitialGeometry::min_size_for(entering_mini);

    // 1) 先把当前几何固化到"切换前"的那一套, 并翻转模式标志落盘。
    if let Some(captured) = capture(&window, Some(state)) {
        state = captured;
    }
    state.mini_mode = entering_mini;
    write_state(&app, state)?;

    // 2) 再做窗口操作。此时事件回调看到的 mode 已经是新值, 会写入正确的那一套。
    //    窗口下限必须跟着切换: 迷你模式用更小的下限, 否则 320×240 会被常规
    //    下限(420×320)抬回去, 用户永远缩不到目标尺寸。
    let _ = window.set_min_size(Some(tauri::LogicalSize::new(min_size.0, min_size.1)));
    if entering_mini {
        // 最大化与迷你模式互斥: 先还原, 否则在最大化状态下 set_size 不生效。
        let _ = window.unmaximize();
        let (width, height) = state.mini_size();
        let (x, y) = state.mini_position();
        let (size, position) = sanitize_geometry(width, height, x, y, &monitors, min_size);
        let _ = window.set_size(size);
        match position {
            Some(position) => {
                let _ = window.set_position(position);
            }
            None => {
                let _ = window.center();
            }
        }
        let _ = window.set_always_on_top(true);
    } else {
        let (size, position, maximized) = sanitize(state, &monitors, min_size);
        let _ = window.set_size(size);
        match position {
            Some(position) => {
                let _ = window.set_position(position);
            }
            None => {
                let _ = window.center();
            }
        }
        if maximized {
            let _ = window.maximize();
        }
        let _ = window.set_always_on_top(false);
    }

    // 3) 不在这里回读窗口几何: macOS 上 set_size/set_position 是异步生效的,
    //    此刻读到的仍是切换前的尺寸, 会把大窗尺寸记进迷你几何(反之亦然)。
    //    系统实际应用的尺寸(含最小尺寸约束、显示器裁剪)由随后的 Resized/Moved
    //    事件写入 —— 那边的节流会在节流结束时补存最后一次变化。

    Ok(WindowMode {
        mini_mode: state.mini_mode,
        always_on_top: state.mini_mode,
    })
}

/// 迷你模式的屏幕吸管: 进入系统取色状态(放大镜), 返回所取颜色的 `#rrggbb`;
/// 用户按 Esc 取消时返回 `None`。与 reader_mac 一样用 `NSColorSampler`,
/// 不打开完整的颜色面板。WKWebView 里的 `<input type="color">` 点了没有反应,
/// 所以 macOS 上取色只能走这里; 其它平台前端仍用网页取色控件。
#[tauri::command]
pub async fn sample_screen_color(app: tauri::AppHandle) -> Result<Option<String>, String> {
    #[cfg(target_os = "macos")]
    {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        app.run_on_main_thread(move || color_sampler::begin(sender))
            .map_err(|error| error.to_string())?;
        receiver.await.map_err(|_| "取色已中断".to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        Err("屏幕取色仅支持 macOS".to_string())
    }
}

#[cfg(target_os = "macos")]
mod color_sampler {
    use std::sync::Mutex;

    use block2::RcBlock;
    use objc2_app_kit::{NSColor, NSColorSampler, NSColorSpace};
    use tokio::sync::oneshot::Sender;

    /// 必须在主线程调用。选择回调也由系统在主线程上调用, 且只会调用一次。
    pub fn begin(sender: Sender<Option<String>>) {
        let sender = Mutex::new(Some(sender));
        let handler = RcBlock::new(move |color: *mut NSColor| {
            // SAFETY: 系统传入的是有效的 NSColor 或空指针(用户取消)。
            let hex = unsafe { color.as_ref() }.and_then(to_hex);
            if let Some(sender) = sender.lock().ok().and_then(|mut slot| slot.take()) {
                let _ = sender.send(hex);
            }
        });
        let sampler = NSColorSampler::new();
        // SAFETY: 在主线程调用; sampler 在取色会话结束前由系统持有。
        unsafe { sampler.showSamplerWithSelectionHandler(&handler) };
    }

    /// 转成不透明的 sRGB `#rrggbb`(文字和背景颜色都不带透明度)。
    fn to_hex(color: &NSColor) -> Option<String> {
        let srgb = color.colorUsingColorSpace(&NSColorSpace::sRGBColorSpace())?;
        let channel = |value: f64| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
        Some(format!(
            "#{:02x}{:02x}{:02x}",
            channel(srgb.redComponent()),
            channel(srgb.greenComponent()),
            channel(srgb.blueComponent()),
        ))
    }
}

#[tauri::command]
pub fn configure_boss_key(
    app: tauri::AppHandle,
    shortcut: Option<String>,
) -> Result<(), String> {
    app.global_shortcut()
        .unregister_all()
        .map_err(|error| error.to_string())?;
    let Some(shortcut) = shortcut
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    else {
        return Ok(());
    };
    if shortcut.split('+').any(|part| {
        matches!(
            part.trim().to_ascii_lowercase().as_str(),
            "super" | "meta" | "win"
        )
    }) {
        return Err("老板键不能使用系统键".to_string());
    }
    let parsed = shortcut
        .parse::<tauri_plugin_global_shortcut::Shortcut>()
        .map_err(|_| "快捷键格式无效".to_string())?;
    app.global_shortcut()
        .on_shortcut(parsed, |app, _shortcut, event| {
            if event.state != ShortcutState::Pressed {
                return;
            }
            if let Some(window) = app.get_webview_window("main") {
                if window.is_visible().unwrap_or(false) {
                    let _ = window.hide();
                } else {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        })
        .map_err(|error| format!("快捷键注册失败：{error}"))
}
