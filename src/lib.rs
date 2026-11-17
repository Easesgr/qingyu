//! 轻屿：音乐、置物架、粘贴板、拾光、邮箱、天气、Agent 与 Mac 状态。
//!
//! - `app` / `pages`：与平台无关的界面与动效（外壳与录制器共用）
//! - `anim` / `paint` / `shape` / `theme`：弹簧、矢量绘制、面板轮廓、配色字体
//! - `model` / `lyrics` / `smtc` / `icons`：数据、歌词、系统媒体、文件图标
//! - `native` / `tray`：仅 Windows 的窗口区域、命中穿透与托盘

pub mod display_layout;
pub mod island_position;
#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "macos")]
pub mod mac_services;
#[cfg(target_os = "macos")]
pub mod mac_design;
#[cfg(target_os = "macos")]
mod mac_settings;
#[cfg(target_os = "macos")]
mod mac_media;
mod mail_privacy;
pub mod poll_clock;
pub mod resources;
pub mod anim;
pub mod oled_shift;
pub mod app;
pub mod agent_monitor;
pub mod agent_open;
mod agent_activity;
mod agent_icons;
mod agent_tasks;
mod agent_ui;
pub mod library;
mod library_ui;
pub mod mac_stats;
mod mac_stats_ui;
pub mod child_process;
mod settings_window;
pub mod glass;
pub mod skin_paint;
pub mod icons;
pub mod lyrics;
pub mod mail;
pub mod mail_format;
mod mail_reader;
pub mod mail_manage;
mod mail_manage_ui;
pub mod clipboard;
pub mod side_dock;
mod portrait;
mod control_center;
mod side_motion;
pub mod frame_pacing;
mod task_picker;
pub mod model;
pub mod notifications;
mod notification_ui;
mod island_notice;
mod island_companion;
mod companion_settings;
pub mod renderer;
pub mod otp;
pub mod weather;
#[cfg(target_os = "macos")]
mod network;
mod weather_ui;
mod pages;
mod extras;
mod bar_layout;
mod resident_settings;
mod avoidance;
#[cfg(windows)]
pub mod backdrop;
#[cfg(windows)]
pub mod composition;
#[cfg(windows)]
pub mod system;
pub mod paint;
pub mod shape;
mod fluid;
pub mod smtc;
pub mod music_volume;
pub mod qq_music;
mod music_library;
mod music_playback;
pub mod theme;
pub mod settings_theme;

// Native, read-only documentation capture; excluded from distributed builds.
#[cfg(all(feature = "record", debug_assertions))]
pub mod showcase;

#[cfg(windows)]
pub mod native;
#[cfg(windows)]
pub mod tray;

#[cfg(test)]
mod ui_tests;
#[cfg(test)]
#[path = "bin/qingyu-record/raster.rs"]
mod preview_raster;
