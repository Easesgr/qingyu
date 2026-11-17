//! The hover destination: live summaries and small controls, with details one click away.
//! This is a navigation layer, never a tenth persisted module.
use crate::{
    agent_monitor::Status as AgentStatus,
    app::{App, Cmd, Env, AGENT_PAGE, CLIPBOARD_PAGE, MAC_STATS_PAGE, MAIL_PAGE,
        MUSIC_PAGE, NOTIFICATIONS_PAGE, PAGE_GLYPHS, PAGE_NAMES, SHELF_PAGE, TODO_PAGE, WEATHER_PAGE},
    paint::{self, Card, H},
    theme::{self, W},
};
use eframe::egui::{self, Color32};

// Shared by the overview and every detail page. Fits the existing native top
// canvas, including the shoulder and lower shadow.
pub(crate) const TOP_H: f32 = if cfg!(target_os = "macos") { 286.0 } else { 252.0 };
const SURFACE: Color32 = Color32::from_rgb(23, 43, 48);
const HOVER: Color32 = Color32::from_rgb(32, 65, 71);

type Tile = (usize, usize, usize, usize);

fn overview_tiles(count: usize) -> (usize, usize, Vec<Tile>) {
    if count == 0 { return (1, 1, Vec::new()); }
    if count == 1 { return (1, 1, vec![(0, 0, 1, 1)]); }
    if count == 2 { return (3, 2, vec![(0, 0, 2, 2), (2, 0, 1, 2)]); }

    let mut tiles = vec![(0, 0, 2, 2)];
    match count {
        3 => tiles.extend([(2, 0, 1, 1), (2, 1, 1, 1)]),
        4 => tiles.extend([(2, 0, 1, 2), (0, 2, 1, 1), (1, 2, 2, 1)]),
        5 => tiles.extend([(2, 0, 1, 1), (2, 1, 1, 1), (0, 2, 1, 1), (1, 2, 2, 1)]),
        6 => tiles.extend([(2, 0, 1, 1), (2, 1, 1, 1), (0, 2, 1, 1), (1, 2, 1, 1), (2, 2, 1, 1)]),
        7 => {
            tiles.extend([(2, 0, 2, 1), (2, 1, 2, 1)]);
            tiles.extend((0..4).map(|col| (col, 2, 1, 1)));
        }
        8 => {
            tiles.extend([(2, 0, 2, 1), (2, 1, 1, 1), (3, 1, 1, 1)]);
            tiles.extend((0..4).map(|col| (col, 2, 1, 1)));
        }
        _ => {
            tiles.extend([(2, 0, 1, 1), (3, 0, 1, 1), (2, 1, 1, 1), (3, 1, 1, 1)]);
            tiles.extend((0..4).map(|col| (col, 2, 1, 1)));
        }
    }
    let (cols, rows) = if count >= 7 { (4, 3) } else if count == 3 { (3, 2) } else { (3, 3) };
    tiles.truncate(count);
    (cols, rows, tiles)
}

/// Reserve reading areas for music and Agent, even between tracks and tasks.
/// Featured modules and remaining modules each retain their saved order.
/// Return rectangles in input order so painting, hit regions and navigation
/// continue to use the same page identities.
pub(crate) fn overview_rects(pages: &[usize], side: bool, size: egui::Vec2) -> Vec<egui::Rect> {
    if pages.is_empty() { return Vec::new(); }
    let agent = pages.iter().position(|page| *page == AGENT_PAGE);
    let music = pages.iter().position(|page| *page == MUSIC_PAGE);
    let mut featured: Vec<_> = if agent.is_some() && music.is_some() { vec![agent.unwrap(), music.unwrap()] }
        else { vec![agent.unwrap_or(0)] };
    featured.sort_unstable();
    let slots: Vec<_> = if side {
        let cols = columns(true, size.x);
        let featured_rows: Vec<_> = featured.iter().map(|index| if pages[*index] == AGENT_PAGE { 2 } else { 1 }).collect();
        let first_regular_row: usize = featured_rows.iter().sum();
        let rows = first_regular_row + pages.len().saturating_sub(featured.len()).div_ceil(cols);
        let cell_w = size.x / cols as f32;
        let cell_h = size.y / rows as f32;
        (0..pages.len()).map(|index| {
            if index < featured.len() { return egui::Rect::from_min_size(egui::pos2(0.0, cell_h * featured_rows[..index].iter().sum::<usize>() as f32), egui::vec2(size.x, cell_h * featured_rows[index] as f32)); }
            let col = (index - featured.len()) % cols;
            let row = first_regular_row + (index - featured.len()) / cols;
            let span = if index == pages.len() - 1 && col == 0 { cols } else { 1 };
            egui::Rect::from_min_size(egui::pos2(col as f32 * cell_w, row as f32 * cell_h), egui::vec2(cell_w * span as f32, cell_h))
        }).collect()
    } else {
        let (cols, rows, tiles) = if featured.len() == 2 {
            let mut tiles = vec![(0, 0, 2, 2), (2, 0, 2, 2)];
            let regular = pages.len() - 2;
            for index in 0..regular {
                let col = index % 4;
                let span = if index == regular - 1 { 4 - col } else { 1 };
                tiles.push((col, 2 + index / 4, span, 1));
            }
            (4, 2 + regular.div_ceil(4), tiles)
        } else { overview_tiles(pages.len()) };
        let column_weights = if featured.len() == 2 { vec![1.0; cols] }
            else if cols == 4 { vec![1.10, 1.05, 0.925, 0.925] }
            else if cols == 3 { vec![1.08, 1.0, 0.92] } else { vec![1.0; cols] };
        let row_weights = if rows == 4 { vec![1.10, 1.10, 0.90, 0.90] }
            else if rows == 3 { vec![1.12, 0.94, 0.94] }
            else if rows == 2 { vec![1.05, 0.95] } else { vec![1.0; rows] };
        let x = track_edges(&column_weights, size.x);
        let y = track_edges(&row_weights, size.y);
        tiles.into_iter().map(|(col, row, span_w, span_h)| egui::Rect::from_min_max(
            egui::pos2(x[col], y[row]), egui::pos2(x[col + span_w], y[row + span_h]))).collect()
    };
    let mut remaining = slots.iter().skip(featured.len());
    pages.iter().enumerate().map(|(index, _)| if let Some(slot) = featured.iter().position(|featured| *featured == index) { slots[slot] }
        else { *remaining.next().expect("one tile per enabled module") }).collect()
}

