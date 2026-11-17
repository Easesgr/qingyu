//! Actual egui composites, including the PNG alpha coverage at 80% island size.
use super::*;
use crate::{app::*, model::{Dock, Skin}, theme};
use super::control_center_tests::{click, draw, key, response};

fn panel(dock: Dock, reduced: bool) -> (H, crate::preview_raster::Raster) {
    let mut h = H::new();
    h.app.settings.dock = dock;
    h.app.settings.skin = Skin::Fluid;
    h.app.settings.island_size = 80;
    h.app.settings.agent_activity = false;
    h.app.settings.lyrics_enabled = false;
    h.app.settings.notification_preview = true;
    h.app.show_lyrics = false;
    h.app.panel_pinned = true;
    h.app.show_control_center();
    h.ctx.set_pixels_per_point(1.6); // Retina 2x × the user's 80% island scale.
    h.ctx.data_mut(|data| data.insert_temp(egui::Id::new("macos-accessibility-preferences"),
        (std::time::Instant::now(), if reduced { 2u32 } else { 0u32 })));
    h.app.reduced_motion = reduced;
    let mut art = egui::ColorImage::new([32, 32], egui::Color32::BLACK);
    for y in 0..32 { for x in 0..32 {
        art.pixels[y*32+x] = if (x as i32-23).pow(2)+(y as i32-9).pow(2)<30 {
            egui::Color32::from_rgb(242, 178, 92)
        } else { egui::Color32::from_rgb(26+x as u8, 82+y as u8, 118+y as u8) };
    }}
    h.media = crate::smtc::NowPlaying { source: "cloudmusic.exe".into(), title: "云间漫步".into(),
        artist: "合成示例".into(), playing: true, duration: 224.0, position: 42.0,
        artwork: Some(std::sync::Arc::new(art)), ..Default::default() };
    let size = h.app.canvas_size();
    let canvas = if dock.is_side() { size } else { Vec2::new(CANVAS_W, size.y) };
    let ppp = 1.6;
    let mut raster = crate::preview_raster::Raster::new((canvas.x*ppp).round() as usize, (canvas.y*ppp).round() as usize);
    for index in 0..85 {
        if index == 1 { h.app.show_page(MUSIC_PAGE); }
        let (output, _) = draw(&mut h, vec![]);
        raster.update_textures(&output.textures_delta);
        raster.free_textures(&output.textures_delta);
    }
    (h, raster)
}

fn tab(h: &H, page: usize) -> egui::Response {
    response(h, (if h.app.settings.dock.is_side() { "portrait-tab" }
        else if cfg!(target_os="macos") { "page-tab" } else { "dot" }, page))
}

/// Text on a light desktop, deliberately crossing the transparent bottom edge.
fn background(h: &H, raster: &mut crate::preview_raster::Raster, name: &str) {
    let dark = name == "dark";
    raster.buf.fill(if dark { [0.035, 0.055, 0.075, 1.0] } else { [0.965, 0.97, 0.98, 1.0] });
    if name != "text" { return; }
    let ppp = h.ctx.pixels_per_point();
    let size = egui::vec2(raster.w as f32/ppp, raster.h as f32/ppp);
    let output = h.ctx.run(egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
        time: Some(h.t), ..Default::default() }, |ctx| {
        let painter = ctx.layer_painter(egui::LayerId::background());
        for y in (0..size.y as usize).step_by(23) {
            painter.text(Pos2::new(5.0, y as f32), egui::Align2::LEFT_TOP,
                "桌面文档：把今天的想法留在这里，明天继续阅读。 Desk notes and a bright window behind the island.",
                theme::font(14.0, theme::W::Semibold), egui::Color32::from_rgb(45, 49, 56));
        }
    });
    raster.update_textures(&output.textures_delta);
    raster.draw(&h.ctx.tessellate(output.shapes, ppp), ppp);
    raster.free_textures(&output.textures_delta);
}

fn save(h: &H, raster: &crate::preview_raster::Raster, dest: &std::path::Path, name: &str) {
    let mut rgba = Vec::new(); raster.to_rgba8(&mut rgba);
    let image = image::RgbaImage::from_raw(raster.w as u32, raster.h as u32, rgba).unwrap();
    let win = Rect::from_min_size(Pos2::ZERO, h.app.canvas_size());
    let body = h.app.displayed_island_rect(win).union(h.app.resident_rect(win)).expand(10.0)
        .intersect(Rect::from_min_size(Pos2::ZERO, Vec2::new(raster.w as f32, raster.h as f32)/h.ctx.pixels_per_point()));
    let ppp = h.ctx.pixels_per_point();
    image::imageops::crop_imm(&image, (body.left()*ppp).round() as u32, (body.top()*ppp).round() as u32,
        (body.width()*ppp).floor() as u32, (body.height()*ppp).floor() as u32).to_image().save(dest.join(format!("{name}.png"))).unwrap();
}

