//! Real egui pointer/keyboard input: settings have one explicit island entry.
use super::*;
use super::control_center_tests::{click, draw, key, response};
use crate::{app::*, model::Dock};

fn panel(dock: Dock, width: u32, height: u32, scale: f32) -> H {
    let mut h = H::new();
    h.app.settings.dock = dock;
    h.app.settings.side_panel_width = width;
    h.app.settings.side_panel_height = height;
    h.app.settings.clipboard_history = false;
    h.app.settings.agent_activity = false;
    h.app.settings.lyrics_enabled = false;
    h.ctx.set_pixels_per_point(scale);
    h.app.show_control_center();
    settle(&mut h);
    h
}

fn settle(h: &mut H) {
    for _ in 0..65 { draw(h, vec![]); }
}

// A display enables resident repositioning, which deliberately locks hover
// expansion on pointer-down. Omit it to exercise the held-hover lifecycle.
fn draw_without_display(h: &mut H, events: Vec<egui::Event>) {
    h.t += 1.0 / 60.0;
    let size = h.app.canvas_size();
    let canvas = Rect::from_min_size(Pos2::ZERO, if h.app.settings.dock.is_side() { size } else { egui::vec2(CANVAS_W, size.y) });
    let win = Rect::from_min_size(Pos2::ZERO, size);
    let env = Env { t: h.t, unix_time: 1_794_703_260, clock: "09:41:00".into(), today: "2026-11-15".into(),
        date: "11月15日 周日".into(), media: h.media.clone(), cursor: h.cursor, screen_locked: h.screen_locked, ..Default::default() };
    let mut raw = egui::RawInput { screen_rect: Some(canvas), time: Some(h.t), events, focused: true, ..Default::default() };
    h.app.filter_todo_input(&mut raw);
    let mut frame = None;
    let output = h.ctx.run(raw, |ctx| {
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| { frame = Some(h.app.frame(ui, win, &env)); });
    });
    let frame = frame.unwrap();
    h.shapes = output.shapes.clone(); h.hit = frame.hit; h.region = frame.region;
    h.cmds.extend(frame.cmds.clone());
}

fn gear(h: &H) -> egui::Response {
    response(h, if h.app.settings.dock.is_side() { "portrait-settings" } else { "page-settings" })
}

fn point(h: &H, x: f32, y: f32) -> Pos2 {
    h.app.content_origin(Rect::from_min_size(Pos2::ZERO, h.app.canvas_size())) + egui::vec2(x, y)
}

fn view(h: &mut H, page: Option<usize>) {
    if let Some(page) = page { h.app.show_page(page); } else { h.app.show_control_center(); }
    // Remove transitions and keep the same fully expanded body for each probe.
    for _ in 0..24 { draw(h, vec![]); }
}

#[test]
fn settings_click_gear_rejects_nearby_blank_space_and_cross_boundary_clicks() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut snapped = 0;
    for dock in [Dock::Top, Dock::Left, Dock::Right] {
        for (width, height, scale) in [(292, 532, 1.0), (420, 700, 1.5), (520, 900, 2.0)] {
            let mut h = panel(dock, width, height, scale);
            for page in [None, Some(WEATHER_PAGE)] {
                view(&mut h, page);
                let rect = gear(&h).rect;
                for distance in [0.25, 1.0, 3.0, 4.5] {
                    for outside in [
                        egui::pos2(rect.left() - distance, rect.center().y),
                        egui::pos2(rect.right() + distance, rect.center().y),
                        egui::pos2(rect.center().x, rect.top() - distance),
                        egui::pos2(rect.center().x, rect.bottom() + distance),
                    ] {
                        click(&mut h, outside);
                        snapped += usize::from(gear(&h).clicked_by(egui::PointerButton::Primary));
                        assert!(!h.app.settings_open, "{dock:?}/{width}x{height}/{scale}: blank {outside:?}, gear {rect:?}");
                    }
                }
                // Both the press and release must belong to the button itself.
                let inside = egui::pos2(rect.left() + 0.5, rect.center().y);
                let outside = egui::pos2(rect.left() - 0.5, rect.center().y);
                for (start, end) in [(outside, inside), (inside, outside)] {
                    h.cursor = Some(start);
                    draw(&mut h, vec![egui::Event::PointerMoved(start)]);
                    draw(&mut h, vec![egui::Event::PointerButton { pos: start, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() }]);
                    h.cursor = Some(end);
                    draw(&mut h, vec![egui::Event::PointerMoved(end), egui::Event::PointerButton { pos: end, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() }]);
                    assert!(!h.app.settings_open, "{dock:?}: crossing gear boundary {start:?} -> {end:?}");
                }
                click(&mut h, rect.center());
                assert!(h.app.settings_open, "{dock:?}: explicit gear click");
                assert_eq!(h.app.settings_tab, page.map_or(0, |page| PAGE_SETTINGS_TABS[page]));
                h.app.settings_open = false;
                // A quick click can deliver down/up together in one frame.
                draw(&mut h, vec![egui::Event::PointerMoved(rect.center())]);
                draw(&mut h, [true, false].into_iter().map(|pressed| egui::Event::PointerButton { pos: rect.center(), button: egui::PointerButton::Primary, pressed, modifiers: Default::default() }).collect());
                assert!(h.app.settings_open, "{dock:?}: same-frame gear click");
                h.app.settings_open = false;
            }
        }
    }
    assert!(snapped > 0, "exercise egui's near-widget pointer snapping, rather than only distant misses");
}