fn separators(c: &Card, rects: &[egui::Rect], light: bool) {
    let ink = if light { Color32::from_black_alpha(28) } else { Color32::from_white_alpha(28) };
    for (index, a) in rects.iter().enumerate() {
        for b in rects.iter().skip(index + 1) {
            if (a.right() - b.left()).abs() < 0.05 || (b.right() - a.left()).abs() < 0.05 {
                let x = if (a.right() - b.left()).abs() < 0.05 { a.right() } else { a.left() };
                let top = a.top().max(b.top());
                let bottom = a.bottom().min(b.bottom());
                if bottom - top > 8.0 { c.seg((x, top + 4.0), (x, bottom - 4.0), 0.7, ink); }
            }
            if (a.bottom() - b.top()).abs() < 0.05 || (b.bottom() - a.top()).abs() < 0.05 {
                let y = if (a.bottom() - b.top()).abs() < 0.05 { a.bottom() } else { a.top() };
                let left = a.left().max(b.left());
                let right = a.right().min(b.right());
                if right - left > 8.0 { c.seg((left + 4.0, y), (right - 4.0, y), 0.7, ink); }
            }
        }
    }
}

fn track_edges(weights: &[f32], length: f32) -> Vec<f32> {
    let total: f32 = weights.iter().sum();
    let mut edge = 0.0;
    let mut edges = Vec::with_capacity(weights.len() + 1);
    edges.push(edge);
    for weight in weights {
        edge += length * *weight / total;
        edges.push(edge);
    }
    edges
}

#[derive(Debug)]
pub(crate) struct Summary {
    pub page: usize,
    pub value: String,
    pub detail: String,
    pub extra: String,
    pub more: Vec<String>,
    pub active: bool,
    pub agent: Option<AgentSummary>,
}

#[derive(Debug)]
pub(crate) struct AgentSummary {
    pub working: usize,
    pub waiting: usize,
    pub approval: usize,
    pub tasks: Vec<AgentTask>,
}

#[derive(Debug)]
pub(crate) struct AgentTask {
    pub title: String,
    pub status: AgentStatus,
}

fn line(text: &str) -> String {
    crate::notification_ui::safe_preview(text).split_whitespace().collect::<Vec<_>>().join(" ")
}

fn clock(seconds: f64) -> String {
    let seconds = seconds.max(0.0) as u64;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

/// The overview always fits its enabled modules in one viewport. Two columns
/// retain readable cards in the narrow side panel; the top panel uses three.
pub(crate) fn columns(side: bool, _width: f32) -> usize {
    if side { 2 } else { 3 }
}

fn button(c: &Card, x: f32, y: f32, size: f32, glyph: &str, title: &str,
          id: impl std::hash::Hash, enabled: bool) -> bool {
    let response = if enabled { c.clone() } else { c.ghost() }.hit(x, y, size, size, id)
        .on_hover_text(title).on_hover_cursor(egui::CursorIcon::PointingHand);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, c.live && enabled, title));
    let (hover, press) = c.hover_t(&response);
    if hover > 0.001 || press > 0.001 || response.has_focus() {
        c.fill(x, y, size, size, 7.0, paint::mix(SURFACE, HOVER, hover.max(press)));
    }
    if response.has_focus() { c.stroke(x, y, size, size, 7.0, 1.4, theme::CYAN); }
    paint::glyph(c, glyph, x + size / 2.0, y + size / 2.0, 14.0,
        if enabled { theme::WHITE } else { theme::DIM });
    response.clicked()
}