/// Remove only a foreground shape to compare its rendered core against the
/// material immediately behind it, including desktop text, hover and focus.
fn navigation_elements(h: &H, shapes: &[egui::epaint::ClippedShape]) -> Vec<(usize, String, f32)> {
    let mut elements = Vec::new();
    for page in 0..PAGE_COUNT {
        let Some(texture) = theme::glyph(&h.ctx, PAGE_GLYPHS[page]) else { continue; };
        let rect = tab(h, page).rect;
        for (index, entry) in shapes.iter().enumerate() {
            let bounds = entry.shape.visual_bounding_rect();
            if !rect.contains(bounds.center()) || !entry.clip_rect.contains_rect(bounds) { continue; }
            match &entry.shape {
                egui::Shape::Mesh(mesh) if mesh.texture_id == texture.id() =>
                    elements.push((index, format!("{} icon", PAGE_NAMES[page]), 3.0)),
                egui::Shape::Text(text) if h.app.settings.dock.is_side()
                    && text.galley.text() == match page { SHELF_PAGE=>"文件", CLIPBOARD_PAGE=>"粘贴", MAC_STATS_PAGE=>"状态", _=>PAGE_NAMES[page] } =>
                    elements.push((index, format!("{} label", PAGE_NAMES[page]), 4.5)),
                _ => {}
            }
        }
    }
    elements
}

#[test]
fn fluid_navigation_pngs_and_labels_keep_contrast_on_real_composites_at_eighty_percent() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|error|error.into_inner());
    let mut worst_icon = f32::INFINITY;
    let mut worst_label = f32::INFINITY;
    for dock in [Dock::Top, Dock::Left, Dock::Right] { for reduced in [false, true] {
        let (mut h, mut raster) = panel(dock, reduced);
        let focus = tab(&h, SHELF_PAGE); h.ctx.memory_mut(|memory|memory.request_focus(focus.id));
        for _ in 0..35 { let (output, _) = draw(&mut h, vec![]); raster.update_textures(&output.textures_delta); }
        let point = tab(&h, TODO_PAGE).rect.center(); h.cursor = Some(point);
        let (output, _) = draw(&mut h, vec![egui::Event::PointerMoved(point)]);
        raster.update_textures(&output.textures_delta);
        assert!(tab(&h, SHELF_PAGE).has_focus());
        assert!(tab(&h, TODO_PAGE).hovered());
        assert!(!h.app.is_control_center() && h.app.page() == MUSIC_PAGE);
        let ppp = output.pixels_per_point;
        let elements = navigation_elements(&h, &output.shapes);
        let icons = elements.iter().filter(|entry|entry.2 == 3.0).count();
        assert!(icons >= if dock.is_side(){3}else{7}, "{dock:?}: actual PNG navigation meshes must be measured");
        let masks = elements.iter().map(|(index, name, minimum)| {
            raster.buf.fill([0.0;4]);
            raster.draw(&h.ctx.tessellate(vec![output.shapes[*index].clone()], ppp), ppp);
            let cores = raster.buf.iter().enumerate().filter_map(|(index,pixel)| (pixel[3]>=0.9).then_some(index)).collect::<Vec<_>>();
            assert!(cores.len()>=6, "{dock:?}: {name} must have visible strokes at 80%: {} core pixels", cores.len());
            (name, minimum, cores)
        }).collect::<Vec<_>>();
        let backing = output.shapes.iter().enumerate().filter(|(index,_)|!elements.iter().any(|element|element.0==*index))
            .map(|(_,shape)|shape.clone()).collect::<Vec<_>>();
        let full = h.ctx.tessellate(output.shapes.clone(), ppp);
        let backing = h.ctx.tessellate(backing, ppp);
        for name in ["light", "dark", "text"] {
            background(&h, &mut raster, name);
            let desktop = raster.buf.clone();
            raster.draw(&backing, ppp);
            let behind = raster.buf.clone();
            raster.buf = desktop; raster.draw(&full, ppp);
            for (element, minimum, cores) in &masks {
                let contrast = cores.iter().map(|index| {
                    let foreground = raster.buf[*index]; let backdrop = behind[*index];
                    crate::glass::contrast([foreground[0],foreground[1],foreground[2]], [backdrop[0],backdrop[1],backdrop[2]])
                }).fold(f32::INFINITY, f32::min);
                if **minimum == 3.0 { worst_icon = worst_icon.min(contrast); } else { worst_label = worst_label.min(contrast); }
                assert!(contrast >= **minimum, "{dock:?}/{reduced}/{name}: {element} core contrast {contrast:.2}:1 < {minimum}:1");
            }
        }
    }}
    eprintln!("80% actual composites: worst PNG core {worst_icon:.2}:1; worst small label core {worst_label:.2}:1");
}