#[test]
fn settings_click_held_from_collapsed_island_does_not_transfer_to_new_content() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    for dock in [Dock::Top, Dock::Left, Dock::Right] {
        let mut h = panel(dock, 292, 532, 1.0);
        for probe in 0..3 {
            h.app.suspend_island();
            let canvas = h.app.canvas_size();
            let win = Rect::from_min_size(Pos2::ZERO, canvas);
            let away = Pos2::new(canvas.x + 80.0, canvas.y + 80.0);
            h.cursor = Some(away);
            draw_without_display(&mut h, vec![egui::Event::PointerMoved(away)]);

            let resident = h.app.resident_rect(win);
            let offset = (probe as f32 - 1.0) * 8.0;
            let start = resident.center() + if dock.is_side() { egui::vec2(0.0, offset) } else { egui::vec2(offset, 0.0) };
            h.cursor = Some(start);
            draw_without_display(&mut h, vec![egui::Event::PointerMoved(start)]);
            draw_without_display(&mut h, vec![egui::Event::PointerButton { pos: start, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() }]);
            for _ in 0..120 { draw_without_display(&mut h, vec![]); }
            assert!(h.app.open && h.app.progress() > 0.95, "{dock:?}: held pointer should expand the panel from {start:?}");
            assert!(h.ctx.input(|input| input.pointer.primary_down()), "{dock:?}: keep the original press held through expansion");
            assert!(!h.app.settings_open, "{dock:?}: expansion while held must not open settings");

            let release = if probe == 0 { gear(&h).rect.center() } else {
                let bounds = h.app.content_rect();
                point(&h, h.app.expanded_width() * if probe == 1 { 0.3 } else { 0.7 }, bounds.center().y)
            };
            h.cursor = Some(release);
            draw_without_display(&mut h, vec![egui::Event::PointerMoved(release), egui::Event::PointerButton { pos: release, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() }]);
            assert!(!h.app.settings_open, "{dock:?}: release {start:?} -> {release:?} on newly revealed content must not open settings");

            let at = gear(&h).rect.center();
            h.cursor = Some(at);
            draw_without_display(&mut h, vec![egui::Event::PointerMoved(at)]);
            for pressed in [true, false] {
                draw_without_display(&mut h, vec![egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() }]);
            }
            assert!(h.app.settings_open, "{dock:?}: a fresh gear click after the held gesture still works");
            h.app.settings_open = false;
        }
    }
}

#[test]
fn settings_click_arbitrary_island_and_body_points_never_activate_the_gear() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    for dock in [Dock::Top, Dock::Left, Dock::Right] {
        let mut h = panel(dock, 292, 532, 1.0);
        h.app.suspend_island();
        let canvas = h.app.canvas_size();
        let win = Rect::from_min_size(Pos2::ZERO, canvas);
        let away = Pos2::new(canvas.x + 80.0, canvas.y + 80.0);
        h.cursor = Some(away);
        draw(&mut h, vec![egui::Event::PointerMoved(away)]);
        let island = h.app.resident_rect(win);
        for at in [island.center(), island.left_center(), island.right_center(), island.center_top(), island.center_bottom()] {
            click(&mut h, at);
            assert!(!h.app.settings_open, "{dock:?}: collapsed island point {at:?}");
        }

        view(&mut h, None);
        let bounds = h.app.content_rect();
        for (x, y) in [(0.08, 0.12), (0.50, 0.35), (0.50, 0.70), (0.92, 0.88)] {
            let at = point(&h, h.app.expanded_width() * x, bounds.top() + bounds.height() * y);
            if gear(&h).rect.contains(at) { continue; }
            click(&mut h, at);
            assert!(!h.app.settings_open, "{dock:?}: expanded body point {at:?}");
        }
    }
}