impl App {
    /// One safe, bounded snapshot per enabled module, in the user's saved order.
    pub(crate) fn control_summaries(&self, env: &Env) -> Vec<Summary> {
        self.settings.page_order.iter().copied().filter(|page| self.module_on(*page))
            .map(|page| self.control_summary(page, env)).collect()
    }

    fn control_summary(&self, page: usize, env: &Env) -> Summary {
        let previews = self.settings.notification_preview && !self.notification_locked;
        let mut summary = Summary { page, value: String::new(), detail: String::new(), extra: String::new(), more: Vec::new(), active: false, agent: None };
        match page {
            MUSIC_PAGE => {
                let display=self.music_display();
                let has_music = !display.title.is_empty();
                let playing = has_music && self.music_playing();
                summary.value = if has_music && previews { line(&display.title) }
                    else if has_music { if playing { "正在播放" } else { "已暂停" }.into() }
                    else { "暂无音乐".into() };
                summary.detail = if has_music && previews {
                    if display.artist.is_empty() { if playing { "正在播放" } else { "已暂停" }.into() }
                    else { line(&display.artist) }
                } else if has_music { "打开音乐查看歌曲".into() } else { "播放音乐后即可控制".into() };
                if has_music { summary.extra = format!("{} / {}", clock(display.position),
                    if display.duration > 0.0 { clock(display.duration) } else { "—".into() }); }
                if let Some((volume, muted)) = self.music_volume_ui {
                    summary.more.push(if muted { "音乐已静音".into() } else { format!("音乐音量 {volume}%") });
                }
                summary.active = playing;
            }
            SHELF_PAGE => {
                summary.value = format!("{} 个文件", self.shelf.items.len());
                summary.detail = if self.shelf.items.is_empty() { "拖入文件，随时取用".into() }
                    else if previews { self.shelf.items.last().and_then(|path| path.file_name())
                        .map(|name| line(&name.to_string_lossy())).unwrap_or_else(|| "打开置物架查看".into()) }
                    else { "打开置物架查看文件".into() };
                summary.extra = "只记位置，保留原文件".into();
                if previews { summary.more = self.shelf.items.iter().rev().skip(1).take(2)
                    .filter_map(|path| path.file_name()).map(|name| line(&name.to_string_lossy())).collect(); }
            }
            TODO_PAGE => {
                let entries: Vec<_> = self.library.entries.iter().filter(|entry| !entry.state.archived).collect();
                let pending = entries.iter().filter(|entry| if entry.kind==crate::library::Kind::Action { !entry.state.done } else { !entry.state.reviewed }).count();
                let reminders = self.todos.items.iter().filter(|item| item.scheduled_on(&env.today) && !item.done_for(&env.today)).count();
                summary.value = format!("{} 条留存", entries.len());
                summary.detail = if let Some(entry) = entries.first() {
                    if previews { line(&entry.title) } else { "打开拾光回顾内容".into() }
                } else { "把想法和好链接交给 Hermes".into() };
                summary.extra = if reminders>0 { format!("{pending} 条待回顾 · {reminders} 条提醒") } else { format!("{pending} 条待回顾 · 想法 / 收藏 / 行动") };
                summary.active = pending>0 || reminders>0;
                if previews { summary.more = entries.iter().skip(1).take(2).map(|entry| line(&entry.title)).collect(); }
            }
            MAIL_PAGE => {
                let accounts: Vec<_> = self.mail.accounts.iter().filter(|account| account.enabled).collect();
                let rows: Vec<_> = self.mail.recent.iter().filter(|item| !self.mail_locally_removed(item)
                    && accounts.iter().any(|account| account.email.eq_ignore_ascii_case(&item.account))).collect();
                let states: Vec<_> = accounts.iter().map(|account| self.mail.status_of(&account.email)).collect();
                let failed = states.iter().any(|status| matches!(status, crate::mail::Status::Auth(_) | crate::mail::Status::Failed(_)));
                let connecting = states.iter().any(|status| matches!(status, crate::mail::Status::Connecting));
                let known = !rows.is_empty() && rows.iter().all(|item| self.mail_seen(item, crate::mail_manage::Folder::Inbox).is_some());
                let unread = rows.iter().filter(|item| self.mail_seen(item, crate::mail_manage::Folder::Inbox) == Some(false)).count();
                summary.value = if accounts.is_empty() { "未添加邮箱".into() }
                    else if !self.settings.mail_enabled { "收信已暂停".into() }
                    else if failed { "连接需要处理".into() }
                    else if connecting { "正在连接…".into() }
                    else if known { format!("{unread} 封未读") }
                    else if states.iter().any(|status| status.ok()) { "已连接".into() }
                    else { "等待连接".into() };
                summary.detail = if previews { rows.first().map(|item| {
                    let subject = item.code.as_ref().map(|code| crate::mail_privacy::mask(&item.subject, code))
                        .unwrap_or_else(|| item.subject.clone()); line(&subject)
                }).unwrap_or_else(|| "打开邮箱查看连接与邮件".into()) }
                else { "打开邮箱查看邮件".into() };
                summary.extra = if known { "最近邮件中的未读数".into() }
                    else { format!("{} 个已启用账号 · 未读数待更新", accounts.len()) };
                summary.active = !failed && !connecting && unread > 0;
                if previews { summary.more = rows.iter().skip(1).take(2).map(|item| {
                    line(&item.code.as_ref().map(|code| crate::mail_privacy::mask(&item.subject, code)).unwrap_or_else(|| item.subject.clone()))
                }).collect(); }
            }
            WEATHER_PAGE => {
                if let Some(weather) = &self.weather.snapshot {
                    summary.value = format!("{:.0}° · {}", weather.temp, crate::weather::describe(weather.code));
                    summary.detail = if self.weather.error.is_some() { format!("{} · 更新失败", weather.place) }
                        else if self.weather.loading { format!("{} · 更新中", weather.place) }
                        else { weather.place.clone() };
                    summary.extra = if let (Some(low), Some(high)) = (weather.low, weather.high) {
                        format!("今天 {low:.0}° — {high:.0}°")
                    } else { "打开天气查看预报".into() };
                    summary.more = weather.days.iter().take(2).map(|day| format!("{} · {:.0}° — {:.0}°", day.label, day.low, day.high)).collect();
                } else {
                    summary.value = if self.weather.loading { "正在更新…" } else if self.weather.error.is_some() { "天气暂不可用" } else { "等待天气数据" }.into();
                    summary.detail = if self.weather.error.is_some() { "打开天气重试或选择城市" } else { "打开天气选择城市" }.into();
                }
            }
            AGENT_PAGE => {
                // A process, a loaded source or Unknown never counts as activity.
                let sessions: Vec<_> = self.agent_tasks.sessions().iter().collect();
                let working = sessions.iter().filter(|session| matches!(session.status, AgentStatus::Thinking | AgentStatus::Working)).count();
                let waiting = sessions.iter().filter(|session| session.status == AgentStatus::Waiting).count();
                let approval = sessions.iter().filter(|session| session.status == AgentStatus::Approval).count();
                let current = sessions.iter().find(|session| session.status == AgentStatus::Approval)
                    .or_else(|| sessions.iter().find(|session| session.status == AgentStatus::Waiting))
                    .or_else(|| sessions.iter().find(|session| matches!(session.status, AgentStatus::Thinking | AgentStatus::Working)))
                    .copied();
                summary.value = if approval > 0 { format!("{working} 执行 · {approval} 需审批") }
                    else if working > 0 || waiting > 0 { format!("{working} 执行 · {waiting} 等待") }
                    else if self.agent_snapshot.scanning { "正在检查状态…".into() }
                    else if self.agent_snapshot.error.is_some() { "状态暂不可用".into() }
                    else { "暂无执行中任务".into() };
                summary.detail = if let Some(session) = current {
                    if previews { line(&crate::agent_activity::title(session)) }
                    else { crate::agent_activity::conversation_label(&session.id) }
                } else if self.agent_snapshot.error.is_some() { "打开 Agent 查看详情".into() }
                else { "开始新任务后会自动显示".into() };
                summary.extra = current.map(|session| format!("{} · {}", session.agent.label(), session.status.label()))
                    .unwrap_or_else(|| "仅显示当前任务".into());
                summary.active = working + waiting + approval > 0;
                summary.more = sessions.iter().filter(|session| session.status.is_active()
                    && current.is_none_or(|current| session.id != current.id)).take(2).map(|session| {
                    let title = if previews { line(&crate::agent_activity::title(session)) }
                        else { crate::agent_activity::conversation_label(&session.id) };
                    format!("{title} · {}", session.status.label())
                }).collect();
                summary.agent = Some(AgentSummary { working, waiting, approval,
                    tasks: sessions.iter().filter(|session| !session.process_only && session.status.is_active()).take(4).map(|session| AgentTask {
                        title: if previews { line(&crate::agent_activity::title(session)) }
                            else { crate::agent_activity::conversation_label(&session.id) },
                        status: session.status,
                    }).collect() });
            }
            CLIPBOARD_PAGE => {
                let history = self.clipboard.lock();
                summary.value = format!("{} 条记录", history.items.len());
                // The dashboard never copies text, paths or media filenames out
                // of clipboard history, even when notification previews are on.
                summary.detail = history.items.first().map(|item| format!("最近 {} · {}", item.at, item.description()))
                    .unwrap_or_else(|| if self.clipboard_active() { "复制内容后自动记录" } else { "历史记录已暂停" }.into());
                summary.extra = format!("{} 条已固定 · 打开查看内容", history.items.iter().filter(|item| item.pinned).count());
                summary.more = history.items.iter().skip(1).take(2).map(|item| format!("{} · {}", item.at, item.description())).collect();
            }
            MAC_STATS_PAGE => {
                let stats = &self.mac_snapshot;
                summary.value = if !stats.supported { "此平台暂不支持".into() }
                    else if stats.paused { "状态采样已暂停".into() }
                    else if let Some(cpu) = stats.cpu_usage { format!("CPU {cpu:.0}%") }
                    else if stats.error.is_some() { "状态暂不可用".into() } else { "正在采样…".into() };
                summary.detail = if stats.memory_valid && !stats.paused && stats.memory_total > 0 {
                    format!("内存 {:.0}% · {:.1} GB", stats.memory_used as f64 / stats.memory_total as f64 * 100.0,
                        stats.memory_used as f64 / 1_073_741_824.0)
                } else { "内存用量待更新".into() };
                summary.extra = if stats.disk_valid && !stats.paused {
                    format!("磁盘可用 {:.0} GB", stats.disk_available as f64 / 1_073_741_824.0)
                } else { "打开查看 CPU、内存与网络".into() };
                if !stats.paused {
                    if let Some(network) = &stats.network {
                        if let (Some(received), Some(sent)) = (network.received_bytes_per_sec, network.sent_bytes_per_sec) {
                            summary.more.push(format!("网络 ↓ {:.0} KB/s · ↑ {:.0} KB/s", received / 1024.0, sent / 1024.0));
                        }
                    }
                    if stats.memory_valid { summary.more.push(format!("压缩内存 {:.1} GB", stats.memory_compressed as f64 / 1_073_741_824.0)); }
                }
            }
            NOTIFICATIONS_PAGE => {
                summary.value = if !self.settings.notifications { "提醒已关闭".into() }
                    else if self.display_notice().is_some() || !self.alerts.is_empty() {
                        format!("{} 条待处理", usize::from(self.display_notice().is_some()) + self.alerts.len())
                    }
                    else if self.history.is_empty() { "暂无新提醒".into() }
                    else { format!("{} 条提醒记录", self.history.len()) };
                summary.detail = if previews { self.display_notice().or(self.history.front()).map(|alert| line(&alert.title))
                    .unwrap_or_else(|| "消息与任务提醒汇总在这里".into()) }
                    else { "打开通知查看提醒".into() };
                summary.extra = if self.alerts.is_empty() { self.notification_status.label() }
                    else { format!("另有 {} 条提醒排队", self.alerts.len()) };
                summary.active = self.display_notice().is_some() || !self.alerts.is_empty();
                if previews { summary.more = self.history.iter().skip(1).take(2).map(|alert| line(&alert.title)).collect(); }
            }
            _ => {}
        }
        summary
    }

