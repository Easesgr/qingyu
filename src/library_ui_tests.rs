//! All collection examples are synthetic; exercise actual egui event routing.
use super::*;
use super::control_center_tests::{click, draw, key, response};
use crate::{app::{Cmd, LIBRARY_PAGE}, library::Kind, library_ui::{Filter, Scope}, model::Dock};

fn fresh(dock: Dock) -> H {
    let mut h = H::new();
    h.app.library_view = Default::default();
    h.app.settings.dock = dock;
    h.app.settings.agent_activity = false;
    h.app.show_page(LIBRARY_PAGE);
    let origin = h.app.content_origin(Rect::from_min_size(Pos2::ZERO, h.app.canvas_size()));
    h.cursor = Some(origin + egui::vec2(120.0, if dock.is_side() { 240.0 } else { 120.0 }));
    h
}

fn fixture(dock: Dock) -> H {
    let mut h = fresh(dock);
    for _ in 0..65 { draw(&mut h, vec![]); }
    h
}

fn tap(h: &mut H, id: impl std::hash::Hash) {
    let control = response(h, id);
    assert!(h.hit.contains_rect(control.rect), "collection controls stay inside the native hit region");
    click(h, control.rect.center());
    draw(h, vec![]);
}

fn text(h: &H, expected: &str) -> bool {
    h.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(value)
        if value.galley.text().contains(expected) && shape.clip_rect.contains(value.visual_bounding_rect().center())))
}

fn seed_records(h: &mut H) -> Vec<String> {
    let rows = [
        (Kind::Idea, "给灵感留一个能再找到的地方", "把微信里的零散想法集中留存，回头能按主题找到，并继续变成行动。", "产品想法", "我想把随手想到的事情都发给 agent，让它帮我归纳，也保留我最初怎么想的。"),
        (Kind::Link, "值得重看的交互设计视频", "先保留链接与收藏理由，等待阅读视频内容。", "设计参考", "收藏这个视频，之后想研究它的导航和信息层级。 https://example.com/videos/design"),
        (Kind::Action, "试做一版每周灵感回顾", "先挑五条这周的想法，整理成一页可回看的记录。", "产品想法", "这周我想试一下，让 agent 帮我把最近的想法做成一份回顾。"),
        (Kind::Idea, "把收藏理由一起留下", "收藏链接时也记住当时被打动的地方，下一次打开更容易接上思路。", "阅读与学习", "只存网址过几天就忘了为什么要收藏，最好把我的一句感想也留下。"),
    ];
    let mut ids = Vec::new();
    for (i, (kind, title, summary, topic, original)) in rows.into_iter().enumerate() {
        let id = h.app.library.capture(kind, original).unwrap();
        let path = h.app.library.root().join("records").join(format!("{id}.json"));
        let mut record: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        record["title"] = title.into(); record["summary"] = summary.into(); record["topic"] = topic.into();
        record["tags"] = serde_json::json!([topic, "灵感整理"]); record["source"] = "微信 · Hermes".into();
        record["created_at"] = format!("2026-11-15T09:{:02}:00+08:00", 40 - i).into();
        record["updated_at"] = record["created_at"].clone();
        record["next_action"] = "先写下最想尝试的一步，再决定什么时候开始。".into();
        std::fs::write(&path, serde_json::to_vec(&record).unwrap()).unwrap();
        ids.push(id);
    }
    h.app.library.refresh();
    ids
}

fn seed(h: &mut H) -> Vec<String> {
    let ids = seed_records(h);
    draw(h, vec![]);
    ids
}

#[test]
fn library_filters_search_and_keyboard_stay_in_the_collection_on_every_dock() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    assert!(!crate::library_ui::View::default().legacy_tasks);
    assert_eq!(crate::app::TODO_PAGE, LIBRARY_PAGE);
    assert_eq!(crate::app::PAGE_NAMES[LIBRARY_PAGE], "拾光");
    for dock in [Dock::Top, Dock::Left, Dock::Right] {
        let mut h = fixture(dock); let ids = seed(&mut h);
        tap(&mut h, ("library-filter", 2usize));
        assert_eq!(h.app.library_view.filter, Filter::Links);
        assert!(text(&h, "值得重看")); assert!(!text(&h, "给灵感留"));
        tap(&mut h, ("library-filter", 0usize));
        let field = h.ctx.read_response(egui::Id::new(("library-query", true))).unwrap();
        click(&mut h, field.rect.center()); draw(&mut h, vec![egui::Event::Text("每周".into())]);
        key(&mut h, egui::Key::ArrowLeft, egui::Modifiers::NONE);
        assert_eq!(h.app.page(), LIBRARY_PAGE, "search arrow keys edit text instead of switching modules");
        assert!(text(&h, "试做一版")); assert!(!text(&h, "值得重看"));
        tap(&mut h, "library-search-clear");
        tap(&mut h, ("library-entry", ids[0].as_str()));
        assert_eq!(h.app.library_view.selected.as_deref(), Some(ids[0].as_str()));
        assert!(text(&h, "微信 · Hermes"));
        tap(&mut h, "library-copy");
        assert!(h.cmds.iter().any(|cmd| matches!(cmd, Cmd::Copy(body) if body.contains("最初怎么想") && body.contains("集中留存"))));
    }
}