#[test]
fn settings_click_overview_cards_navigation_and_detail_body_do_not_open_settings() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut probes = 0;
    for dock in [Dock::Top, Dock::Left, Dock::Right] {
        for (width, height, scale) in [(292, 532, 1.0), (420, 700, 1.5), (520, 900, 2.0)] {
            let mut h = panel(dock, width, height, scale);
            // A regular grid catches oversized settings targets anywhere in the body.
            for row in 0..7 {
                for col in 0..7 {
                    view(&mut h, None);
                    let x = 12.0 + (h.app.expanded_width() - 24.0) * col as f32 / 6.0;
                    let y = 8.0 + (h.app.expanded_height() - 16.0) * row as f32 / 6.0;
                    let at = point(&h, x, y);
                    if gear(&h).rect.contains(at) { continue; }
                    click(&mut h, at);
                    assert!(!h.app.settings_open, "{dock:?}/{width}x{height}/{scale}: overview {at:?}");
                    probes += 1;
                }
            }
            for page in 0..PAGE_COUNT {
                view(&mut h, None);
                let card = response(&h, ("control-module", page));
                click(&mut h, card.rect.center());
                assert!(!h.app.is_control_center());
                assert_eq!(h.app.page(), page);
                assert!(!h.app.settings_open, "{dock:?}: module card {page}");
                view(&mut h, Some(page));
                let tab = response(&h, (if dock.is_side() { "portrait-tab" } else { "page-tab" }, page));
                click(&mut h, tab.rect.center());
                assert!(!h.app.settings_open, "{dock:?}: navigation {page}");
                for (x, y) in [(0.07, 0.04), (0.50, 0.40), (0.50, 0.70), (0.50, 0.94), (0.93, 0.55)] {
                    let at = point(&h, h.app.expanded_width() * x, h.app.expanded_height() * y);
                    click(&mut h, at);
                    assert!(!h.app.settings_open, "{dock:?}/{width}x{height}/{scale}: detail {page}, {at:?}");
                    probes += 1;
                }
            }
        }
    }
    assert!(probes >= 800);
}

#[test]
fn settings_click_empty_mail_weather_footer_and_notification_footer_are_inert() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    for dock in [Dock::Top, Dock::Left, Dock::Right] {
        let mut h = panel(dock, 292, 532, 1.0);
        view(&mut h, Some(MAIL_PAGE));
        let bounds = h.app.content_rect();
        let at = point(&h, h.app.expanded_width() / 2.0, bounds.center().y + 66.0);
        click(&mut h, at);
        assert!(!h.app.settings_open, "{dock:?}: empty mail body");
        assert!(h.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text().contains("右上角齿轮"))));

        view(&mut h, Some(WEATHER_PAGE));
        h.app.weather.picker_open = true;
        settle(&mut h);
        let footer = h.shapes.iter().find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text().contains("管理城市") => Some(text.visual_bounding_rect().center()), _ => None,
        }).expect("weather picker explains where to manage cities");
        click(&mut h, footer);
        assert!(!h.app.settings_open, "{dock:?}: city picker footer");

        view(&mut h, Some(NOTIFICATIONS_PAGE));
        let height = if dock.is_side() { h.app.expanded_height() } else { h.app.content_rect().bottom() };
        let footer_y = height - if dock.is_side() { 32.0 } else { 14.0 };
        let at = point(&h, h.app.expanded_width() / 2.0, footer_y);
        click(&mut h, at);
        assert!(!h.app.settings_open, "{dock:?}: notification footer");
        let settings = gear(&h).rect.center();
        click(&mut h, settings);
        assert!(h.app.settings_open);
        assert_eq!(h.app.settings_tab, PAGE_SETTINGS_TABS[NOTIFICATIONS_PAGE]);
    }
}