    /// Decorative copies share the resident identity without extra hit IDs.
    pub(crate) fn control_center_symbol(&self,c:&Card,x:f32,y:f32,size:f32,animate:bool,_ink:Color32){
        let moving=animate&&!self.reduced_motion&&c.live;
        crate::island_companion::pet(&c.ghost(),self.settings.resident_pet_kind(),x,y,size,if moving{self.t}else{0.0},!moving);
        if moving{c.ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));}
    }

    pub(crate) fn control_center_tab(&mut self,c:&Card,x:f32,y:f32,width:f32,height:f32){
        let response=c.hit(x,y,width,height,"control-center-tab").on_hover_text("主控 · ⌘[").on_hover_cursor(egui::CursorIcon::PointingHand);
        response.widget_info(||egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel,c.live,self.overview,"主控"));
        let (hover,press)=c.hover_t(&response);
        if self.overview||hover>0.001||press>0.001{c.fill(x,y,width,height,8.0,if self.overview{Color32::from_rgb(33,75,81)}else{paint::mix(SURFACE,HOVER,hover.max(press))});}
        if response.has_focus(){c.stroke(x,y,width,height,8.0,1.4,theme::CYAN);response.scroll_to_me(Some(egui::Align::Center));}
        self.control_center_symbol(c,x+width/2.0,y+height/2.0,20.0,self.overview||response.hovered(),if self.overview{theme::CYAN}else{theme::GRAY});
        if self.overview&&self.settings.skin==crate::model::Skin::Fluid{c.fill(x+width/2.0-5.0,y+height-3.0,10.0,2.0,1.0,theme::CYAN);}
        if self.navigation_activated(c,&response,self.overview){self.cancel_edit();self.enter_control_center();}
    }

    /// Content only: both docks use their existing shared header and tab row.
    pub(crate) fn control_center(&mut self,c:&Card,env:&Env,cmds:&mut Vec<Cmd>){
        let side=self.side();let bounds=self.content_rect();
        let summaries=self.control_summaries(env);
        if summaries.is_empty(){
            c.centered_text(bounds.center().x,bounds.center().y,"没有启用的模块",14.0,W::Semibold,theme::WHITE,H::Center);
            return;
        }
        let grid=c.clipped(bounds.left(),bounds.top(),bounds.width(),bounds.height());
        let pages:Vec<_>=summaries.iter().map(|summary|summary.page).collect();
        let rects=overview_rects(&pages,side,bounds.size());
        let light=matches!(self.settings.skin,crate::model::Skin::Cream|crate::model::Skin::GlassLight);
        separators(&grid.shifted(bounds.left(),bounds.top()),&rects,light);
        for (summary,rect) in summaries.iter().zip(rects){
            self.control_card(&grid.shifted(bounds.left()+rect.left(),bounds.top()+rect.top()),summary,rect.width(),rect.height(),cmds);
        }
    }

    fn control_card(&mut self,c:&Card,summary:&Summary,width:f32,height:f32,cmds:&mut Vec<Cmd>){
        let page=summary.page;let name=PAGE_NAMES[page];
        let response=c.hit(0.0,0.0,width,height,("control-module",page)).on_hover_text(format!("{name}\n{}\n{}\n{}",summary.value,summary.detail,summary.extra)).on_hover_cursor(egui::CursorIcon::PointingHand);
        response.widget_info(||egui::WidgetInfo::labeled(egui::WidgetType::Button,c.live,format!("打开{name}，{}，{}",summary.value,summary.detail)));
        let (hover,press)=c.hover_t(&response);
        let feedback=(hover*24.0+press*18.0).clamp(0.0,42.0) as u8;
        if feedback>0{c.fill(0.0,0.0,width,height,8.0,Color32::from_rgba_unmultiplied(HOVER.r(),HOVER.g(),HOVER.b(),feedback));}
        if response.has_focus(){c.stroke(0.7,0.7,width-1.4,height-1.4,10.0,1.4,theme::CYAN);}
        let spacious=height>=95.0;let wide=width>=170.0;let compact=height<50.0;
        let number=if spacious{27.0}else{20.0};let middle=if spacious{height*0.54}else{height*0.65};
        match page{
            MUSIC_PAGE=>{
                let has=self.has_live_music();let art=48.0;
                let playable=self.can_play_music();
                let artwork=wide&&spacious;
                if artwork{
                    if let Some((image,texture))=self.music_artwork.as_ref().filter(|_|self.settings.notification_preview&&!self.notification_locked){
                        let [w,h]=image.size;let uv=egui::Rect::from_center_size(egui::pos2(0.5,0.5),if w>h{egui::vec2(h as f32/w as f32,1.0)}else{egui::vec2(1.0,w as f32/h as f32)});
                        crate::skin_paint::mark_colored(texture.id());
                        c.painter.add(egui::epaint::RectShape::filled(c.rect(9.0,10.0,art,art),6.0*c.s,Color32::WHITE).with_texture(texture.id(),uv));
                    }else{c.fill(9.0,10.0,art,art,6.0,Color32::from_rgb(36,68,75));paint::glyph(c,"music",9.0+art/2.0,10.0+art/2.0,art*0.56,theme::CYAN);}
                }
                let text=if playable{summary.value.as_str()}else{"暂无播放"};
                let title_x=if artwork{art+19.0}else{9.0};let title_y=if artwork{25.0}else{14.0};
                let title_width=width-title_x-9.0;
                c.centered_text(title_x,title_y,&c.fit(text,if wide{13.0}else{11.0},W::Semibold,title_width),if wide{13.0}else{11.0},W::Semibold,theme::WHITE,H::Left);
                if height>=90.0{c.centered_text(title_x,title_y+20.0,&c.fit(&summary.detail,11.0,W::Regular,title_width),11.0,W::Regular,theme::GRAY,H::Left);}
                // All transports share one baseline, including the compact
                // music tile beside Agent and the narrow side-panel cards.
                let gap=if width>=120.0{6.0}else{2.0};
                let control_x=(width-78.0-gap*2.0)/2.0;
                let control_y=height-30.0;
                if button(c,control_x,control_y,26.0,"previous","上一首","control-music-previous",has){cmds.push(Cmd::Media(crate::smtc::Transport::Prev));}
                let(glyph,label)=if playable{self.music_play_button()}else{("music","打开音乐播放")};
                if button(c,control_x+26.0+gap,control_y,26.0,glyph,label,"control-music-play",true){
                    if playable{self.toggle_music_playback(cmds);}else{self.go(MUSIC_PAGE,1.0);}
                }
                if button(c,control_x+52.0+gap*2.0,control_y,26.0,"next","下一首","control-music-next",has){cmds.push(Cmd::Media(crate::smtc::Transport::Next));}
                if has&&self.media.duration>0.0&&wide{
                    let fraction=(self.music_position()/self.media.duration).clamp(0.0,1.0)as f32;
                    c.fill(9.0,height-4.0,width-18.0,2.0,1.0,theme::TRACK);
                    c.fill(9.0,height-4.0,(width-18.0)*fraction,2.0,1.0,theme::CYAN);
                }
            }
            WEATHER_PAGE=>{
                self.control_weather_icon(c,width-22.0,if spacious{height*0.40}else if compact{height/2.0}else{20.0},if spacious{35.0}else{23.0});
                let (temp,place)=self.weather.snapshot.as_ref().map(|snapshot|(format!("{:.0}°",snapshot.temp),snapshot.place.as_str())).unwrap_or_else(||("—".into(),"待更新"));
                c.centered_text(10.0,if spacious{height*0.43}else if compact{height/2.0}else{25.0},&temp,if spacious{30.0}else{23.0},W::Semibold,theme::WHITE,H::Left);
                if spacious{if let Some(snapshot)=&self.weather.snapshot{if let(Some(low),Some(high))=(snapshot.low,snapshot.high){c.centered_text(10.0,height-32.0,&format!("{low:.0}° — {high:.0}°"),11.0,W::Regular,theme::GRAY,H::Left);}}}
                if !compact{c.centered_text(10.0,height-12.0,&c.fit(place,10.5,W::Regular,width-20.0),10.5,W::Regular,theme::GRAY,H::Left);}
            }
            MAC_STATS_PAGE=>{
                let stats=&self.mac_snapshot;let cpu=(!stats.paused).then_some(stats.cpu_usage).flatten();
                let memory=(!stats.paused&&stats.memory_valid&&stats.memory_total>0).then(||stats.memory_used as f32/stats.memory_total as f32*100.0);
                for (index,(label,value,fraction)) in [("CPU",cpu.map(|value|format!("{value:.0}%")).unwrap_or_else(||"—".into()),cpu.map(|value|value as f32/100.0)),("内存",memory.map(|value|format!("{value:.0}%")).unwrap_or_else(||"—".into()),memory.map(|value|value/100.0))].into_iter().enumerate(){
                    let y=if spacious{28.0+index as f32*(height-42.0)/2.0}else if compact{10.0+index as f32*height/2.0}else{17.0+index as f32*26.0};
                    c.centered_text(9.0,y,label,10.0,W::Regular,theme::GRAY,H::Left);
                    c.centered_text(width-9.0,y,&value,if spacious{18.0}else{12.5},W::Semibold,theme::WHITE,H::Right);
                    let track_y=y+if compact{7.0}else{9.0};
                    c.fill(9.0,track_y,width-18.0,2.5,1.25,theme::TRACK);
                    if let Some(fraction)=fraction{c.fill(9.0,track_y,(width-18.0)*fraction.clamp(0.0,1.0),2.5,1.25,if index==0{theme::CYAN}else{theme::SOFT_GREEN});}
                }
            }
            AGENT_PAGE=>{
                paint::glyph(c,"agent",17.0,14.0,14.0,theme::CYAN);
                c.centered_text(29.0,14.0,"Agent",10.5,W::Semibold,theme::GRAY,H::Left);
                if let Some(agent)=summary.agent.as_ref().filter(|_|summary.active){
                    let counts=if agent.approval>0{format!("{} 执行 · {} 等待 · {} 审批",agent.working,agent.waiting,agent.approval)}
                        else{format!("{} 执行 · {} 等待",agent.working,agent.waiting)};
                    c.centered_text(10.0,37.0,&c.fit(&counts,12.0,W::Semibold,width-20.0),12.0,W::Semibold,theme::WHITE,H::Left);
                    let rows=(((height-76.0)/22.0).floor().max(0.0)as usize+1).min(agent.tasks.len());
                    for (index,task) in agent.tasks.iter().take(rows).enumerate(){
                        let y=56.0+index as f32*22.0;
                        let status=task.status.label();let status_width=c.text_w(status,10.0,W::Regular);
                        c.centered_text(10.0,y,&c.fit(&task.title,12.0,W::Semibold,width-28.0-status_width),12.0,W::Semibold,theme::WHITE,H::Left);
                        c.centered_text(width-10.0,y,status,10.0,W::Regular,if matches!(task.status,AgentStatus::Waiting|AgentStatus::Approval){theme::YELLOW}else{theme::CYAN},H::Right);
                    }
                    let remaining=(agent.working+agent.waiting+agent.approval).saturating_sub(rows);
                    if remaining>0{c.centered_text(10.0,height-10.0,&format!("另有 {remaining} 个任务 · 打开查看"),10.0,W::Regular,theme::GRAY,H::Left);}
                }else{
                    c.centered_text(10.0,39.0,&c.fit(&summary.value,13.0,W::Semibold,width-20.0),13.0,W::Semibold,theme::WHITE,H::Left);
                    c.centered_text(10.0,62.0,&c.fit(&summary.detail,11.0,W::Regular,width-20.0),11.0,W::Regular,theme::GRAY,H::Left);
                }
                if summary.active{self.activity_dots(c,width-16.0,14.0);}
            }
            MAIL_PAGE=>{
                let accounts:Vec<_>=self.mail.accounts.iter().filter(|account|account.enabled).collect();
                let connected=accounts.iter().any(|account|self.mail.status_of(&account.email).ok());
                let unread=self.mail.recent.iter().filter(|item|self.mail_seen(item,crate::mail_manage::Folder::Inbox)==Some(false)).count();
                let known=self.mail.recent.iter().any(|item|self.mail_seen(item,crate::mail_manage::Folder::Inbox).is_some());
                paint::glyph(c,"mail",17.0,if compact{height/2.0}else{14.0},15.0,theme::GRAY);
                if !compact{c.centered_text(29.0,14.0,"邮箱",10.0,W::Semibold,theme::GRAY,H::Left);}
                c.circle(width-11.0,if compact{height/2.0}else{14.0},2.5,if connected{theme::SOFT_GREEN}else{theme::DIM});
                let text=if known{format!("{unread} 未读")}else if connected{"已连接".into()}else if accounts.is_empty(){"未添加".into()}else{"待连接".into()};
                c.centered_text(if compact{31.0}else{10.0},if compact{height/2.0}else{middle},&c.fit(&text,if known{14.0}else{12.0},W::Semibold,if compact{width-47.0}else{width-20.0}),if known{14.0}else{12.0},W::Semibold,theme::WHITE,H::Left);
            }
            _=>{
                let (count,unit)=match page{SHELF_PAGE=>(self.shelf.items.len(),"文件"),CLIPBOARD_PAGE=>(self.clipboard.lock().items.len(),"记录"),TODO_PAGE=>(self.library.entries.iter().filter(|entry|!entry.state.archived).count(),"留存"),NOTIFICATIONS_PAGE=>(self.history.len(),"通知"),_=>(0,"")};
                paint::glyph(c,PAGE_GLYPHS[page],17.0,if compact{height/2.0}else{14.0},15.0,if summary.active{theme::CYAN}else{theme::GRAY});
                let text=count.to_string();let number=if compact{18.0}else{number};let w=c.text_w(&text,number,W::Semibold);
                c.centered_text(if compact{33.0}else{10.0},if compact{height/2.0}else{middle},&text,number,W::Semibold,theme::WHITE,H::Left);
                if !compact||page!=SHELF_PAGE{c.centered_text((if compact{37.0+w}else{14.0+w}).min(width-31.0),if compact{height/2.0+1.0}else{middle+2.0},unit,10.0,W::Regular,theme::GRAY,H::Left);}
                if page==SHELF_PAGE&&button(c,width-29.0,if compact{height/2.0-13.0}else{1.0},26.0,"plus","添加文件","control-shelf-add",true){cmds.push(Cmd::AddFiles);}
                if page==CLIPBOARD_PAGE&&spacious{let pinned=self.clipboard.lock().items.iter().filter(|item|item.pinned).count();paint::glyph(c,"pin",15.0,height-17.0,11.0,theme::GRAY);c.centered_text(27.0,height-17.0,&pinned.to_string(),11.0,W::Regular,theme::GRAY,H::Left);}
                if page==TODO_PAGE&&spacious{let pending=self.library.entries.iter().filter(|entry|!entry.state.archived&&!entry.state.reviewed).count();c.centered_text(10.0,height-16.0,&format!("{pending} 待回顾"),10.5,W::Regular,theme::GRAY,H::Left);}
                if page==NOTIFICATIONS_PAGE&&summary.active{c.circle(width-13.0,14.0,3.0,theme::CYAN);}
            }
        }
        if response.clicked(){self.go(page,if page>=self.page{1.0}else{-1.0});}
    }
}