#[test]
fn library_pin_review_archive_and_restore_persist_without_changing_agent_content() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    for dock in [Dock::Top, Dock::Right] {
        let mut h = fixture(dock); let ids = seed(&mut h); let id = &ids[0];
        let path = h.app.library.root().join("records").join(format!("{id}.json"));
        let original_record = std::fs::read(&path).unwrap();
        tap(&mut h, ("library-entry", id.as_str()));
        tap(&mut h, "library-pin"); tap(&mut h, "library-reviewed");
        assert!(h.app.library.entries.iter().find(|entry| &entry.id == id).is_some_and(|entry| entry.state.pinned && entry.state.reviewed));
        tap(&mut h, "library-archive");
        assert!(h.app.library_view.selected.is_none()); assert!(!text(&h, "给灵感留"));
        tap(&mut h, ("library-scope", 3usize));
        assert_eq!(h.app.library_view.scope, Scope::Archived);
        tap(&mut h, ("library-entry", id.as_str())); tap(&mut h, "library-archive");
        assert!(!text(&h, "给灵感留"));
        let reloaded = crate::library::Store::at(h.app.library.root().to_path_buf());
        let entry = reloaded.entries.iter().find(|entry| &entry.id == id).unwrap();
        assert!(entry.state.pinned && entry.state.reviewed && !entry.state.archived);
        assert_eq!(std::fs::read(&path).unwrap(), original_record);
    }
}

#[test]
fn library_manual_capture_keeps_failed_drafts_and_supports_ime_and_actions() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    for dock in [Dock::Top, Dock::Left, Dock::Right] {
        let mut h = fixture(dock);
        tap(&mut h, "library-add"); tap(&mut h, ("library-capture-kind", 1usize));
        draw(&mut h, vec![egui::Event::Text("想收藏的视频".into())]);
        tap(&mut h, "library-capture-save");
        assert!(h.app.library_view.composing && h.app.library.entries.is_empty());
        assert_eq!(h.app.library_view.draft, "想收藏的视频");
        assert!(h.app.library_view.feedback.is_some());
        let field = h.ctx.read_response(egui::Id::new(("library-draft-text", true))).unwrap();
        click(&mut h, field.rect.center());
        draw(&mut h, vec![egui::Event::Ime(egui::ImeEvent::Enabled), egui::Event::Ime(egui::ImeEvent::Preedit("视频".into()))]);
        key(&mut h, egui::Key::Enter, egui::Modifiers::NONE);
        assert!(h.app.library_view.composing && h.app.library.entries.is_empty());
        draw(&mut h, vec![egui::Event::Ime(egui::ImeEvent::Disabled)]);
        h.app.library_view.draft = "一个好视频 https://example.com/watch?v=idea".into();
        tap(&mut h, "library-capture-save");
        assert_eq!(h.app.library.entries.len(), 1);
        assert_eq!(h.app.library.entries[0].source, "手动"); assert!(h.app.library.entries[0].summary.is_empty());
        tap(&mut h, "library-open-url");
        assert!(h.cmds.contains(&Cmd::OpenUrl("https://example.com/watch?v=idea".into())));
        tap(&mut h, "library-to-action");
        assert!(h.app.library_view.composing);
        assert_eq!(h.app.library_view.draft_kind, Some(Kind::Action));
        tap(&mut h, "library-capture-save"); tap(&mut h, "library-reviewed");
        let action = h.app.library.entries.iter().find(|entry| entry.kind == Kind::Action).unwrap();
        assert!(action.state.done);
        tap(&mut h, "library-to-reminder"); assert!(h.app.task_dialog.is_some());
        assert!(h.app.library_view.legacy_tasks); assert_eq!(h.app.todos.items.len(), 1);
    }
}

#[test]
fn library_empty_setup_and_reminder_return_are_reachable() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    for dock in [Dock::Top, Dock::Right] {
        let mut h = fixture(dock);
        assert!(text(&h, "先留住一个想法")); tap(&mut h, "library-empty-connect");
        assert!(h.app.library_view.connect); assert!(text(&h, "随手发给它"));
        assert!(!text(&h, "微信已连接")); tap(&mut h, "library-back");
        tap(&mut h, "library-tasks"); assert!(h.app.library_view.legacy_tasks);
        tap(&mut h, "library-from-tasks"); assert!(!h.app.library_view.legacy_tasks);
        assert!(text(&h, "先留住一个想法"));
    }
}