#[test]
fn settings_click_gear_stays_available_while_weather_picker_is_open() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    for dock in [Dock::Top, Dock::Left, Dock::Right] {
        let mut h = panel(dock, 292, 532, 1.5);
        view(&mut h, Some(WEATHER_PAGE));
        h.app.weather.picker_open = true;
        settle(&mut h);
        let at = gear(&h).rect.center();
        click(&mut h, at);
        assert!(h.app.settings_open, "{dock:?}: weather backdrop must leave the header gear available");
        assert_eq!(h.app.settings_tab, PAGE_SETTINGS_TABS[WEATHER_PAGE]);
    }
}

#[test]
fn settings_click_side_size_control_only_resizes_and_does_not_open_settings() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    for dock in [Dock::Left, Dock::Right] {
        let mut h = panel(dock, 340, 620, 1.5);
        let id = egui::Id::new(("card-drag", true, "portrait-size"));
        let start = h.ctx.read_response(id).unwrap().rect.center();
        click(&mut h, start);
        assert!(!h.app.settings_open, "{dock:?}: short size-control click");
        assert_eq!((h.app.settings.side_panel_width, h.app.settings.side_panel_height), (340, 620));
        h.cursor = Some(start);
        draw(&mut h, vec![egui::Event::PointerButton { pos: start, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() }]);
        let end = start + egui::vec2(if dock == Dock::Left { 38.0 } else { -38.0 }, 50.0);
        h.cursor = Some(end);
        draw(&mut h, vec![egui::Event::PointerMoved(end)]);
        draw(&mut h, vec![egui::Event::PointerButton { pos: end, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() }]);
        assert_eq!((h.app.settings.side_panel_width, h.app.settings.side_panel_height), (378, 670), "{dock:?}: size drag retains both dimensions");
        assert!(!h.app.settings_open);
    }
}

#[test]
fn settings_click_gear_keeps_keyboard_and_accessibility_activation() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    for dock in [Dock::Top, Dock::Left, Dock::Right] {
        let mut h = panel(dock, 420, 700, 2.0);
        for page in std::iter::once(None).chain((0..PAGE_COUNT).map(Some)) {
            view(&mut h, page);
            let id = gear(&h).id;
            h.cursor = None;
            draw(&mut h, vec![egui::Event::PointerGone]);
            for activation in [egui::Key::Enter, egui::Key::Space] {
                h.ctx.memory_mut(|memory| memory.request_focus(id));
                key(&mut h, activation, egui::Modifiers::NONE);
                assert!(h.app.settings_open, "{dock:?}: keyboard {activation:?}, page {page:?}");
                assert_eq!(h.app.settings_tab, page.map_or(0, |page| PAGE_SETTINGS_TABS[page]));
                h.app.settings_open = false;
            }
            #[cfg(target_os = "macos")]
            {
                h.ctx.enable_accesskit();
                draw(&mut h, vec![egui::Event::AccessKitActionRequest(egui::accesskit::ActionRequest { action: egui::accesskit::Action::Click, target: id.value().into(), data: None })]);
                assert!(h.app.settings_open, "{dock:?}: accessibility click, page {page:?}");
                assert_eq!(h.app.settings_tab, page.map_or(0, |page| PAGE_SETTINGS_TABS[page]));
                h.app.settings_open = false;
            }
        }
    }
}

#[test]
fn settings_click_hidden_page_does_not_redirect_but_external_settings_calls_still_work() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut h = panel(Dock::Top, 292, 532, 1.0);
    for page in 0..PAGE_COUNT {
        h.app.set_page_visible(page, false);
        if h.app.module_on(page) { continue; }
        h.app.settings_tab = 3;
        h.app.show_page(page);
        assert!(!h.app.settings_open, "hidden page {page}");
        assert_eq!(h.app.settings_tab, 3);
        assert_ne!(h.app.page(), page);
    }
    h.app.open_settings();
    assert!(h.app.settings_open);
    h.app.settings_open = false;
    h.app.open_mail_settings();
    assert!(h.app.settings_open);
    assert_eq!(h.app.settings_tab, PAGE_SETTINGS_TABS[MAIL_PAGE]);
    h.app.settings_open = false;
    h.app.open_settings_category(3);
    assert!(h.app.settings_open);
    assert_eq!(h.app.settings_tab, 0);
}
