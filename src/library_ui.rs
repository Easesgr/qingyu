//! 拾光：Hermes 整理的想法、收藏与行动，共用顶部和侧边的浏览状态。
use crate::{
    app::{App, Cmd, Env},
    library::{Entry, Kind},
    paint::{self, Card, H},
    theme::{self, W},
};
use eframe::egui::{self, Color32};

const SURFACE: Color32 = Color32::from_rgb(25, 53, 58);
const SELECTED: Color32 = Color32::from_rgb(37, 79, 85);
const SECONDARY: Color32 = Color32::from_rgb(173, 192, 195);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) enum Filter {
    #[default]
    All,
    Ideas,
    Links,
    Actions,
}

impl Filter {
    const ALL: [Self; 4] = [Self::All, Self::Ideas, Self::Links, Self::Actions];
    fn label(self) -> &'static str {
        match self { Self::All => "全部", Self::Ideas => "想法", Self::Links => "收藏", Self::Actions => "行动" }
    }
    fn accepts(self, kind: Kind) -> bool {
        match self { Self::All => true, Self::Ideas => kind == Kind::Idea, Self::Links => kind == Kind::Link, Self::Actions => kind == Kind::Action }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) enum Scope {
    #[default]
    All,
    Unreviewed,
    Pinned,
    Archived,
}

#[derive(Default)]
pub(crate) struct View {
    pub filter: Filter,
    pub scope: Scope,
    pub query: String,
    pub selected: Option<String>,
    pub composing: bool,
    pub draft: String,
    pub draft_kind: Option<Kind>,
    pub connect: bool,
    pub legacy_tasks: bool,
    pub text_focused: bool,
    pub focus_draft: bool,
    pub last_refresh: f64,
    pub feedback: Option<String>,
    pub installed: bool,
}

pub(crate) fn matches(entry: &Entry, view: &View) -> bool {
    if !view.filter.accepts(entry.kind) { return false; }
    if entry.state.archived != (view.scope == Scope::Archived) { return false; }
    if view.scope == Scope::Pinned && !entry.state.pinned { return false; }
    if view.scope == Scope::Unreviewed && (if entry.kind == Kind::Action { entry.state.done } else { entry.state.reviewed }) { return false; }
    let query = view.query.trim().to_lowercase();
    if query.is_empty() { return true; }
    let haystack = format!("{} {} {} {} {} {} {}", entry.title, entry.summary, entry.original,
        entry.url.as_deref().unwrap_or_default(), entry.tags.join(" "),
        entry.topic.as_deref().unwrap_or_default(), entry.next_action.as_deref().unwrap_or_default()).to_lowercase();
    query.split_whitespace().all(|term| haystack.contains(term))
}

fn button(c: &Card, x: f32, y: f32, width: f32, name: &str, selected: bool, id: impl std::hash::Hash) -> bool {
    let response = c.hit(x, y, width, 28.0, id).on_hover_text(name);
    response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, c.live, selected, name));
    if selected || response.hovered() || response.has_focus() {
        c.fill(x, y, width, 28.0, 7.0, if selected { SELECTED } else { SURFACE });
    }
    if response.has_focus() { c.stroke(x, y, width, 28.0, 7.0, 1.3, theme::CYAN); }
    c.centered_text(x + width / 2.0, y + 14.0, &c.fit(name, 11.5, W::Semibold, width - 10.0),
        11.5, W::Semibold, if selected { theme::CYAN } else { theme::WHITE }, H::Center);
    response.clicked()
}

fn icon_button(c: &Card, x: f32, y: f32, glyph: &str, name: &str, id: impl std::hash::Hash) -> bool {
    let response = c.hit(x, y, 28.0, 28.0, id).on_hover_text(name);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, c.live, name));
    if response.hovered() || response.has_focus() { c.fill(x, y, 28.0, 28.0, 7.0, SURFACE); }
    if response.has_focus() { c.stroke(x, y, 28.0, 28.0, 7.0, 1.3, theme::CYAN); }
    paint::glyph(c, glyph, x + 14.0, y + 14.0, 16.0, theme::CYAN);
    response.clicked()
}