#[test]
fn library_larger_side_panels_show_more_content_and_scrolling_never_switches_modules() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut visible = Vec::new();
    for (dock, width, height) in [(Dock::Top, 292, 532), (Dock::Right, 292, 532), (Dock::Left, 480, 820)] {
        let mut h = fixture(dock); h.app.settings.side_panel_width = width; h.app.settings.side_panel_height = height;
        let mut ids = seed(&mut h);
        for index in 0..16 { ids.push(h.app.library.capture(Kind::Idea, &format!("内容容量测试 {index:02}" )).unwrap()); }
        for _ in 0..4 { draw(&mut h, vec![]); }
        let before = h.shapes.iter().filter(|shape| matches!(&shape.shape, egui::Shape::Text(value)
            if value.galley.text().contains("内容容量测试") && shape.clip_rect.contains_rect(value.visual_bounding_rect()))).count();
        if dock.is_side() { visible.push(before); }
        let origin = h.app.content_origin(Rect::from_min_size(Pos2::ZERO, h.app.canvas_size()));
        let point = origin + egui::vec2(if dock.is_side() { 120.0 } else { 240.0 }, if dock.is_side() { 370.0 } else { 170.0 });
        h.cursor = Some(point);
        for _ in 0..6 { draw(&mut h, vec![egui::Event::PointerMoved(point), egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: egui::vec2(0.0, -120.0), modifiers: Default::default() }]); }
        assert_eq!(h.app.page(), LIBRARY_PAGE);
        let overflow: Vec<_> = h.shapes.iter().filter_map(|shape| if let egui::Shape::Text(value) = &shape.shape {
            let bounds = value.visual_bounding_rect();
            (value.galley.text().contains("内容容量测试") && (bounds.left() < h.hit.left() - 1.0 || bounds.right() > h.hit.right() + 1.0)).then(|| (value.galley.text(), bounds, shape.clip_rect))
        } else { None }).collect();
        assert!(overflow.is_empty(), "{dock:?}/{width}: hit {:?}, text overflow {overflow:?}", h.hit);
    }
    assert!(visible[1] > visible[0] + 2, "larger panels expose more records: {visible:?}");
}

#[test]
#[ignore = "Synthetic rendered collection gallery: QINGYU_LIBRARY_PREVIEW_DIR=directory"]
fn library_gallery() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dest = std::path::PathBuf::from(std::env::var_os("QINGYU_LIBRARY_PREVIEW_DIR").expect("preview directory"));
    std::fs::create_dir_all(&dest).unwrap();
    for (name, dock, large, detail, empty) in [
        ("top", Dock::Top, false, false, false), ("left", Dock::Left, false, false, false),
        ("right", Dock::Right, false, false, false), ("large", Dock::Right, true, false, false),
        ("detail", Dock::Right, false, true, false), ("empty", Dock::Right, false, false, true),
    ] {
        // Start the raster before the first frame so every texture upload is
        // retained; otherwise cached PNG icons appear as solid rectangles.
        let mut h = fresh(dock);
        let ids = if empty { vec![] } else { seed_records(&mut h) };
        if large { h.app.settings.side_panel_width = 480; h.app.settings.side_panel_height = 820; }
        if detail { h.app.library_view.selected = Some(ids[0].clone()); }
        h.ctx.set_pixels_per_point(2.0);
        let size = h.app.canvas_size(); let canvas = if dock.is_side() { size } else { egui::vec2(crate::app::CANVAS_W, size.y) };
        let (w, height) = ((canvas.x * 2.0) as usize, (canvas.y * 2.0) as usize);
        let mut raster = crate::preview_raster::Raster::new(w, height);
        for frame in 0..65 {
            let (output, _) = draw(&mut h, vec![]); raster.update_textures(&output.textures_delta);
            if frame == 64 {
                raster.buf.fill([0.12, 0.17, 0.19, 1.0]);
                raster.draw(&h.ctx.tessellate(output.shapes, output.pixels_per_point), output.pixels_per_point);
                let mut rgba = Vec::new(); raster.to_rgba8(&mut rgba);
                let screenshot = image::RgbaImage::from_raw(w as u32, height as u32, rgba).unwrap();
                let crop = h.hit.expand(20.0).intersect(Rect::from_min_size(Pos2::ZERO, canvas));
                let (left, top) = ((crop.left() * 2.0).max(0.0) as u32, (crop.top() * 2.0).max(0.0) as u32);
                let (cw, ch) = ((crop.width() * 2.0) as u32, (crop.height() * 2.0) as u32);
                image::imageops::crop_imm(&screenshot, left, top, cw.min(w as u32-left), ch.min(height as u32-top))
                    .to_image().save(dest.join(format!("shiguang-{name}.png"))).unwrap();
            }
            raster.free_textures(&output.textures_delta);
        }
    }
}