#[test]
fn fluid_navigation_backing_keeps_hits_keyboard_selection_and_color_artwork() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|error|error.into_inner());
    for dock in [Dock::Top, Dock::Left, Dock::Right] {
        let (mut h, mut raster) = panel(dock, true);
        let home = response(&h, "control-center-tab").rect;
        let rectangles = (0..PAGE_COUNT).map(|page|tab(&h,page).rect).collect::<Vec<_>>();
        for skin in Skin::ALL {
            h.app.settings.skin = skin;
            for _ in 0..3 { let (output, _) = draw(&mut h, vec![]); raster.update_textures(&output.textures_delta); }
            assert_eq!(response(&h, "control-center-tab").rect, home, "{dock:?}/{skin:?}: home hit target");
            assert_eq!((0..PAGE_COUNT).map(|page|tab(&h,page).rect).collect::<Vec<_>>(), rectangles, "{dock:?}/{skin:?}: navigation geometry");
        }
        h.app.settings.skin = Skin::Fluid;
        let (output, _) = draw(&mut h, vec![]); raster.update_textures(&output.textures_delta);
        background(&h, &mut raster, "light"); raster.draw(&h.ctx.tessellate(output.shapes.clone(), output.pixels_per_point), output.pixels_per_point);
        let (artwork, texture) = h.app.music_artwork.as_ref().unwrap();
        let art = output.shapes.iter().find_map(|shape| match &shape.shape {
            egui::Shape::Rect(rect) if rect.brush.as_ref().is_some_and(|brush|brush.fill_texture_id==texture.id()) => {
                assert_eq!(rect.fill, egui::Color32::WHITE, "cover must not receive a theme tint"); Some(rect.rect)
            }
            _ => None
        }).expect("colored cover in the actual music page");
        let point = art.center()*output.pixels_per_point;
        let actual = raster.buf[point.y.floor() as usize*raster.w+point.x.floor() as usize];
        let source = artwork.pixels[16*32+16];
        for (actual, expected) in actual[..3].iter().zip([source.r(),source.g(),source.b()]) {
            assert!((actual*255.0-expected as f32).abs()<3.0, "cover pixels keep their original RGB");
        }
        let pets = output.shapes.iter().filter_map(|shape| match &shape.shape {
            egui::Shape::Mesh(mesh) if mesh.texture_id!=egui::TextureId::default()
                && home.contains(mesh.calc_bounds().center()) => Some(mesh), _=>None
        }).collect::<Vec<_>>();
        assert!(!pets.is_empty(), "home contains the selected resident pet");
        assert!(pets.iter().all(|mesh|mesh.vertices.iter().all(|vertex|vertex.color==egui::Color32::WHITE)), "pet colors must not receive monochrome navigation ink");
        let point = tab(&h, SHELF_PAGE).rect.center(); click(&mut h, point);
        assert!(!h.app.is_control_center() && h.app.page()==SHELF_PAGE);
        let music = tab(&h, MUSIC_PAGE); h.ctx.memory_mut(|memory|memory.request_focus(music.id));
        key(&mut h, egui::Key::Enter, egui::Modifiers::NONE);
        assert!(!h.app.is_control_center() && h.app.page()==MUSIC_PAGE);
    }
}

#[test]
#[ignore = "Exports actual egui composites; all seeded content is synthetic"]
fn fluid_navigation_gallery() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|error|error.into_inner());
    let dest = std::path::PathBuf::from(std::env::var_os("QINGYU_FLUID_PREVIEW_DIR").expect("preview directory"));
    std::fs::create_dir_all(&dest).unwrap();
    for dock in [Dock::Top, Dock::Left] { for reduced in [false, true] {
        let (mut h, mut raster) = panel(dock, reduced);
        // Focus and pointer hover are intentionally on separate, unselected tabs.
        let focus = tab(&h, SHELF_PAGE); h.ctx.memory_mut(|memory|memory.request_focus(focus.id));
        for _ in 0..35 { let (output, _) = draw(&mut h, vec![]); raster.update_textures(&output.textures_delta); }
        let point = tab(&h, TODO_PAGE).rect.center(); h.cursor = Some(point);
        let (output, _) = draw(&mut h, vec![egui::Event::PointerMoved(point)]);
        raster.update_textures(&output.textures_delta);
        for backdrop in ["light", "dark", "text"] {
            background(&h, &mut raster, backdrop);
            raster.draw(&h.ctx.tessellate(output.shapes.clone(), output.pixels_per_point), output.pixels_per_point);
            save(&h, &raster, &dest, &format!("{}-{backdrop}-{}", if dock.is_side(){"side"}else{"top"}, if reduced{"reduced"}else{"motion"}));
        }
    }}
    std::fs::write(dest.join("README.txt"), "Actual egui renders. Retina 2x at 80% island size (1.6 physical pixels per logical point). Selected music tab, focused files tab and hovered third tab. Photos and pets retain their original texture colors. Synthetic music data and desktop text only.\n").unwrap();
}