fn paragraph(ui: &mut egui::Ui, c: &Card, text: &str, size: f32, color: Color32, bold: bool) {
    let font = c.font(size, if bold { W::Semibold } else { W::Regular });
    ui.add(egui::Label::new(egui::RichText::new(text).font(font).color(color)).wrap());
}

fn section(ui: &mut egui::Ui, c: &Card, label: &str, text: &str) {
    if text.trim().is_empty() { return; }
    ui.add_space(12.0 * c.s);
    paragraph(ui, c, label, 11.0, theme::CYAN, true);
    ui.add_space(3.0 * c.s);
    paragraph(ui, c, text, 13.0, theme::WHITE, false);
}

fn display_time(value: &str, compact: bool) -> String {
    chrono::DateTime::parse_from_rfc3339(value).map(|date| date.with_timezone(&chrono::Local)
        .format(if compact { "%m-%d" } else { "%Y-%m-%d %H:%M" }).to_string())
        .unwrap_or_else(|_| value.chars().take(if compact { 10 } else { 16 }).collect())
}

impl App {
    pub(crate) fn library_settings_ui(&mut self, ui: &mut egui::Ui) {
        use crate::extras::{settings_card, settings_hint, settings_label};
        settings_label(ui, "想法、收藏与行动");
        settings_card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.add_sized([110.0, 30.0], egui::Button::new("打开拾光")).clicked() {
                    self.settings_open = false; self.library_view.legacy_tasks = false;
                    self.library_view.connect = false; self.library_view.selected = None;
                    self.show_page(crate::app::TODO_PAGE);
                }
                if ui.add_sized([110.0, 30.0], egui::Button::new("打开提醒清单")).clicked() {
                    self.settings_open = false; self.library_view.legacy_tasks = true;
                    self.show_page(crate::app::TODO_PAGE);
                }
            });
        });
        settings_label(ui, "Hermes 整理能力");
        settings_card(ui, |ui| {
            let installed = crate::library::hermes_installed(self.library.root());
            ui.label(if installed { "整理能力已安装" } else { "尚未安装拾光整理能力" });
            settings_hint(ui, "安装后新开 Hermes 会话；微信需先连接 Hermes。");
            ui.add_space(6.0);
            if ui.add_sized([176.0, 30.0], egui::Button::new(if installed { "检查 / 更新整理能力" } else { "安装 Hermes 整理能力" })).clicked() { self.install_library_hermes(); }
            if let Some(message) = &self.library_view.feedback { ui.add_space(5.0); ui.label(message); }
        });
        settings_label(ui, "本地收藏库");
        settings_card(ui, |ui| {
            if ui.add_sized([132.0, 30.0], egui::Button::new("打开资料文件夹")).clicked() { self.pending.push(Cmd::Open(self.library.root().to_path_buf())); }
        });
    }

    pub(crate) fn library_poll(&mut self) {
        if self.module_on(crate::app::TODO_PAGE) && self.t - self.library_view.last_refresh >= 1.0 {
            self.library_view.last_refresh = self.t;
            self.library.refresh();
        }
    }

    pub(crate) fn library_open_connect(&mut self) {
        self.library_view.installed = crate::library::hermes_installed(self.library.root());
        self.library_view.connect = true;
        self.library_view.selected = None;
        self.library_view.composing = false;
        self.library_view.legacy_tasks = false;
    }

    pub(crate) fn install_library_hermes(&mut self) {
        self.library_view.feedback = Some(match crate::library::install_hermes(self.library.root()) {
            Ok(message) => { self.library_view.installed = true; message }
            Err(message) => message,
        });
    }

    pub(crate) fn library_compose(&mut self, kind: Kind, text: String) {
        self.library_view.composing = true;
        self.library_view.selected = None;
        self.library_view.connect = false;
        self.library_view.draft_kind = Some(kind);
        self.library_view.draft = text;
        self.library_view.feedback = None;
        self.library_view.focus_draft = true;
        self.pending.push(Cmd::Focus);
    }

    fn library_save_capture(&mut self) {
        let kind = self.library_view.draft_kind.unwrap_or(Kind::Idea);
        match self.library.capture(kind, &self.library_view.draft) {
            Ok(id) => {
                self.library_view.composing = false;
                self.library_view.draft.clear();
                self.library_view.selected = Some(id);
                self.library_view.text_focused = false;
                self.library_view.feedback = None;
                self.say("已收进拾光");
            }
            Err(message) => self.library_view.feedback = Some(message),
        }
    }

    pub(crate) fn page_library(&mut self, c: &Card, env: &Env, cmds: &mut Vec<Cmd>, side: bool) {
        if self.library_view.legacy_tasks {
            if side { self.portrait_todo(c, env); } else { self.page_todo(c, env, cmds); }
            let (x, y) = if side { (crate::portrait::INSET, self.expanded_height() - 34.0) } else { (44.0, self.content_rect().bottom()-28.0) };
            if button(c, x, y, if side { 78.0 } else { 88.0 }, "返回拾光", false, "library-from-tasks") {
                self.cancel_edit();
                self.library_view.legacy_tasks = false;
            }
            return;
        }
        let height = self.expanded_height();
        let inset=crate::portrait::INSET;
        let (left, top, width, bottom) = if side { (inset, 112.0, self.expanded_width() - inset*2.0, height - 40.0) }
            else { (44.0, 36.0, 331.0, height - if cfg!(target_os = "macos") { 47.0 } else { 17.0 }) };

        if self.library_view.connect {
            self.library_connection(c, left, top, width, bottom - top, cmds);
            return;
        }
        if self.library_view.composing {
            self.library_capture(c, left, top, width, bottom - top);
            return;
        }
        if let Some(entry) = self.library_view.selected.as_ref().and_then(|id| self.library.entries.iter().find(|entry| &entry.id == id)).cloned() {
            self.library_detail(c, &entry, left, top, width, bottom - top, cmds);
            return;
        }
        self.library_view.selected = None;

        let count = self.library.entries.iter().filter(|entry| !entry.state.archived).count();
        let pending = self.library.entries.iter().filter(|entry| !entry.state.archived &&
            if entry.kind == Kind::Action { !entry.state.done } else { !entry.state.reviewed }).count();
        let list_left;
        let list_width;
        let search_y;
        let list_top;
        if side {
            c.centered_text(left, top + 13.0, "拾光", 20.0, W::Semibold, theme::WHITE, H::Left);
            c.centered_text(left, top + 42.0, &format!("{count} 条留存 · {pending} 条待回顾"), 11.5, W::Regular, SECONDARY, H::Left);
            if icon_button(c, left + width - 62.0, top - 1.0, "agent", "Hermes 整理与微信接入", "library-connect") { self.library_open_connect(); }
            if icon_button(c, left + width - 28.0, top - 1.0, "plus", "记下想法或收藏链接", "library-add") { self.library_compose(Kind::Idea, String::new()); }
            search_y = top + 62.0;
            let gap = 4.0;
            let tab_w = (width - gap * 3.0) / 4.0;
            for (index, filter) in Filter::ALL.into_iter().enumerate() {
                if button(c, left + index as f32 * (tab_w + gap), top + 101.0, tab_w, filter.label(), self.library_view.filter == filter, ("library-filter", index)) {
                    self.library_view.filter = filter;
                }
            }
            self.library_scopes(c, left, top + 139.0, width);
            list_left = left;
            list_width = width;
            list_top = top + 175.0;
        } else {
            // The top dock uses its width: a stable category column and a reading column.
            for (index, filter) in Filter::ALL.into_iter().enumerate() {
                let n = self.library.entries.iter().filter(|entry| !entry.state.archived && filter.accepts(entry.kind)).count();
                if button(c, left, top + index as f32 * 32.0, 78.0, &format!("{} {n}", filter.label()), self.library_view.filter == filter, ("library-filter", index)) {
                    self.library_view.filter = filter;
                }
            }
            if button(c, left, top + 137.0, 78.0, "提醒清单", false, "library-tasks") { self.library_view.legacy_tasks = true; }
            if icon_button(c, left + 3.0, bottom - 29.0, "agent", "Hermes 整理与微信接入", "library-connect") { self.library_open_connect(); }
            if icon_button(c, left + 42.0, bottom - 29.0, "plus", "记下想法或收藏链接", "library-add") { self.library_compose(Kind::Idea, String::new()); }
            c.seg((left + 89.0, top + 4.0), (left + 89.0, bottom - 4.0), 0.5, paint::alpha(SECONDARY, 0.25));
            list_left = left + 101.0;
            list_width = width - 101.0;
            search_y = top;
            self.library_scopes(c, list_left, top + 37.0, list_width);
            list_top = top + 74.0;
        }
        self.library_search(c, list_left, search_y, list_width);
        let error_height = if self.library.error.is_some() { 22.0 } else { 0.0 };
        if let Some(error) = &self.library.error {
            let response = c.hit(list_left, list_top, list_width, 21.0, "library-error").on_hover_text(error);
            response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, c.live, error));
            c.centered_text(list_left, list_top + 9.0, "部分内容读取失败，稍后会重试", 10.5, W::Regular, theme::YELLOW, H::Left);
        }
        self.library_list(c, list_left, list_top + error_height, list_width, (bottom - list_top - error_height).max(36.0), side);
        if side {
            let label = format!("提醒清单 {}", self.todos.items.iter().filter(|item| !item.done_for(&env.today)).count());
            if button(c, left + width - 96.0, height - 34.0, 96.0, &label, false, "library-tasks") { self.library_view.legacy_tasks = true; }
            c.centered_text(left, height - 20.0, "微信发给 Hermes", 10.0, W::Regular, SECONDARY, H::Left);
        }
    }

    fn library_search(&mut self, c: &Card, x: f32, y: f32, width: f32) {
        c.fill(x, y, width, 29.0, 7.0, SURFACE);
        paint::glyph(c, "search", x + 14.0, y + 14.5, 13.0, SECONDARY);
        let mut ui = crate::portrait::scroll_ui(c, x + 27.0, y + 4.0, width - 53.0, 23.0, "library-search-input");
        let response = ui.add_sized([(width - 53.0) * c.s, 23.0 * c.s], egui::TextEdit::singleline(&mut self.library_view.query)
            .id(egui::Id::new(("library-query", c.live))).frame(false).margin(egui::Margin::ZERO)
            .font(c.font(12.0, W::Regular)).text_color(theme::WHITE)
            .hint_text(egui::RichText::new("搜索想法、链接、标签").font(c.font(11.0, W::Regular)).color(SECONDARY)));
        self.library_view.text_focused = response.has_focus();
        if response.has_focus() { c.stroke(x, y, width, 29.0, 7.0, 1.2, theme::CYAN); }
        if !self.library_view.query.is_empty() && icon_button(c, x + width - 28.0, y + 0.5, "close", "清空搜索", "library-search-clear") { self.library_view.query.clear(); }
    }

    fn library_scopes(&mut self, c: &Card, x: f32, y: f32, width: f32) {
        let choices = [(Scope::All, "最近"), (Scope::Unreviewed, "待回顾"), (Scope::Pinned, "置顶"), (Scope::Archived, "归档")];
        let w = (width - 9.0) / 4.0;
        for (index, (scope, label)) in choices.into_iter().enumerate() {
            if button(c, x + index as f32 * (w + 3.0), y, w, label, self.library_view.scope == scope, ("library-scope", index)) { self.library_view.scope = scope; }
        }
    }

    fn library_list(&mut self, c: &Card, x: f32, y: f32, width: f32, height: f32, side: bool) {
        let entries: Vec<_> = self.library.entries.iter().filter(|entry| matches(entry, &self.library_view)).collect();
        let mut content = crate::portrait::scroll_ui(c, x, y, width, height, "library-list");
        let mut selected = None;
        let mut topic = None;
        egui::ScrollArea::vertical().id_salt(("library-list", c.live, self.library_view.filter, self.library_view.scope))
            .auto_shrink([false, false]).max_height(height * c.s).show(&mut content, |ui| {
                if entries.is_empty() {
                    ui.add_space(((height-140.0)/2.0).max(4.0)*c.s);
                    let (title, description) = if !self.library_view.query.trim().is_empty() { ("还没找到这条内容", "试试主题、标签，或原话里的几个字。") }
                        else if self.library_view.scope == Scope::Archived { ("归档里还没有内容", "暂时用不到的内容可以归档，之后随时找回。") }
                        else if self.library_view.scope == Scope::Pinned { ("把常用的留在手边", "打开一条内容，点「置顶」即可。") }
                        else if self.library_view.scope == Scope::Unreviewed { ("都回顾过了", "新的想法、收藏和未完成行动会出现在这里。") }
                        else if self.library.entries.is_empty() { ("先留住一个想法", "在微信发给 Hermes：\n「帮我记下这个想法……」\n或「收藏这个视频链接」。") }
                        else { ("这一类还没有内容", "换个分类看看，也可以点加号手动记一条。") };
                    paragraph(ui, c, title, 15.0, theme::WHITE, true);
                    ui.add_space(7.0 * c.s);
                    paragraph(ui, c, description, 12.0, SECONDARY, false);
                    if self.library.entries.is_empty() && self.library_view.scope == Scope::All {
                        ui.add_space(12.0 * c.s);
                        let (rect, _) = ui.allocate_exact_size(egui::vec2((width - 10.0) * c.s, 30.0 * c.s), egui::Sense::hover());
                        let mut row = Card::new(ui, rect.min, ui.clip_rect()); row.s = c.s; row.live = c.live;
                        if button(&row, 0.0, 0.0, (width - 10.0).min(158.0), "设置 Hermes 整理", true, "library-empty-connect") { selected = Some(String::new()); }
                    }
                }
                for entry in &entries {
                    let row_h = if side { 94.0 } else { 66.0 };
                    let row_width=ui.available_width()/c.s;
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(row_width * c.s, row_h * c.s), egui::Sense::hover());
                    let mut row = Card::new(ui, rect.min, ui.clip_rect()); row.s = c.s; row.live = c.live;
                    let w = row_width;
                    let response = row.hit(0.0, 0.0, w, row_h, ("library-entry", entry.id.as_str())).on_hover_text(&entry.title);
                    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, c.live, format!("{}，{}，打开详情", entry.kind.label(), entry.title)));
                    if response.hovered() || response.has_focus() { row.fill(0.0, 0.0, w, row_h - 2.0, 8.0, SURFACE); }
                    if response.has_focus() { row.stroke(0.5, 0.5, w - 1.0, row_h - 3.0, 8.0, 1.3, theme::CYAN); }
                    let complete = entry.kind == Kind::Action && entry.state.done;
                    paint::glyph(&row, if complete { "check" } else { entry.kind.glyph() }, 10.0, 15.0, 13.0, theme::CYAN);
                    let kind_label = if complete { "已完成" } else { entry.kind.label() };
                    row.centered_text(23.0, 15.0, kind_label, 10.5, W::Regular, SECONDARY, H::Left);
                    let date = display_time(&entry.created_at, true);
                    row.centered_text(w - 6.0, 15.0, &date, 10.0, W::Regular, SECONDARY, H::Right);
                    if entry.state.pinned { paint::glyph(&row, "pin", w - 49.0, 15.0, 11.0, theme::CYAN); }
                    let title = row.fit(&entry.title, 13.0, W::Semibold, w - 13.0);
                    row.centered_text(5.0, 37.0, &title, 13.0, W::Semibold, if complete { SECONDARY } else { theme::WHITE }, H::Left);
                    let excerpt = if entry.summary.is_empty() { &entry.original } else { &entry.summary };
                    row.centered_text(5.0, 57.0, &row.fit(&excerpt.replace('\n', " "), 11.5, W::Regular, w - 13.0), 11.5, W::Regular, SECONDARY, H::Left);
                    if side {
                        if let Some(label) = entry.topic.as_deref().filter(|text| !text.is_empty()).or_else(|| entry.tags.first().map(String::as_str)) {
                            let label = row.fit(label, 10.5, W::Regular, w - 34.0);
                            row.centered_text(5.0, 79.0, &label, 10.5, W::Regular, theme::CYAN, H::Left);
                            // The whole row remains the navigation target; filtering by every tag is available in detail.
                        } else {
                            row.centered_text(5.0, 79.0, &row.fit(&entry.source, 10.5, W::Regular, w - 13.0), 10.5, W::Regular, SECONDARY, H::Left);
                        }
                        if entry.state.reviewed && entry.kind != Kind::Action { paint::glyph(&row, "check", w - 10.0, 79.0, 11.0, SECONDARY); }
                    }
                    row.seg((5.0, row_h - 1.0), (w - 5.0, row_h - 1.0), 0.5, paint::alpha(SECONDARY, 0.20));
                    if response.clicked() { selected = Some(entry.id.clone()); }
                    response.context_menu(|ui| {
                        if let Some(value) = &entry.topic {
                            if ui.button(format!("查看「{value}」主题")).clicked() { topic = Some(value.clone()); ui.close_menu(); }
                        }
                        if ui.button("查看原话与整理结果").clicked() { selected = Some(entry.id.clone()); ui.close_menu(); }
                    });
                }
            });
        if let Some(id) = selected {
            if id.is_empty() { self.library_open_connect(); } else { self.library_view.selected = Some(id); self.library_view.text_focused = false; }
        }
        if let Some(value) = topic { self.library_view.query = value; }
    }

    fn library_detail(&mut self, c: &Card, entry: &Entry, x: f32, y: f32, width: f32, height: f32, cmds: &mut Vec<Cmd>) {
        if button(c, x, y, 62.0, "返回", false, "library-back") { self.library_view.selected = None; return; }
        if let Some(url) = &entry.url {
            if icon_button(c, x + width - 96.0, y, "link", "打开原链接", "library-open-url") { cmds.push(Cmd::OpenUrl(url.clone())); }
        }
        if icon_button(c, x + width - 62.0, y, "clipboard", "复制原话与整理结果", "library-copy") {
            cmds.push(Cmd::Copy(format!("{}\n\n{}\n\n{}{}", entry.title, entry.summary, entry.original,
                entry.url.as_ref().map(|url| format!("\n\n{url}")).unwrap_or_default())));
        }
        if icon_button(c, x + width - 28.0, y, "pin", if entry.state.pinned { "取消置顶" } else { "置顶这条内容" }, "library-pin") {
            let mut state = entry.state.clone(); state.pinned = !state.pinned;
            if let Err(error) = self.library.set_state(&entry.id, state) { self.say(error); }
        }
        let mut content = crate::portrait::scroll_ui(c, x, y + 38.0, width, (height - 82.0).max(40.0), "library-detail");
        let mut search = None;
        egui::ScrollArea::vertical().id_salt(("library-detail", &entry.id, c.live)).auto_shrink([false, false])
            .max_height((height - 82.0).max(40.0) * c.s).show(&mut content, |ui| {
                paragraph(ui, c, &entry.title, 17.0, theme::WHITE, true);
                ui.add_space(5.0 * c.s);
                paragraph(ui, c, &format!("{} · {}", entry.kind.label(), entry.source), 10.5, SECONDARY, false);
                if !entry.created_at.is_empty() { paragraph(ui, c, &display_time(&entry.created_at, false), 10.5, SECONDARY, false); }
                if let Some(topic) = &entry.topic {
                    ui.add_space(9.0 * c.s);
                    let response = ui.add(egui::Button::new(egui::RichText::new(format!("主题 · {topic}")).font(c.font(11.0, W::Regular)).color(theme::CYAN)).frame(false).wrap());
                    if response.clicked() { search = Some(topic.clone()); }
                }
                if !entry.tags.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        for tag in &entry.tags {
                            let response = ui.add(egui::Button::new(egui::RichText::new(format!("#{tag}")).font(c.font(11.0, W::Regular)).color(theme::CYAN)).frame(false));
                            if response.clicked() { search = Some(tag.clone()); }
                        }
                    });
                }
                section(ui, c, "整理摘要", &entry.summary);
                section(ui, c, "下一步", entry.next_action.as_deref().unwrap_or_default());
                section(ui, c, "我的原话", &entry.original);
                if let Some(url) = &entry.url { section(ui, c, "原链接", url); }
            });
        let action_y = y + height - 34.0;
        let w = (width - 12.0) / 3.0;
        let done = if entry.kind == Kind::Action { entry.state.done } else { entry.state.reviewed };
        let label = if entry.kind == Kind::Action { if done { "重新打开" } else { "完成行动" } } else if done { "标为未回顾" } else { "已回顾" };
        if button(c, x, action_y, w, label, done, "library-reviewed") {
            let mut state = entry.state.clone();
            if entry.kind == Kind::Action { state.done = !state.done; } else { state.reviewed = !state.reviewed; }
            if let Err(error) = self.library.set_state(&entry.id, state) { self.say(error); }
        }
        if entry.kind != Kind::Action {
            if button(c, x + w + 6.0, action_y, w, "转为行动", false, "library-to-action") {
                let next = entry.next_action.as_deref().filter(|text| !text.is_empty()).unwrap_or(&entry.title);
                let text = format!("{next}\n\n来自想法：{}\n{}", entry.title, entry.original);
                self.library_compose(Kind::Action, text);
            }
        } else if button(c, x + w + 6.0, action_y, w, "另建提醒", false, "library-to-reminder") {
            let existing = self.todos.items.iter().position(|item| item.text == entry.title);
            let index = existing.unwrap_or_else(|| { self.todos.items.insert(0, crate::model::TodoItem::new(&entry.title)); self.todos.save(); 0 });
            self.library_view.legacy_tasks = true;
            self.open_task_reminder(index);
        }
        if button(c, x + 2.0 * (w + 6.0), action_y, w, if entry.state.archived { "移回收藏库" } else { "归档" }, false, "library-archive") {
            let mut state = entry.state.clone(); state.archived = !state.archived;
            match self.library.set_state(&entry.id, state) {
                Ok(()) => { self.library_view.selected = None; self.say(if entry.state.archived { "已移回拾光" } else { "已归档，可在归档中找回" }); }
                Err(error) => self.say(error),
            }
        }
        if let Some(query) = search { self.library_view.query = query; self.library_view.filter = Filter::All; self.library_view.scope = Scope::All; self.library_view.selected = None; }
    }

    fn library_capture(&mut self, c: &Card, x: f32, y: f32, width: f32, height: f32) {
        if button(c, x, y, 62.0, "取消", false, "library-capture-cancel") {
            self.library_view.composing = false; self.library_view.text_focused = false; self.library_view.feedback = None; return;
        }
        c.centered_text(x + width, y + 14.0, "手动记一条", 13.0, W::Semibold, theme::WHITE, H::Right);
        let kinds = [Kind::Idea, Kind::Link, Kind::Action];
        let w = (width - 8.0) / 3.0;
        for (index, kind) in kinds.into_iter().enumerate() {
            if button(c, x + index as f32 * (w + 4.0), y + 38.0, w, kind.label(), self.library_view.draft_kind == Some(kind), ("library-capture-kind", index)) {
                self.library_view.draft_kind = Some(kind);
                self.library_view.focus_draft = true;
                self.library_view.feedback = None;
            }
        }
        let input_h = (height - 133.0).max(44.0);
        c.fill(x, y + 77.0, width, input_h, 8.0, SURFACE);
        let mut content = crate::portrait::scroll_ui(c, x + 10.0, y + 84.0, width - 20.0, input_h - 14.0, "library-draft");
        let response = egui::ScrollArea::vertical().id_salt(("library-draft", c.live)).auto_shrink([false, false]).show(&mut content, |ui| {
            ui.add_sized([(width - 24.0) * c.s, (input_h - 14.0) * c.s], egui::TextEdit::multiline(&mut self.library_view.draft)
                .id(egui::Id::new(("library-draft-text", c.live))).frame(false).margin(egui::Margin::ZERO)
                .font(c.font(13.0, W::Regular)).text_color(theme::WHITE)
                .hint_text(egui::RichText::new("写下想法，或粘贴想收藏的链接……").color(SECONDARY).font(c.font(12.0, W::Regular))))
        }).inner;
        if self.library_view.focus_draft && c.live { response.request_focus(); self.library_view.focus_draft = false; }
        self.library_view.text_focused = response.has_focus();
        if response.has_focus() { c.stroke(x, y + 77.0, width, input_h, 8.0, 1.2, theme::CYAN); }
        if let Some(error) = &self.library_view.feedback {
            c.centered_text(x, y + height - 42.0, &c.fit(error, 10.5, W::Regular, width), 10.5, W::Regular, theme::YELLOW, H::Left);
        }
        if button(c, x + width - 88.0, y + height - 30.0, 88.0, "收进拾光", true, "library-capture-save") { self.library_save_capture(); }
        c.centered_text(x, y + height - 16.0, "保留原文 · 仅存本机", 10.5, W::Regular, SECONDARY, H::Left);
    }

    fn library_connection(&mut self, c: &Card, x: f32, y: f32, width: f32, height: f32, cmds: &mut Vec<Cmd>) {
        if button(c, x, y, 62.0, "返回", false, "library-back") { self.library_view.connect = false; return; }
        c.centered_text(x + width, y + 14.0, "Hermes 整理", 14.0, W::Semibold, theme::WHITE, H::Right);
        let mut content = crate::portrait::scroll_ui(c, x, y + 40.0, width, (height - 82.0).max(40.0), "library-connection");
        egui::ScrollArea::vertical().id_salt(("library-connection", c.live)).auto_shrink([false, false])
            .max_height((height - 82.0).max(40.0) * c.s).show(&mut content, |ui| {
                paragraph(ui, c, "随手发给它，整理留在这里。", 16.0, theme::WHITE, true);
                ui.add_space(8.0 * c.s);
                paragraph(ui, c, "在微信明确说「收藏这个」「记个想法」或「加入行动」，Hermes 才会整理并收录。普通聊天、提问和接入调试不会写进收藏库。", 12.0, SECONDARY, false);
                section(ui, c, "1 · 安装整理能力", if self.library_view.installed { "整理能力已安装。新开一个 Hermes 会话后使用；微信连接由 Hermes 管理。" } else { "为这台 Mac 的 Hermes 添加「拾光」整理技能。收藏保存在轻屿本地，可在这里回顾。" });
                section(ui, c, "2 · 在微信里试一句", "帮我记个想法：做一个每周自动回顾的灵感库。\n\n收藏这个视频，顺便记下我为什么想看：……\n\n帮我记一件要做的事：……");
                section(ui, c, "3 · 回来翻翻", "按想法、收藏或行动筛选；用主题和标签搜索。重要内容置顶，读过标记回顾，要做的事情标记完成。");
                ui.add_space(12.0 * c.s);
                paragraph(ui, c, "需要先把微信接到 Hermes。安装技能不会替你登录微信；暂时读不到的视频也会保留链接，不会编造摘要。", 11.0, SECONDARY, false);
                if let Some(message) = &self.library_view.feedback { section(ui, c, "安装结果", message); }
            });
        if button(c, x, y + height - 31.0, (width - 8.0) * 0.62, if self.library_view.installed { "检查 / 更新整理能力" } else { "安装 Hermes 整理能力" }, true, "library-install") { self.install_library_hermes(); }
        if button(c, x + (width - 8.0) * 0.62 + 8.0, y + height - 31.0, (width - 8.0) * 0.38, "本地资料", false, "library-folder") { cmds.push(Cmd::Open(self.library.root().to_path_buf())); }
    }
}
