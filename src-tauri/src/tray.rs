//! 系统托盘：让「关窗」不等于「杀任务」。
//!
//! 背景（2026-07-20 实测的坑）：媒体流水线跑在 App 进程的后台线程里，它拉起的 claude /
//! python 子进程全部登记在 `runtime::procs::CHILDREN`；而 `lib.rs` 的退出钩子在
//! `RunEvent::ExitRequested` 就调 `CHILDREN.kill_all()`。于是用户点一下窗口右上角的 ✕
//! （流程详情卡是 97vw 满屏的，它自己的 ✕ 就贴在系统 ✕ 下面，极易点岔），两条正在
//! 配图的 job 同一秒一起断掉，日志停在「claude 子进程被中断」。
//!
//! 现在：**有活在跑时，关窗只是把窗口藏起来**，进程继续跑，托盘图标是回到前台和真正
//! 退出的唯一入口。没活在跑时行为不变——关窗就是退出，不留后台幽灵。
//!
//! 「真正退出」只有两条路：托盘菜单的「退出」、以及空闲时直接关窗。两者都会走到
//! `RunEvent::Exit`，子进程照常回收，不会留孤儿。

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, Runtime,
};

/// 「用户确实要退出」闸门。窗口的 CloseRequested 钩子看它决定拦不拦：
/// 托盘点了「退出」→ 置真 → 后续的关窗一路放行到进程退出。
static QUITTING: AtomicBool = AtomicBool::new(false);

/// 「窗口去哪了」提示只在每个进程里弹一次，之后用户已经知道规矩了。
static HINTED: AtomicBool = AtomicBool::new(false);

/// 托盘菜单里点了「退出」——此后不再拦截关窗。
pub fn is_quitting() -> bool {
    QUITTING.load(Ordering::SeqCst)
}

/// 当前是否有「死了就白跑」的后台活计。
/// 目前只认媒体流水线（它最长、最贵、断了要从断点重跑）；以后有别的长任务往这儿加。
pub fn background_busy() -> usize {
    crate::media_engine::live_job_count()
}

/// 把主窗口拉回前台（托盘左键 / 菜单「显示窗口」/ 第二次启动被单实例锁挡回来时）。
pub fn show_main<R: Runtime>(app: &AppHandle<R>) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// 托盘提示文案跟着后台活计走，鼠标悬停就知道「它还在替我干活」。
pub fn refresh_tooltip<R: Runtime>(app: &AppHandle<R>) {
    let n = background_busy();
    let tip = if n > 0 {
        format!("北极星 · GEO —— 后台运行中（{n} 条流程在跑）")
    } else {
        "北极星 · GEO".to_string()
    };
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_tooltip(Some(&tip));
    }
}

/// 窗口刚被收进托盘时说一声。静默消失跟「App 崩了」长得一模一样，用户会去重启，
/// 反而白白折腾。非阻塞弹窗（不能在窗口事件回调里同步等），且一个进程只弹一次。
pub fn notify_backgrounded<R: Runtime>(app: &AppHandle<R>, busy: usize) {
    if HINTED.swap(true, Ordering::SeqCst) {
        return;
    }
    use tauri_plugin_dialog::DialogExt;
    app.dialog()
        .message(format!(
            "还有 {busy} 条流程在跑，已转入后台继续。\n\n\
             窗口收进了系统托盘：点托盘图标可随时叫回来；\n\
             要真正退出，请用托盘右键菜单里的「退出」。"
        ))
        .title("北极星 · GEO 转入后台")
        .show(|_| {});
}

/// 建托盘图标 + 菜单。图标复用窗口图标（bundle 里那套），不额外塞资源。
pub fn init<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "显示窗口", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出（会中断在跑的流程）", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;

    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("北极星 · GEO")
        .menu(&menu)
        // 左键单击直接回前台，别逼用户去翻右键菜单。
        .show_menu_on_left_click(false)
        .on_menu_event(|app, ev| match ev.id.as_ref() {
            "show" => show_main(app),
            "quit" => {
                QUITTING.store(true, Ordering::SeqCst);
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, ev| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = ev
            {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }
    builder.build(app)?;
    Ok(())
}
