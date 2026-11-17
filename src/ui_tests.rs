//! 无窗口交互测试：直接向 egui 注入输入事件，验证悬停展开、切页、按钮。
use crate::app::{App, Env, Options, PH, PW, WIN_H, WIN_W};
use eframe::egui::{self, Pos2, Rect, Vec2};
use std::sync::Mutex;

pub(crate) static ENV_LOCK: Mutex<()> = Mutex::new(());
#[path="mac_stats_tests.rs"]
mod mac_stats_tests;
#[path="control_center_tests.rs"]
mod control_center_tests;
#[path="settings_click_tests.rs"]
mod settings_click_tests;
#[path="navigation_hover_tests.rs"]
mod navigation_hover_tests;
#[path="fluid_contrast_tests.rs"]
mod fluid_contrast_tests;
#[path="island_position_tests.rs"]
mod island_position_tests;
#[path="island_companion_tests.rs"]
mod island_companion_tests;
#[path="library_ui_tests.rs"]
mod library_ui_tests;
const SETTINGS_TABS:[usize;9]=[0,10,1,5,14,4,7,12,16];


#[test]
fn weather_content_tap_switches_both_ways_with_a_short_transition(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for (x,y) in [(100.0,75.0),(270.0,82.0),(314.0,178.0)] {
        let mut h=H::new();
        h.app.weather.snapshot=Some(crate::weather::Snapshot{place:"杭州".into(),temp:19.0,sunrise:Some("06:52".into()),sunset:Some("17:21".into()),..Default::default()});
        h.app.show_page(crate::app::WEATHER_PAGE);let p=H::local(x,y);h.hover(p,1.0);h.click(p);
        assert!(h.app.weather.solar_open,"天气内容区 ({x},{y}) 应可以轻点切换");
        h.hover(p,0.05);
        assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text().contains("返回天气")&&t.fallback_color.a()>0&&t.fallback_color.a()<255)),"切换时应有短暂淡入动效");
        h.hover(p,0.3);h.click(p);assert!(!h.app.weather.solar_open,"日照内容区也可以轻点返回");
        h.hover(p,0.3);
        assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()=="19°"&&t.fallback_color.a()==255)),"动效完成后应恢复清晰的天气概览");
    }
}

#[test]
fn weather_content_tap_keeps_other_controls_independent(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for (x,y,target) in [(375.0,crate::app::WEATHER_H-7.0,crate::app::WEATHER_PAGE),(394.0,18.0,crate::app::WEATHER_PAGE),(367.0,18.0,crate::app::MUSIC_PAGE),(398.0,88.0,crate::app::WEATHER_PAGE),(20.0,88.0,crate::app::WEATHER_PAGE)] {
        let mut h=H::new();
        h.media=crate::smtc::NowPlaying{source:"cloudmusic.exe".into(),title:"歌曲".into(),artist:"歌手".into(),playing:true,..Default::default()};
        h.app.weather.snapshot=Some(crate::weather::Snapshot{place:"杭州".into(),temp:19.0,..Default::default()});
        h.app.show_page(crate::app::WEATHER_PAGE);h.hover(H::local(250.0,65.0),1.0);
        if y>200.0{h.click_control("weather-source");}else if x==394.0{h.click_control("page-settings");}else if x==367.0{h.click_control("header-music");}else{
            let view=h.response("weather-view").rect;h.click(if x<100.0{view.left_center()-egui::vec2(8.0,0.0)}else{view.right_center()+egui::vec2(8.0,0.0)});
        }
        assert!(!h.app.weather.solar_open,"操作 ({x},{y}) 不应触发日照切换");
        assert_eq!(h.app.page(),target,"操作 ({x},{y}) 的页面跳转应保持不变");
        if y>200.0 {assert!(h.cmds.contains(&crate::app::Cmd::OpenUrl("https://open-meteo.com/".into())));}
        if x==394.0 {assert!(h.app.settings_open);}
    }
}

#[test]
fn weather_week_shows_all_seven_days(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.weather.snapshot=Some(crate::weather::Snapshot{place:"杭州".into(),temp:19.0,
        days:["今天","明天","周五","周六","周日","周一","周二"].into_iter().map(|label|crate::weather::Day{label:label.into(),high:23.0,low:15.0,..Default::default()}).collect(),
        at:chrono::Utc::now().timestamp(),..Default::default()});
    h.app.show_page(crate::app::WEATHER_PAGE);h.hover(H::local(250.0,65.0),1.0);
    for label in ["今天","明天","周五","周六","周日","周一","周二"] {
        assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()==label)),"七天预报缺少 {label}");
    }
}

#[test]
fn weather_week_keeps_each_forecast_date_visible(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.weather.snapshot=Some(crate::weather::parse("杭州",&serde_json::json!({"current":{"temperature_2m":19},
        "daily":{"time":["2026-11-28","2026-11-29","2026-11-30","2026-12-01","2026-12-02","2026-12-03","2026-12-04"]}})).unwrap());
    h.app.show_page(crate::app::WEATHER_PAGE);h.hover(H::local(250.0,65.0),1.0);
    for date in ["11/28","11/29","11/30","12/01","12/02","12/03","12/04"] {
        assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()==date)),"七天预报缺少日期 {date}");
    }
    for s in &h.shapes {if let egui::Shape::Text(t)=&s.shape {assert!(s.clip_rect.contains_rect(t.visual_bounding_rect()),"日期与预报文字不应裁切：{}",t.galley.text());}}
}

#[test]
fn weather_today_opens_solar_details_and_returns_to_forecast(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.weather.snapshot=Some(crate::weather::Snapshot{place:"杭州".into(),temp:19.0,sunrise:Some("06:52".into()),sunset:Some("17:21".into()),sun_times:None,..Default::default()});
    h.app.show_page(crate::app::WEATHER_PAGE);h.hover(H::local(250.0,65.0),1.0);h.click(H::local(100.0,75.0));h.hover(H::local(100.0,75.0),0.3);
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text().contains("返回天气"))),"点击当天区域后应进入日出日落视图");
    for phrase in ["日出","日落","06:52","17:21","日照进度待更新"] {
        assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text().contains(phrase))),"旧缓存日照视图缺少 {phrase}");
    }
    assert!(!h.app.weather.loading,"浏览日照视图不应额外发起网络请求");
    h.click(H::local(80.0,44.0));
    assert!(!h.app.weather.solar_open);
    h.hover(H::local(80.0,44.0),0.3);
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()=="日出 06:52")),"返回后应显示天气视图的日照入口");
}

#[test]
fn weather_side_bar_shows_a_clickable_temperature_on_both_edges(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for dock in [crate::model::Dock::Left,crate::model::Dock::Right] {
        let mut h=H::new();h.app.settings.dock=dock;
        h.app.weather.snapshot=Some(crate::weather::Snapshot{place:"杭州".into(),temp:-12.0,code:0,day:true,..Default::default()});
        for _ in 0..60{side_frame(&mut h,Some(Pos2::new(250.0,580.0)));}
        let response=h.ctx.read_response(egui::Id::new(("card",true,"weather-side"))).expect("两侧都应提供常驻天气入口");
        let temp=h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t)if t.galley.text()=="12°"=>Some((s.clip_rect,t.visual_bounding_rect())),_=>None}).expect("负温度应完整显示");
        assert!(temp.0.contains_rect(temp.1),"{dock:?} 负温度不应裁切");
        let click=response.rect.center();
        side_frame(&mut h,Some(click));
        for pressed in [true,false] {
            h.t+=1.0/60.0;
            let env=Env{t:h.t,clock:"09:41:00".into(),cursor:Some(click),..Default::default()};
            let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(500.0,600.0))),time:Some(h.t),focused:true,
                events:vec![egui::Event::PointerButton{pos:click,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()}],..Default::default()};
            let _=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::new()).show(ctx,|ui|{h.app.frame(ui,ui.max_rect(),&env);});});
        }
        assert_eq!(h.app.page(),crate::app::WEATHER_PAGE,"{dock:?} 点击侧边温度应进入天气页");
        assert!(!h.app.weather.solar_open);
        h.app.settings.visible_pages[crate::app::WEATHER_PAGE]=false;
        assert!(!h.app.side_items().contains(&crate::side_dock::SideItem::Weather));
        h.app.settings.visible_pages[crate::app::WEATHER_PAGE]=true;h.app.settings.minimal=true;
        #[cfg(target_os="macos")]
        {assert!(h.app.side_items().contains(&crate::side_dock::SideItem::Weather),"top minimal preference does not override visible side mode");h.app.settings.macos_side_handle=true;}
        assert!(!h.app.side_items().contains(&crate::side_dock::SideItem::Weather));
    }
}

#[test]
fn weather_solar_view_preserves_readable_times_across_skins_and_scales(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    use chrono::TimeZone;
    let now=chrono_tz::Asia::Shanghai.with_ymd_and_hms(2026,10,28,9,41,0).single().unwrap().with_timezone(&chrono::Local);
    for skin in crate::model::Skin::ALL {for scale in [1.0,1.25,1.5] {
        let mut h=H::new();h.app.settings.skin=skin;h.ctx.set_pixels_per_point(scale);
        h.app.weather.snapshot=Some(crate::weather::parse("杭州",&serde_json::json!({"timezone":"Asia/Shanghai","current":{"temperature_2m":19},
            "daily":{"time":["2026-10-28"],"sunrise":["2026-10-28T06:52"],"sunset":["2026-10-28T17:21"]}})).unwrap());
        h.app.weather.solar_open=true;h.app.show_page(crate::app::WEATHER_PAGE);
        for _ in 0..60 {
            h.t+=1.0/60.0;
            let cursor=H::local(180.0,185.0);
            let env=Env{t:h.t,clock:now.format("%H:%M:%S").to_string(),today:now.format("%Y-%m-%d").to_string(),date:now.format("%m月%d日").to_string(),cursor:Some(cursor),..Default::default()};
            let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(crate::app::CANVAS_W,WIN_H))),time:Some(h.t),focused:true,
                events:vec![egui::Event::PointerMoved(cursor)],..Default::default()};
            h.shapes=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::new()).show(ctx,|ui|{
                h.app.frame(ui,Rect::from_min_size(ui.max_rect().min,Vec2::new(WIN_W,WIN_H)),&env);
            });}).shapes;
        }
        for phrase in ["返回天气","距离日落","07:40","白昼中点","12:06","06:52","17:21","白昼 10小时29分"] {
            assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text().contains(phrase))),"{skin:?} / {scale} 日照视图缺少 {phrase}");
        }
        for s in &h.shapes {if let egui::Shape::Text(t)=&s.shape {assert!(s.clip_rect.contains_rect(t.visual_bounding_rect()),"日照文字不应裁切：{}",t.galley.text());}}
    }}
}

#[test]
#[ignore="按需导出七日预报、日照和两侧常驻天气的实际画面"]
fn weather_foundation_preview(){
    use crate::model::{Dock,Skin};
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let dest=std::path::PathBuf::from(std::env::var_os("QINGYU_WEATHER_PREVIEW_DIR").expect("需指定预览目录"));std::fs::create_dir_all(&dest).unwrap();
    for (name,dock,solar,clock,skin) in [
        ("weather-week",Dock::Top,false,"14:08:00",Skin::ALL[0]),
        ("weather-sun",Dock::Top,true,"14:08:00",Skin::ALL[0]),
        ("weather-night",Dock::Top,true,"20:08:00",Skin::ALL[0]),
        ("weather-side-left",Dock::Left,false,"14:08:00",Skin::ALL[0]),
        ("weather-side-right",Dock::Right,false,"14:08:00",Skin::ALL[0]),
        ("weather-week-cream",Dock::Top,false,"14:08:00",Skin::Cream),
        ("weather-sun-cream",Dock::Top,true,"14:08:00",Skin::Cream),
        ("weather-week-glass",Dock::Top,false,"14:08:00",Skin::GlassDark),
        ("weather-sun-glass",Dock::Top,true,"14:08:00",Skin::GlassDark),
        ("weather-city-picker",Dock::Top,false,"14:08:00",Skin::Classic),
    ] {
        let mut h=H::new();h.app.settings.dock=dock;h.app.settings.skin=skin;
        let mut snapshot=crate::weather::parse("杭州",&serde_json::json!({"timezone":"Asia/Shanghai",
            "current":{"temperature_2m":19,"apparent_temperature":18,"relative_humidity_2m":58,"wind_speed_10m":12,"weather_code":2,"is_day":1},
            "daily":{"time":["2026-11-11","2026-11-12","2026-11-13","2026-11-14","2026-11-15","2026-11-16","2026-11-17"],
                "weather_code":[2,0,3,61,80,2,0],"temperature_2m_max":[23,24,22,19,18,20,22],"temperature_2m_min":[15,16,14,13,12,12,14],
                "precipitation_probability_max":[10,0,20,70,55,10,0],"sunrise":["2026-11-11T06:52"],"sunset":["2026-11-11T17:21"]}})).unwrap();
        if clock=="20:08:00" {snapshot.day=false;}
        h.app.weather.snapshot=Some(snapshot);
        if name=="weather-city-picker" {
            let city=|label:&str,region:&str,lat:f64|crate::weather::Position{lat,lon:120.0,label:label.into(),region:region.into()};
            h.app.settings.weather_saved=vec![city("杭州","浙江 · 中国",30.27),city("上海","上海 · 中国",31.23),city("哈尔滨","黑龙江 · 中国",45.75)];
            h.app.settings.weather_city="杭州".into();h.app.settings.weather_position=Some(h.app.settings.weather_saved[0].clone());
            h.app.weather.snapshot.as_mut().unwrap().position=h.app.settings.weather_position.clone();
            h.app.weather.picker_open=true;
        }
        if dock==Dock::Top {h.app.weather.solar_open=solar;h.app.show_page(crate::app::WEATHER_PAGE);}
        let (w,hgt)=if dock==Dock::Top {(crate::app::CANVAS_W,WIN_H)}else{(crate::side_dock::SIDE_W,crate::side_dock::SIDE_H)};
        let mut raster=feature_raster_sized(w,hgt);
        let frames=90;
        for frame in 0..frames {
            h.t+=1.0/60.0;
            let cursor=Some(if dock==Dock::Top{H::local(180.0,185.0)}else{Pos2::new(250.0,580.0)});
            let env=Env{t:h.t,clock:clock.into(),today:"2026-11-11".into(),date:"11月11日 周三".into(),cursor,battery:Some(76),..Default::default()};
            let events=vec![egui::Event::PointerMoved(cursor.unwrap())];
            let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(w,hgt))),time:Some(h.t),focused:true,
                events,viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(2.0),..Default::default()})).collect(),..Default::default()};
            let output=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::new()).show(ctx,|ui|{
                h.app.frame(ui,Rect::from_min_size(ui.max_rect().min,Vec2::new(if dock==Dock::Top{WIN_W}else{w},hgt)),&env);
            });});
            raster.update_textures(&output.textures_delta);
            if frame==frames-1 {
                raster.buf.fill([0.20,0.24,0.32,1.0]);raster.draw(&h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);
                let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
                let image=image::RgbaImage::from_raw((w*2.0)as u32,(hgt*2.0)as u32,rgba).unwrap();
                let (x,y,crop_w,crop_h)=if dock==Dock::Top {(200,0,880,(h.app.expanded_height()*2.0+24.0)as u32)}else{
                    let rect=h.app.island_rect(Rect::from_min_size(Pos2::ZERO,Vec2::new(w,hgt)),h.app.w.pos,h.app.h.pos);
                    (if dock==Dock::Left{0}else{880},((rect.top()-12.0)*2.0)as u32,120,((rect.height()+24.0)*2.0)as u32)
                };
                let file=format!("{name}.png");
                image::imageops::crop_imm(&image,x,y,crop_w,crop_h).to_image().save(dest.join(file)).unwrap();
            }
            raster.free_textures(&output.textures_delta);
        }
    }
}

#[test]
#[ignore="按需导出进入日照视图时的涟漪与太阳滑入过程"]
fn weather_solar_entry_preview(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let dest=std::path::PathBuf::from(std::env::var_os("QINGYU_WEATHER_PREVIEW_DIR").expect("需指定预览目录"));std::fs::create_dir_all(&dest).unwrap();
    let mut h=H::new();
    h.app.weather.snapshot=Some(crate::weather::parse("杭州",&serde_json::json!({"timezone":"Asia/Shanghai",
        "current":{"temperature_2m":19,"apparent_temperature":18,"relative_humidity_2m":58,"wind_speed_10m":12,"weather_code":2,"is_day":1},
        "daily":{"time":["2026-11-11"],"weather_code":[2],"temperature_2m_max":[23],"temperature_2m_min":[15],
            "precipitation_probability_max":[10],"sunrise":["2026-11-11T06:52"],"sunset":["2026-11-11T17:21"]}})).unwrap());
    h.app.show_page(crate::app::WEATHER_PAGE);
    let (w,hgt)=(crate::app::CANVAS_W,WIN_H);
    let mut raster=feature_raster_sized(w,hgt);
    let click=H::local(330.0,94.0);
    // 第 60 帧按下、61 帧松开；之后按时间点导出。
    let shots=[(70,"weather-entry-a"),(85,"weather-entry-b"),(110,"weather-entry-c"),(160,"weather-entry-d")];
    for frame in 0..161 {
        h.t+=1.0/60.0;
        let env=Env{t:h.t,clock:"14:08:00".into(),today:"2026-11-11".into(),date:"11月11日 周三".into(),cursor:Some(click),battery:Some(76),..Default::default()};
        let mut events=vec![egui::Event::PointerMoved(click)];
        if frame==60||frame==61 {events.push(egui::Event::PointerButton{pos:click,button:egui::PointerButton::Primary,pressed:frame==60,modifiers:Default::default()});}
        let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(w,hgt))),time:Some(h.t),focused:true,events,
            viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(2.0),..Default::default()})).collect(),..Default::default()};
        let output=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::new()).show(ctx,|ui|{
            h.app.frame(ui,Rect::from_min_size(ui.max_rect().min,Vec2::new(WIN_W,hgt)),&env);
        });});
        raster.update_textures(&output.textures_delta);
        if let Some((_,name))=shots.iter().find(|(at,_)|*at==frame) {
            raster.buf.fill([0.20,0.24,0.32,1.0]);raster.draw(&h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);
            let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
            let image=image::RgbaImage::from_raw((w*2.0)as u32,(hgt*2.0)as u32,rgba).unwrap();
            image::imageops::crop_imm(&image,200,0,880,(h.app.expanded_height()*2.0+24.0)as u32).to_image().save(dest.join(format!("{name}.png"))).unwrap();
        }
        raster.free_textures(&output.textures_delta);
    }
    assert!(h.app.weather.solar_open,"点击天气视图后应进入日照视图");
}

#[test]
fn island_meshes_stay_valid_after_long_uptime_and_repeated_hover(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.app.page=0;h.power=(Some(100),true);
    let mut frames=0;
    for days in [0.0,1.0,3.0,7.0,30.0] {
        h.t=days*86400.0;
        for skin in crate::model::Skin::ALL {
            h.app.settings.skin=skin;
            for scale in [1.0,1.25,1.5] {
                h.ctx.set_pixels_per_point(scale);
                for frame in 0..120 {
                    let point=if frame%60<30{H::local(200.0,15.0)}else{Pos2::new(900.0,250.0)};
                    h.cursor=Some(point);h.frame(vec![egui::Event::PointerMoved(point)]);
                    let meshes=h.ctx.tessellate(h.shapes.clone(),h.ctx.pixels_per_point());
                    for primitive in meshes {if let egui::epaint::Primitive::Mesh(mesh)=primitive.primitive {
                        assert!(mesh.is_valid(),"运行 {days} 天后的网格索引必须有效");
                        assert!(mesh.vertices.iter().all(|v|v.pos.is_finite()&&v.uv.is_finite()),"运行 {days} 天后展开不能出现非有限坐标");
                        assert!(mesh.vertices.iter().all(|v|v.pos.x.abs()<10000.0&&v.pos.y.abs()<10000.0),"运行 {days} 天后顶点不能拉到窗口远处");
                    }}
                    frames+=1;
                }
            }
        }
    }
    eprintln!("长运行绘制检查：{frames} 帧；覆盖 0、1、3、7、30 天及全部皮肤、三种缩放。");
}


#[test]
fn weather_hidden_page_does_not_open_settings(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.set_page_visible(crate::app::WEATHER_PAGE,false);h.app.show_page(crate::app::WEATHER_PAGE);
    assert!(!h.app.settings_open);assert_eq!(h.app.settings_tab,0);
}

#[test]
fn weather_settings_maps_to_the_module_switch(){
    assert_eq!(crate::extras::settings_tab_module(12),Some(crate::app::WEATHER_PAGE));
}

#[test]
fn weather_city_draft_keeps_the_saved_location_until_confirmed(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.settings.weather_city="杭州".into();h.app.settings.clock_12h=true;
    h.app.settings.save();h.app.weather.city_input="北京".into();
    h.frame(vec![]);
    assert_eq!(h.app.settings.weather_city,"杭州");
    let saved=crate::model::Settings::load();
    assert_eq!(saved.weather_city,"杭州");
    assert!(saved.clock_12h,"未确认的城市编辑不能改变其他设置");
    h.app.set_page_visible(crate::app::WEATHER_PAGE,false);h.app.add_weather_city();
    assert_eq!(h.app.settings.weather_city,"杭州");assert!(!h.app.weather.loading);
}

#[test]
fn weather_cities_switch_from_the_island_title_and_keep_their_own_cache(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    let hz=crate::weather::Position{lat:30.27,lon:120.15,label:"杭州".into(),region:"浙江 · 中国".into()};
    let bj=crate::weather::Position{lat:39.90,lon:116.40,label:"北京".into(),region:"北京 · 中国".into()};
    // 设置页添加两个城市：只加入列表，搜索框清空。
    for city in [&hz,&bj] {
        h.app.weather.city_input=city.label.clone();h.app.weather.selected_city=Some(city.clone());
        assert!(h.app.add_weather_city());
        assert!(h.app.settings.weather_city.is_empty(),"添加城市只加入列表，不切换当前城市");
        assert!(h.app.weather.city_input.is_empty());
    }
    assert_eq!(h.app.settings.weather_saved,vec![hz.clone(),bj.clone()]);
    h.app.weather.city_input=hz.label.clone();h.app.weather.selected_city=Some(hz.clone());
    assert!(!h.app.add_weather_city(),"重复城市不再添加");
    assert_eq!(crate::model::Settings::load().weather_saved,vec![hz.clone(),bj.clone()],"城市列表应保存到设置");
    // 打开天气页，点标题展开下拉框，选「杭州」。
    h.app.weather.snapshot=Some(crate::weather::Snapshot{place:"北京".into(),position:Some(bj.clone()),temp:-3.0,code:71,day:true,at:chrono::Utc::now().timestamp(),..Default::default()});
    h.app.show_page(crate::app::WEATHER_PAGE);h.hover(H::local(250.0,65.0),1.0);
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()=="当地")),"标题保持「当地天气」");
    h.click(H::local(115.0,112.0));h.hover(H::local(330.0,200.0),0.4);
    assert!(h.app.weather.picker_open,"点地点行展开城市下拉");assert!(!h.app.weather.solar_open,"点地点行不应切到日照");
    for phrase in ["自动定位","杭州","北京","在右上角齿轮中管理城市"] {
        assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text().contains(phrase))),"下拉框缺少 {phrase}");
    }
    let row=h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t)if t.galley.text()=="杭州"=>Some(t.visual_bounding_rect().center()),_=>None}).unwrap();
    h.click(row);h.hover(row,0.3);
    assert_eq!(h.app.settings.weather_city,"杭州");assert!(!h.app.weather.picker_open);assert!(!h.app.weather.solar_open);
    assert!(h.app.weather.snapshot.is_none(),"杭州没有缓存时不能显示北京的天气");
    // 点面板外收起，不触发日照视图。
    h.click(H::local(209.0,147.0));h.hover(H::local(330.0,200.0),0.4);assert!(h.app.weather.picker_open,"无数据时入口在页面中下方");
    h.click(H::local(330.0,200.0));h.hover(H::local(330.0,200.0),0.3);
    assert!(!h.app.weather.picker_open);assert!(!h.app.weather.solar_open,"点外面只收起下拉框");
    // 删除当前城市：回到自动定位。
    h.app.remove_weather_city(&hz);
    assert_eq!(h.app.settings.weather_saved,vec![bj.clone()]);assert!(h.app.settings.weather_city.is_empty());assert!(h.app.settings.weather_position.is_none());
}

#[test]
fn weather_negative_temperature_draws_a_hyphen_length_minus_on_the_digit_midline(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.weather.snapshot=Some(crate::weather::Snapshot{place:"哈尔滨".into(),temp:-6.0,code:71,day:true,at:chrono::Utc::now().timestamp(),..Default::default()});
    for _ in 0..60{h.frame(vec![]);}
    let digits=h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t)if t.galley.text()=="6°"=>Some(t.visual_bounding_rect()),_=>None}).expect("胶囊应显示 6°");
    let bar=h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Rect(r) if (digits.left()-r.rect.right()).abs()<4.0 && r.rect.right()<digits.center().x && r.rect.width()>r.rect.height()*1.5 && r.rect.height()<4.0=>Some(r.rect),_=>None}).expect("6° 左侧应有手绘减号");
    assert!(!h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text().contains('−'))),"不再使用过长的排版减号");
    assert!(bar.width()<digits.height()*0.6,"减号长度与连字符相当");
    assert!((bar.center().y-digits.center().y).abs()<digits.height()*0.2,"减号在数字中线上");
}

#[test]
fn weather_city_dropdown_selects_the_region_without_applying_the_draft(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    let correct=crate::weather::Position{lat:23.02677,lon:113.13148,label:"佛山市".into(),region:"广东 · 中国".into()};
    let wrong=crate::weather::Position{lat:28.73401,lon:98.80612,label:"佛山".into(),region:"迪庆藏族自治州 · 云南 · 中国".into()};
    h.app.settings.weather_city="杭州".into();h.app.weather.manual_city=true;h.app.weather.city_input="佛山".into();
    h.app.weather.cities.seed("佛山",vec![correct.clone(),wrong]);
    fn draw(h:&mut H,events:Vec<egui::Event>){
        h.t+=1.0/60.0;h.app.t=h.t;
        let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(560.0,380.0))),time:Some(h.t),events,..Default::default()};
        h.shapes=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().show(ctx,|ui|{h.app.weather_city_picker(ui);});}).shapes;
    }
    for _ in 0..3 {draw(&mut h,vec![]);}
    h.ctx.memory_mut(|m|m.open_popup(egui::Id::new("weather-city-options")));
    for _ in 0..3 {draw(&mut h,vec![]);}
    let point=h.shapes.iter().find_map(|s|match &s.shape {egui::Shape::Text(text) if text.galley.text()=="佛山市"=>Some(text.visual_bounding_rect().center()),_=>None}).expect("下拉框必须显示完整城市名");
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()=="广东 · 中国")),"下拉框必须显示省份");
    draw(&mut h,vec![egui::Event::PointerMoved(point),egui::Event::PointerButton{pos:point,button:egui::PointerButton::Primary,pressed:true,modifiers:Default::default()}]);
    draw(&mut h,vec![egui::Event::PointerButton{pos:point,button:egui::PointerButton::Primary,pressed:false,modifiers:Default::default()}]);
    assert_eq!(h.app.weather.selected_city.as_ref(),Some(&correct));assert_eq!(h.app.weather.draft_city(),Some("佛山市"));
    assert_eq!(h.app.settings.weather_city,"杭州","选择候选后仍需应用确认");assert!(!h.app.weather.loading);
    assert!(!h.ctx.memory(|m|m.is_popup_open(egui::Id::new("weather-city-options"))));
    h.app.settings.weather_city="佛山市".into();h.app.settings.weather_position=Some(correct.clone());h.app.settings.save();
    let saved=crate::model::Settings::load();assert_eq!(saved.weather_position,Some(correct));
    h.app.weather.city_input="伦敦".into();assert_eq!(h.app.weather.draft_city(),None,"改写输入后必须重新选择地区");
}

#[test]
fn weather_attribution_opens_its_source_without_changing_pages(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.show_page(crate::app::WEATHER_PAGE);h.hover(H::local(300.0,60.0),1.0);
    h.click(H::local(375.0,crate::app::WEATHER_H-7.0));
    assert!(h.cmds.contains(&crate::app::Cmd::OpenUrl("https://open-meteo.com/".into())));
    assert_eq!(h.app.page(),crate::app::WEATHER_PAGE);
}

#[test]
fn weather_refresh_keeps_music_and_settings_shortcuts_separate(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.media=crate::smtc::NowPlaying{source:"cloudmusic.exe".into(),title:"歌曲".into(),artist:"歌手".into(),playing:true,..Default::default()};
    h.app.show_page(crate::app::WEATHER_PAGE);h.hover(H::local(300.0,60.0),1.0);
    let rect=|name|h.ctx.read_response(egui::Id::new(("card",true,name))).unwrap().rect;
    let refresh=rect("weather-refresh");
    assert!(!refresh.intersects(rect("header-music")));
    assert!(!refresh.intersects(rect("page-settings")));
}

#[test]
fn resized_font_atlases_stop_reuploading_and_release_unused_sizes(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let ctx=egui::Context::default();crate::theme::load_fonts(&ctx);
    ctx.set_embed_viewports(false);
    let child=egui::ViewportId::from_hash_of("font-upload-settings");
    let children=std::sync::Arc::new(Mutex::new(Vec::new()));let capture=children.clone();
    egui::Context::set_immediate_viewport_renderer(move|ctx,mut viewport|{
        let id=viewport.ids.this;
        let input=egui::RawInput{viewport_id:id,screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(640.0,270.0))),max_texture_side:Some(2048),
            viewports:std::iter::once((id,egui::ViewportInfo{native_pixels_per_point:Some(1.0),..Default::default()})).collect(),..Default::default()};
        capture.lock().unwrap().push(ctx.run(input,|ctx|(viewport.viewport_ui_cb)(ctx)));
    });
    let frame=|ppp:f32,settings:bool|{
        let mut viewports=std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(ppp),..Default::default()})).collect::<egui::ViewportIdMap<_>>();
        if settings{viewports.insert(child,egui::ViewportInfo{native_pixels_per_point:Some(1.0),..Default::default()});}
        let input=egui::RawInput{viewports,screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(640.0,270.0))),max_texture_side:Some(2048),..Default::default()};
        let draw=|ctx:&egui::Context|{egui::CentralPanel::default().show(ctx,|ui|{ui.label("专注时间 25:00 · 休息 · 音乐与歌词");});};
        let root=ctx.run(input,|ctx|{draw(ctx);if settings{ctx.show_viewport_immediate(child,Default::default(),|ctx,_|draw(ctx));}});
        let mut outputs=std::mem::take(&mut *children.lock().unwrap());outputs.push(root);
        outputs
    };
    for settings in [false,true,false] {
        for ppp in [1.0,1.4,1.2,0.8,1.4,1.0] {
            for _ in 0..3{frame(ppp,settings);}
            let start=std::time::Instant::now();let mut bytes=0usize;
            for _ in 0..60 {for output in frame(ppp,settings){
                bytes+=output.textures_delta.set.iter().map(|(_,delta)|delta.image.width()*delta.image.height()*4).sum::<usize>();
            }}
            println!("缩放={ppp:.2} 设置={settings} 60帧上传={bytes}字节 耗时={:?}",start.elapsed());
            assert_eq!(bytes,0,"大小稳定后不能重复上传字形图集，包括设置同时打开时");
            let textures=ctx.tex_manager();let textures=textures.read();
            let fonts=textures.allocated().filter(|(_,meta)|meta.name.starts_with("egui_font_texture")).count();
            assert!(fonts<=if settings&&ppp!=1.0{2}else{1},"旧倍率的字体贴图应释放：{fonts}");
        }
    }
}


#[test]
fn enlarged_island_preserves_font_resolution_with_a_separate_settings_window(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.show_page(crate::app::MUSIC_PAGE);h.hover(H::local(200.0,60.0),1.0);
    h.app.open_settings();h.app.settings_tab=1;h.ctx.set_embed_viewports(false);
    let captured=std::sync::Arc::new(Mutex::new(None));let capture=captured.clone();
    egui::Context::set_immediate_viewport_renderer(move|ctx,mut viewport|{
        let id=viewport.ids.this;
        let input=egui::RawInput{viewport_id:id,screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(690.0,560.0))),
            viewports:std::iter::once((id,egui::ViewportInfo{native_pixels_per_point:Some(1.0),..Default::default()})).collect(),..Default::default()};
        *capture.lock().unwrap()=Some(ctx.run(input,|ctx|(viewport.viewport_ui_cb)(ctx)));
    });
    let mut raster=crate::preview_raster::Raster::new(WIN_W as usize,WIN_H as usize);
    raster.update_textures(&egui::TexturesDelta{set:vec![(egui::TextureId::default(),egui::epaint::ImageDelta::full(h.ctx.fonts(|fonts|fonts.image()),egui::TextureOptions::LINEAR))],..Default::default()});
    for scale in [1.0,1.2,1.4] {
        raster.w=(WIN_W*scale) as usize;raster.h=(WIN_H*scale) as usize;raster.buf.resize(raster.w*raster.h,[0.0;4]);
        for frame in 0..3 {
            h.t+=1.0/60.0;
            let input=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(crate::app::CANVAS_W,WIN_H))),
                time:Some(h.t),events:vec![egui::Event::PointerMoved(H::local(70.0,40.0))],viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(scale),..Default::default()})).collect(),..Default::default()};
            let root=h.ctx.run(input,|ctx|{
                egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx,|ui|{
                    let win=Rect::from_min_size(ui.max_rect().min,Vec2::new(WIN_W,WIN_H));
                    h.app.frame(ui,win,&Env{t:h.t,cursor:Some(H::local(70.0,40.0)),clock:"09:41:00".into(),..Default::default()});
                });
                h.app.settings_ui(ctx,false);
            });
            let child=captured.lock().unwrap().take().unwrap();
            assert_eq!(child.pixels_per_point,1.0,"设置窗口不跟随岛体变大");
            for (shapes,ppp) in [(&root.shapes,scale),(&child.shapes,1.0)] {
                for shape in shapes {if let egui::Shape::Text(text)=&shape.shape {
                    assert!((text.galley.pixels_per_point-ppp).abs()<0.0001,"各窗口应使用各自物理尺寸的字形");
                }}
            }
            raster.update_textures(&child.textures_delta);raster.update_textures(&root.textures_delta);
            if frame==2 {if let Some(dest)=std::env::var_os("QINGYU_FOCUS_SCALE_PREVIEW") {
                let dest=std::path::PathBuf::from(dest);std::fs::create_dir_all(&dest).unwrap();
                raster.buf.fill([0.18,0.20,0.24,1.0]);raster.draw(&h.ctx.tessellate(root.shapes,root.pixels_per_point),root.pixels_per_point);
                let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
                image::save_buffer(dest.join(format!("focus-size-{}.png",(scale*100.0) as u32)),&rgba,(WIN_W*scale) as u32,(WIN_H*scale) as u32,image::ColorType::Rgba8).unwrap();
            }}
            raster.free_textures(&root.textures_delta);
            raster.free_textures(&child.textures_delta);
        }
    }
}

#[test]
fn opaque_surface_gets_a_contour_with_every_skin_and_dock(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for skin in crate::model::Skin::ALL {for dock in crate::model::Dock::ALL {for island_shape in crate::model::IslandShape::ALL {for opaque_surface in [false,true] {
        let mut h=H::new();h.app.settings.skin=skin;h.app.settings.dock=dock;h.app.settings.island_shape=island_shape;h.app.settings.glass_blur=false;
        let size=if dock.is_side(){Vec2::new(crate::side_dock::SIDE_W,crate::side_dock::SIDE_H)}else{Vec2::new(WIN_W,WIN_H)};
        let mut outlined=false;
        let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,size)),..Default::default()};
        let _=h.ctx.run(raw,|ctx|{
            egui::CentralPanel::default().show(ctx,|ui|{
                let win=Rect::from_min_size(ui.max_rect().min,size);
                let out=h.app.frame(ui,win,&Env{opaque_surface,..Default::default()});
                outlined=out.glass.is_some_and(|shapes|!shapes.is_empty());
            });
        });
        assert_eq!(outlined,opaque_surface,"皮肤={skin:?}，停靠={dock:?}，形状={island_shape:?}");
    }}}}
}

#[test]
fn changing_qq_metadata_cannot_spawn_lyrics_workers_while_one_is_running(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    let (release,wait)=std::sync::mpsc::channel();
    let worker=std::thread::spawn(move||{let _=wait.recv();});
    let id=worker.thread().id();h.app.ly_worker=Some(worker);
    h.app.opts.fetch_lyrics=true;h.app.settings.island_lyrics=true;
    for step in 0..1440 {
        h.media=crate::smtc::NowPlaying{source:"QQMusic.exe".into(),title:format!("歌曲{}",step%2),artist:"歌手".into(),duration:120.0,playing:true,..Default::default()};
        h.t=step as f64*0.25;h.frame(vec![]);
        assert_eq!(h.app.ly_worker.as_ref().unwrap().thread().id(),id,"旧请求未结束时不得另开取词线程");
        assert!(h.app.ly.rx.is_none(),"不能为抖动的歌曲堆积请求");
    }
    release.send(()).unwrap();h.app.ly_worker.take().unwrap().join().unwrap();
}

#[test]
fn v0649_settings_and_mail_accounts_survive_upgrade(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let _h=H::new();
    let directory=crate::model::config_dir();
    std::fs::create_dir_all(&directory).unwrap();
    let legacy=concat!("settings_theme=dark\npet=0\nminimal=1\nbrowser_minimal=1\n",
        "visible_pages=10111111\npage_order=5,2,0,1,3,4,6,7\nshow_battery=0\n",
        "system_hud=0\nread_notifications=1\nnotifications=0\nsound=1\n",
        "lyrics_enabled=0\nisland_lyrics=1\nlyric_delay_ms=-650\nhide_fullscreen=0\n",
        "focus_minutes=45\nrest_minutes=10\nwater_minutes=60\nserving_ml=300\nwater_start=8\nwater_end=23\n",
        "mail_enabled=0\nmail_code=0\nmail_interval=300\nmail_ics=30\ncalendar_prompt=0\ncalendar_auto_sync=0\n");
    std::fs::write(directory.join("settings.txt"),legacy).unwrap();
    let upgraded=crate::model::Settings::load();
    assert!(upgraded.display_id.is_empty() && upgraded.avoid_apps.is_empty());
    assert!(upgraded.avoid_browsers && !upgraded.bluetooth_notifications);
    assert!(!upgraded.clock_12h && !upgraded.notification_preview);
    assert_eq!(upgraded.clock_period,0);assert_eq!(upgraded.mail_code_hide_seconds,60);
    for _ in 0..2 {
        crate::model::Settings::load().save();
        let saved=std::fs::read_to_string(directory.join("settings.txt")).unwrap();
        for line in legacy.lines().filter(|line|!["visible_pages=","page_order=","pet=","show_battery=","system_hud=","read_notifications=","focus_minutes=","rest_minutes=","water_minutes=","serving_ml=","water_start=","water_end=","mail_ics=","calendar_prompt=","calendar_auto_sync="].iter().any(|key|line.starts_with(key))){assert!(saved.lines().any(|value|value==line),"升级保存不能改变旧设置：{line}");}
        assert!(saved.lines().any(|value|value=="visible_pages=111111111"),"旧页面开关应保留，并默认开启天气页");
        assert!(saved.lines().any(|value|value=="page_order=3,0,1,6,2,4,5,7,8"),"旧顺序应保留，并把天气页接在最后");
    }
    std::fs::write(directory.join("mail-accounts.json"),br#"[{"email":"OldUser@qq.com","provider":"qq","enabled":true},{"email":"old@icloud.com","provider":"icloud","enabled":false}]"#).unwrap();
    let accounts=crate::mail::load_accounts();
    assert_eq!(accounts.len(),2);assert!(accounts.iter().all(|account|account.custom.is_none()));
    assert_eq!(accounts[0].credential_key(),"olduser@qq.com");
    assert_eq!(accounts[0].endpoint().unwrap(),("imap.qq.com".into(),993,"OldUser@qq.com".into()));
    assert!(!accounts[1].enabled);
    crate::mail::save_accounts(&accounts);assert_eq!(crate::mail::load_accounts(),accounts);
}


#[test]
fn switching_music_source_discards_previous_lyrics() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    h.media=crate::smtc::NowPlaying{source:"cloudmusic.exe".into(),title:"Track".into(),artist:"Artist".into(),duration:120.0,..Default::default()};
    h.frame(vec![]);
    for source in ["QQMusic.exe","cloudmusic.exe","汽水音乐","KuGou.exe","QQMusic.exe"] {
        h.app.set_lyrics("Track","Artist",120.0,crate::lyrics::parse_lrc("[00:01]旧来源歌词"));
        h.media.source=source.into();
        h.frame(vec![]);
        assert!(h.app.ly.lyrics.is_none(),"切换到 {source} 后必须重新选择歌词来源");
    }
}

#[test]
fn closing_a_player_then_starting_native_netease_restores_music_and_lyrics(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.app.settings.island_lyrics=true;
    h.media=crate::smtc::NowPlaying{source:"SodaMusic.exe".into(),title:"旧歌曲".into(),artist:"歌手".into(),playing:true,duration:120.0,..Default::default()};h.frame(vec![]);
    h.app.set_lyrics("旧歌曲","歌手",120.0,crate::lyrics::parse_lrc("[00:01]旧歌词"));
    h.media=Default::default();h.frame(vec![]);assert!(h.app.media.title.is_empty());assert!(h.app.ly.lyrics.is_none());
    h.media=crate::smtc::NowPlaying{source:"cloudmusic.exe".into(),song_id:Some(123),title:"新歌曲".into(),artist:"歌手".into(),playing:true,position:7.0,duration:120.0,..Default::default()};h.frame(vec![]);
    h.app.set_lyrics("新歌曲","歌手",120.0,crate::lyrics::parse_lrc("[00:05]新歌词\n[00:10]下一句"));h.frame(vec![]);
    assert_eq!(h.app.media.title,"新歌曲");assert_eq!(h.app.island_lyric().map(|(_,line)|line),Some("新歌词"));
    h.media.song_id=Some(124);h.frame(vec![]);assert!(h.app.ly.lyrics.is_none(),"歌名相同但歌曲 ID 不同仍需重新取词");
}

#[test]
fn settings_keep_dark_buttons_when_system_theme_changes() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let h=H::new();
    for theme in [egui::Theme::Light,egui::Theme::Dark,egui::Theme::Light] {
        let _=h.ctx.run(egui::RawInput{system_theme:Some(theme),..Default::default()},|ctx| {
            assert!(ctx.style().visuals.widgets.inactive.weak_bg_fill.r()<100,"系统浅色模式不能把深色设置里的按钮变成白底");
        });
    }
}

#[test]
fn expanded_settings_routes_preserve_inline_controls(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for (page,tab) in crate::app::PAGE_SETTINGS_TABS.into_iter().enumerate(){
        let mut h=H::new();h.app.show_page(page);h.hover(H::local(200.0,40.0),1.0);
        h.click(H::local(394.0,18.0));
        assert!(h.app.settings_open,"页面 {page} 应打开设置窗口");
        assert_eq!(h.app.settings_tab,tab);assert_eq!(h.app.page,page);

    }
    let mut h=H::new();h.app.show_page(crate::app::SHELF_PAGE);h.hover(H::local(200.0,40.0),1.0);
    h.click(H::local(330.0,18.0));assert!(h.cmds.iter().any(|cmd|matches!(cmd,crate::app::Cmd::AddFiles)));
    assert!(!h.app.settings_open);
}


#[test]
fn quality_distinct_reminders_are_queued_instead_of_overwritten() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.alert("待办到时间了","任务一",crate::app::TODO_PAGE,true);h.app.alert("待办到时间了","任务二",crate::app::TODO_PAGE,true);
    h.app.alert("待办到时间了","任务一",crate::app::TODO_PAGE,true);
    assert_eq!(h.app.alerts.iter().filter(|a|a.title=="待办到时间了").count(),2,"不同待办均排队，同一待办合并");
    h.app.alert("系统音量","10%",crate::app::MUSIC_PAGE,false);h.app.alert("系统音量","20%",crate::app::MUSIC_PAGE,false);
    assert_eq!(h.app.alerts.iter().filter(|a|a.title=="系统音量").count(),0,"系统 HUD 已移除，不能进入提醒队列");
}

#[test]
fn quality_removed_mail_accounts_ignore_in_flight_results() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.mail_event(crate::mail::Event::Status{account:"removed@example.com".into(),status:crate::mail::Status::Ready});
    h.app.mail_event(crate::mail::Event::Mail{item:crate::mail::Item {received_unix: None, account:"removed@example.com".into(),from:"发件人".into(),subject:"延迟邮件".into(),time:String::new(),code:None,html: String::new().into(), summary:String::new().into(),links:vec![],uid:1},new:true});
    assert!(h.app.mail.status.is_empty() && h.app.mail.recent.is_empty());
    assert!(h.app.notification_bell_until<=h.t);
}

#[test]
fn glass_pill_rim_stays_on_outline_while_collapsing() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for ppp in [1.0,1.25,1.5,2.0] {
        let mut h=H::new();h.app.settings.skin=crate::model::Skin::GlassLight;
        h.app.settings.island_shape=crate::model::IslandShape::Pill;h.ctx.set_zoom_factor(ppp);
        h.hover(Pos2::new(WIN_W/2.0,15.0),1.5);assert!(h.app.progress()>0.99);h.cursor=None;
        let mut raster=crate::preview_raster::Raster::new((crate::app::CANVAS_W*ppp) as usize,(WIN_H*ppp) as usize);
        let mut fills=0;let mut rims=0;
        for frame in 0..100 {
            h.frame(vec![egui::Event::PointerGone]);let fill=h.app.glass_fill();
            let outline=h.hit_shapes[0].outline();let body=Rect::from_points(&outline);
            let surfaces=h.shapes.iter().filter_map(|shape|match &shape.shape {
                egui::Shape::Mesh(mesh) if mesh.vertices.iter().any(|v|v.color==fill)&&mesh.vertices.iter().all(|v|v.color==fill||v.color==egui::Color32::TRANSPARENT)=>Some((shape.clone(),fill.a() as f32/255.0,false)),
                egui::Shape::Rect(rect) if rect.rect.width()>100.0&&rect.fill==fill=>Some((shape.clone(),fill.a() as f32/255.0,false)),
                egui::Shape::Rect(rect) if rect.rect.width()>100.0&&rect.stroke.color==h.app.glass_rim()=>Some((shape.clone(),0.0,true)),
                egui::Shape::Path(path) if path.closed&&path.stroke.width==1.0&&path.points.len()>20&&
                    (Rect::from_points(&path.points).width()-body.width()).abs()<3.0&&
                    (Rect::from_points(&path.points).height()-body.height()).abs()<3.0=>Some((shape.clone(),0.0,true)),
                _=>None,
            }).collect::<Vec<_>>();
            assert!(!surfaces.is_empty(),"每一帧都应检查实际绘制的玻璃轮廓");
            for (shape,alpha,rim) in surfaces {
                if rim{rims+=1;}else{fills+=1;}
                raster.buf.fill([0.0;4]);raster.draw(&h.ctx.tessellate(vec![shape],ppp),ppp);
                // The resident, fused neck and departing body now form one mesh.
                // Sample their actual interior; the rim must never cross it.
                for y in [body.top()+body.height()*0.25,body.center().y,body.top()+body.height()*0.75] {
                    let row=(y*ppp).floor() as usize;
                    for x in ((body.left()+4.0)*ppp).ceil() as usize..((body.right()-4.0)*ppp).floor() as usize {
                        let point=Pos2::new((x as f32+0.5)/ppp,(row as f32+0.5)/ppp);
                        if [egui::vec2(-3.0,0.0),egui::vec2(3.0,0.0),egui::vec2(0.0,-3.0),egui::vec2(0.0,3.0)].iter().all(|offset|crate::shape::contains_outline(&outline,point+*offset)){
                            assert!((raster.buf[row*raster.w+x][3]-alpha).abs()<0.02,"胶囊内部出现接缝：ppp={ppp}, frame={frame}, rim={rim}, point={point:?}, expected={alpha}, actual={}",raster.buf[row*raster.w+x][3]);
                        }
                    }
                }
            }
        }
        assert!(fills>=100&&rims>=100,"应覆盖完整收起动画的底色与边缘：fills={fills}, rims={rims}");
    }
}

#[test]
fn oled_shift_waits_for_the_panel_to_finish_closing() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    h.app.settings.oled_shift=true;
    h.app.show_page(crate::app::MUSIC_PAGE);
    h.hover(H::local(200.0,60.0),5.0);
    assert!(h.app.open);
    h.cursor=Some(Pos2::new(600.0,180.0));
    for _ in 0..120 {
        h.frame(vec![]);
        if !h.app.open {break;}
    }
    assert!(!h.app.open);
    assert!(h.app.progress()>0.1,"测试应停在收起动画尚未完成时");
    assert!(!h.app.oled_shift_idle(&h.ctx),"收起动画仍有 {:.1}% 展开程度时不能恢复位移",h.app.progress()*100.0);
    h.hover(Pos2::new(600.0,180.0),1.0);
    assert!(h.app.oled_shift_idle(&h.ctx),"动画完成后应恢复位移");
}

#[test]
fn oled_shift_pauses_during_hover_intent() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    h.app.settings.oled_shift=true;
    h.hover(Pos2::new(600.0,180.0),1.0);
    assert!(h.app.oled_shift_idle(&h.ctx));
    h.hover(Pos2::new(WIN_W/2.0,12.0),0.05);
    assert!(!h.app.open,"悬停意图缓冲期间尚未展开");
    assert!(h.hit.contains(Pos2::new(WIN_W/2.0,12.0)),"鼠标已进入岛体交互区域");
    assert!(!h.app.oled_shift_idle(&h.ctx),"悬停交互期间不能移动岛体");
}

#[test]
fn oled_shift_pauses_on_the_first_pointer_press_frame() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();let away=Pos2::new(600.0,180.0);
    h.hover(away,1.0);assert!(h.app.oled_shift_idle(&h.ctx));
    h.frame(vec![egui::Event::PointerButton{pos:away,button:egui::PointerButton::Primary,pressed:true,modifiers:Default::default()}]);
    assert!(!h.app.oled_shift_idle(&h.ctx),"按下的首帧必须暂停位移");
    h.frame(vec![]);assert!(!h.app.oled_shift_idle(&h.ctx),"按住期间必须暂停位移");
    h.frame(vec![egui::Event::PointerButton{pos:away,button:egui::PointerButton::Primary,pressed:false,modifiers:Default::default()}]);
    assert!(h.app.oled_shift_idle(&h.ctx),"松开且没有交互后应恢复位移");
}

#[test]
fn oled_shift_waits_for_side_panels_to_finish_closing() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for dock in [crate::model::Dock::Left,crate::model::Dock::Right] {
        let mut h=H::new();h.app.settings.dock=dock;h.app.show_page(crate::app::MUSIC_PAGE);
        let inside=Some(Pos2::new(if dock==crate::model::Dock::Left {100.0}else{400.0},300.0));
        for _ in 0..300 {side_frame(&mut h,inside);}
        assert!(h.app.open);
        for _ in 0..120 {side_frame(&mut h,None);if !h.app.open {break;}}
        assert!(!h.app.open && h.app.progress()>0.1);
        assert!(!h.app.oled_shift_idle(&h.ctx),"{dock:?} 收起动画期间不能移动");
        for _ in 0..120 {side_frame(&mut h,None);}
        assert!(h.app.oled_shift_idle(&h.ctx),"{dock:?} 动画完成后应恢复位移");
    }
}

#[cfg(target_os="macos")]
#[test]
fn qq_music_top_entries_keep_track_text_clear_and_open_the_requested_page(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for lyrics in [false,true]{
        let mut h=H::new();h.app.settings.dock=crate::model::Dock::Top;
        h.app.show_lyrics=lyrics;h.app.settings.lyrics_enabled=lyrics;
        h.media=crate::smtc::NowPlaying{title:"正在听的歌".into(),artist:"歌手".into(),source:"com.apple.Music".into(),..Default::default()};
        h.app.show_page(crate::app::MUSIC_PAGE);
        for _ in 0..65{h.frame(vec![]);}
        for (id,page) in [("m-qq-library",crate::qq_music::Page::Library),("m-qq-player",crate::qq_music::Page::Player)]{
            h.app.qq_library_open=false;for _ in 0..45{h.frame(vec![]);}
            let response=h.ctx.read_response(egui::Id::new(("card",true,id))).expect("QQ Music entry must be present with lyrics on or off");
            assert!(h.hit.contains_rect(response.rect));
            assert!(response.rect.height()>=20.0);
            for shape in &h.shapes{
                if let egui::Shape::Text(text)=&shape.shape{
                    if ["正在听的歌","歌手"].contains(&text.galley.text()){
                        assert!(!text.visual_bounding_rect().intersects(response.rect),"QQ Music controls must not cover track metadata");
                    }
                }
            }
            h.cmds.clear();h.click(response.rect.center());
            assert_eq!(h.cmds,vec![if page==crate::qq_music::Page::Library{crate::app::Cmd::QqMusicRefresh}else{crate::app::Cmd::QqMusic(page)}]);
        }
    }
}

struct H {
    ctx: egui::Context,
    app: App,
    t: f64,
    cursor: Option<Pos2>,
    cmds:Vec<crate::app::Cmd>,
    region:Rect,
    hit:Rect,
    hit_shapes:Vec<crate::app::GlassShape>,
    animating:bool,
    shapes:Vec<egui::epaint::ClippedShape>,

    media: crate::smtc::NowPlaying,
    power: (Option<u8>, bool),
    network: Option<bool>,
    system_idle: Option<f64>,
    screen_locked: bool,
    browser_at_top: Option<bool>,
    file_drop_zone:Option<Rect>,
    hovered_files:Vec<egui::HoveredFile>,
    dropped_files:Vec<egui::DroppedFile>,
}

impl H {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("qingyu-test-{}-{}", std::process::id(), rand_id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("QINGYU_DATA_DIR", &dir);
        let ctx = egui::Context::default();
        let mut app = App::new(&ctx, Options { fetch_lyrics: false, monitor_agents: false });
        // Existing todo tests exercise the retained reminder subview. 拾光 tests
        // explicitly use the production default View instead.
        app.library_view.legacy_tasks = true;
        Self { ctx, app, t: 0.0, cursor: None,cmds:vec![],region:Rect::NOTHING,hit:Rect::NOTHING,hit_shapes:Vec::new(),animating:false,shapes:Vec::new(), media:Default::default(),power:(None,false),network:None,system_idle:None,screen_locked:false,browser_at_top:None,file_drop_zone:None,hovered_files:vec![],dropped_files:vec![] }
    }
    fn frame(&mut self, events: Vec<egui::Event>) {
        self.t += 1.0 / 60.0;
        let mut raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(crate::app::CANVAS_W, WIN_H))),
            time: Some(self.t),
            events,
            hovered_files:self.hovered_files.clone(),
            dropped_files:std::mem::take(&mut self.dropped_files),
            focused: true,
            ..Default::default()
        };
        self.app.filter_todo_input(&mut raw);
        let env = Env {
            t: self.t,
            clock: "09:41:00".into(),
            today: "2026-10-28".into(),
            date: "10月28日 周三".into(),
            cursor: self.cursor,


            media:self.media.clone(),
            battery:self.power.0,
            charging:self.power.1,
            network:self.network,
            idle_seconds:self.system_idle,
            screen_locked:self.screen_locked,
            browser_at_top:self.browser_at_top,
            ..Default::default()
        };
        let cmds=&mut self.cmds;
        let region=&mut self.region;
        let hit=&mut self.hit;
        let hit_shapes=&mut self.hit_shapes;
        let animating=&mut self.animating;
        let app = &mut self.app;
        let file_drop_zone=&mut self.file_drop_zone;
        let output = self.ctx.run(raw, |ctx| {
            egui::CentralPanel::default().frame(egui::Frame::new()).show(ctx, |ui| {
                let win = Rect::from_min_size(ui.max_rect().min,Vec2::new(WIN_W,WIN_H));
                let frame=app.frame(ui, win, &env);*region=frame.region;*hit=frame.hit;*hit_shapes=frame.hit_shapes;*animating=frame.animating;*file_drop_zone=frame.file_drop_zone;cmds.extend(frame.cmds);
            });
        });
        self.shapes=output.shapes;
    }
    fn hover(&mut self, p: Pos2, secs: f64) {
        self.cursor = Some(p);
        let n = (secs * 60.0) as usize;
        for _ in 0..n {
            self.frame(vec![egui::Event::PointerMoved(p)]);
        }
    }
    fn click(&mut self, p: Pos2) {
        self.cursor = Some(p);
        self.frame(vec![egui::Event::PointerMoved(p)]);
        self.frame(vec![egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() }]);
        self.frame(vec![egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() }]);
    }
    #[track_caller]
    fn response(&self,id:impl std::hash::Hash)->egui::Response {
        self.ctx.read_response(egui::Id::new(("card",true,id))).expect("visible control")
    }
    #[track_caller]
    fn drag_response(&self,id:impl std::hash::Hash)->egui::Response {
        self.ctx.read_response(egui::Id::new(("card-drag",true,id))).expect("visible drag control")
    }
    #[track_caller]
    fn click_control(&mut self,id:impl std::hash::Hash) {self.click(self.response(id).rect.center());}
    fn shelf_scroll_offset(&self)->f32 {
        let id=egui::Id::new(("portrait-scroll",true,"shelf-grid")).with(egui::Id::new("shelf-grid"));
        egui::scroll_area::State::load(&self.ctx,id).expect("shelf scroll state").offset.y
    }
    fn music_seek_track(&self)->Rect {
        let seek=self.drag_response("m-seek").rect;
        self.shapes.iter().filter_map(|shape|match &shape.shape {
            egui::Shape::Rect(rect) if seek.contains_rect(rect.rect)&&rect.rect.height()<6.0=>Some(rect.rect),_=>None,
        }).max_by(|a,b|a.width().total_cmp(&b.width())).expect("visible music seek track")
    }
    fn click_text(&mut self,label:&str){
        if !self.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()==label))&&self.app.notice.is_some(){self.click_notice_badge();}
        let point=self.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t)if t.galley.text()==label&&t.opacity_factor>0.9&&s.clip_rect.contains(t.visual_bounding_rect().center())=>Some(t.visual_bounding_rect().center()),_=>None}).unwrap_or_else(||panic!("未找到可点击文字：{label}"));
        self.click(point);
    }
    fn click_notice_badge(&mut self){if self.app.compact_notices(){self.click(Pos2::new(WIN_W/2.0+(self.app.current_width()-44.0)/2.0+26.0,15.0));}}
    fn click_module(&mut self,page:usize){
        let id=egui::Id::new(("card",true,(if cfg!(target_os="macos"){"page-tab"}else{"dot"},page)));
        let point=self.ctx.read_response(id).expect("visible module navigation").rect.center();self.click(point);
    }
    fn page_key(&mut self,next:bool){
        let key=if next{egui::Key::ArrowRight}else{egui::Key::ArrowLeft};
        for pressed in [true,false]{self.frame(vec![egui::Event::Key{key,physical_key:None,pressed,repeat:false,modifiers:Default::default()}]);}
    }
    fn wheel(&mut self,delta:f32) {
        self.frame(vec![egui::Event::MouseWheel{unit:egui::MouseWheelUnit::Point,delta:Vec2::new(0.0,delta),modifiers:Default::default()}]);
    }
    fn picker_frame(&mut self,events:Vec<egui::Event>){
        self.frame(events);
    }
    fn picker_click(&mut self,label:&str){
        for _ in 0..60 {self.picker_frame(vec![]);if (self.app.progress()-1.0).abs()<0.01 {break;}}
        self.picker_frame(vec![]);
        let find=|h:&H|h.shapes.iter().find_map(|shape|match &shape.shape{
            egui::Shape::Text(text) if text.galley.text()==label && shape.clip_rect.contains(text.visual_bounding_rect().center())=>Some(text.visual_bounding_rect().center()),_=>None,
        });
        let point=find(self).unwrap_or_else(||panic!("未找到可见选项：{label}"));
        assert!(self.hit.contains(point),"选项 {label} 超出岛体命中区域");
        self.picker_frame(vec![egui::Event::PointerMoved(point)]);
        for pressed in [true,false]{self.picker_frame(vec![egui::Event::PointerButton{pos:point,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()}]);}
    }
    fn local(x: f32, y: f32) -> Pos2 {
        Pos2::new((WIN_W - PW) / 2.0 + x, y+crate::app::BAR_H+crate::island_position::PANEL_GAP)
    }

}

fn rand_id() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos() as u64
}

fn island_drag_frame(h:&mut H,bounds:crate::display_layout::Bounds,ppp:f32,pointer:Option<Pos2>,pressed:Option<bool>)->Rect {
    use crate::{display_layout,side_dock};
    let dock=h.app.settings.dock;
    let (width,height,screen)=if let Some(position)=h.app.placement_position(){
        let room=Vec2::new((bounds.right-bounds.left) as f32,(bounds.bottom-bounds.top) as f32)/ppp;
        let panel=if dock.is_side(){h.app.settings.side_panel_size()}else{Vec2::new(PW,crate::control_center::TOP_H)};
        let (_,plan)=crate::island_position::plan(room,position,dock,h.app.resident_size(),panel,ppp);
        h.app.set_desktop_placement(crate::app::DesktopPlacement::Positioned(plan));
        (plan.canvas.x,plan.canvas.y,plan.room)
    }else{
        h.app.set_desktop_placement(crate::app::DesktopPlacement::Legacy);
        let (width,height)=if dock.is_side(){let size=h.app.canvas_size();(size.x,size.y)}else{(WIN_W,WIN_H)};
        let offset=h.app.placement_offset();
        let (x,y)=if dock.is_side(){display_layout::side_origin(bounds,dock,(width,height),height/2.0,ppp,offset)}
            else{display_layout::island_origin_offset(bounds,width,ppp,offset)}.unwrap();
        let screen=Rect::from_min_max(Pos2::new((bounds.left-x) as f32/ppp,(bounds.top-y) as f32/ppp),Pos2::new((bounds.right-x) as f32/ppp,(bounds.bottom-y) as f32/ppp));
        (width,height,screen)
    };
    let cursor=pointer.map(|p|p+screen.min.to_vec2());
    let mut events=cursor.map(|p|vec![egui::Event::PointerMoved(p)]).unwrap_or_default();
    if let (Some(pos),Some(pressed))=(cursor,pressed){events.push(egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()});}
    h.t+=1.0/60.0;
    let env=Env{t:h.t,clock:"09:41:00".into(),today:"2026-10-28".into(),date:"10月28日 周三".into(),cursor,screen:Some(screen),media:h.media.clone(),..Default::default()};
    let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(crate::app::CANVAS_W.max(width),height))),time:Some(h.t),events,focused:true,
        viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(ppp),..Default::default()})).collect(),..Default::default()};
    let output=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::new()).show(ctx,|ui|{
        let out=h.app.frame(ui,Rect::from_min_size(ui.max_rect().min,Vec2::new(width,height)),&env);
        if let (Some(input),Some(glass))=(&out.hit_outline,&out.glass){assert_eq!(*input,glass[0].outline(),"drag edge changes must preserve the painted frame's input and mask");}
        h.region=out.region;h.hit=out.hit;h.cmds.extend(out.cmds);
    });});
    h.shapes=output.shapes;
    screen
}

fn island_drag_header(h:&mut H,bounds:crate::display_layout::Bounds,ppp:f32)->Pos2 {
    h.app.show_page(1);
    let mut screen=Rect::NOTHING;
    for _ in 0..90{screen=island_drag_frame(h,bounds,ppp,None,None);}
    let rect=h.ctx.read_response(egui::Id::new(("card-drag",true,"island-position"))).expect("展开后的时钟或日期应提供拖动把手").rect;
    let point=if h.app.side(){Pos2::new(rect.center().x,rect.bottom()-1.0)}else{rect.center()};
    point-screen.min.to_vec2()
}

#[test]
fn island_position_drag_ignores_jitter_and_preserves_header_buttons(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let bounds=crate::display_layout::Bounds{left:0,top:0,right:1920,bottom:1080};
    let mut h=H::new();let start=island_drag_header(&mut h,bounds,1.0);
    island_drag_frame(&mut h,bounds,1.0,Some(start),Some(true));
    island_drag_frame(&mut h,bounds,1.0,Some(start+Vec2::splat(1.0)),None);
    island_drag_frame(&mut h,bounds,1.0,Some(start+Vec2::splat(1.0)),Some(false));
    assert_eq!(h.app.settings.island_offset,0,"轻微手抖不能改变位置");
    let screen=island_drag_frame(&mut h,bounds,1.0,Some(start),None);
    let gear=H::local(394.0,18.0)-screen.min.to_vec2();
    island_drag_frame(&mut h,bounds,1.0,Some(gear),Some(true));
    island_drag_frame(&mut h,bounds,1.0,Some(gear),Some(false));
    assert!(h.app.settings_open,"时间区域拖动不能拦截设置按钮");
    assert_eq!(h.app.settings.island_offset,0);
    h.app.settings_open=false;
    for (page,x,expected) in [(crate::app::SHELF_PAGE,330.0,crate::app::Cmd::AddFiles)] {
        h.app.show_page(page);
        for _ in 0..60{island_drag_frame(&mut h,bounds,1.0,Some(start),None);}
        let screen=island_drag_frame(&mut h,bounds,1.0,Some(start),None);
        let point=H::local(x,18.0)-screen.min.to_vec2();
        island_drag_frame(&mut h,bounds,1.0,Some(point),Some(true));
        island_drag_frame(&mut h,bounds,1.0,Some(point),Some(false));
        assert!(h.cmds.contains(&expected),"顶部操作按钮应正常响应");
        assert_eq!(h.app.settings.island_offset,0);
    }
}
#[test]
fn shelf_drop_zone_leaves_normal_hover_and_text_selection_unchanged(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    for _ in 0..90{h.frame(vec![]);}let zone=h.file_drop_zone.unwrap();
    let win=Rect::from_min_size(Pos2::ZERO,h.app.canvas_size());
    let expected=Rect::from_min_size(h.app.content_origin(win),Vec2::new(h.app.expanded_width(),h.app.expanded_height())).union(h.app.resident_rect(win));
    assert_eq!(zone,expected);
    let point=zone.right_bottom()-Vec2::splat(12.0);assert!(!h.hit.contains(point));
    h.hover(point,0.8);assert!(!h.app.open);
    h.frame(vec![egui::Event::PointerButton{pos:point,button:egui::PointerButton::Primary,pressed:true,modifiers:Default::default()}]);
    h.hover(point,0.8);assert!(!h.app.open,"按住鼠标拖选文字不应打开置物架");
    h.frame(vec![egui::Event::PointerButton{pos:point,button:egui::PointerButton::Primary,pressed:false,modifiers:Default::default()}]);
    h.app.settings.visible_pages[crate::app::SHELF_PAGE]=false;h.frame(vec![]);assert!(h.file_drop_zone.is_none());
}

#[test]
fn shelf_file_drag_opens_below_the_island_and_cancellation_restores_the_page(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.go(1,1.0);for _ in 0..90{h.frame(vec![]);}
    let point=h.file_drop_zone.unwrap().right_bottom()-Vec2::splat(12.0);
    assert!(!h.hit.contains(point));h.cursor=Some(point);
    h.hovered_files.push(egui::HoveredFile{path:Some(crate::model::config_dir().join("拖入文件.txt")),..Default::default()});
    h.frame(vec![]);assert!(h.app.open && h.app.page==crate::app::SHELF_PAGE && h.app.drag_hot,"文件进入感应区应立即展开置物架");
    for _ in 0..60{h.frame(vec![]);}assert!(h.app.open);
    h.hovered_files.clear();h.cursor=None;for _ in 0..60{h.frame(vec![]);}
    assert!(!h.app.drag_hot && !h.app.open);assert_eq!(h.app.page,1);
    assert!(h.app.shelf.items.is_empty(),"取消拖入不应保存文件");
}

#[test]
fn shelf_quick_drop_in_the_capture_zone_shows_the_added_file(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.frame(vec![]);
    let path=crate::model::config_dir().join("快速拖入.txt");std::fs::create_dir_all(path.parent().unwrap()).unwrap();std::fs::write(&path,"test").unwrap();
    h.cursor=Some(h.file_drop_zone.unwrap().right_bottom()-Vec2::splat(12.0));
    h.dropped_files.push(egui::DroppedFile{path:Some(path.clone()),..Default::default()});
    for _ in 0..90{h.frame(vec![]);}
    assert!(h.app.open && h.app.page==crate::app::SHELF_PAGE);assert_eq!(h.app.shelf.items,vec![path.clone()]);assert!(path.exists());
    let mut h=H::new();for _ in 0..90{h.frame(vec![]);}
    h.cursor=Some(h.file_drop_zone.unwrap().right_bottom()-Vec2::splat(12.0));h.app.settings.visible_pages[crate::app::SHELF_PAGE]=false;
    h.dropped_files.push(egui::DroppedFile{path:Some(path),..Default::default()});h.frame(vec![]);
    assert!(!h.app.open && !h.app.settings_open,"关闭置物架后应忽略拖入");
}

#[test]
fn settings_opening_restores_focus_once_without_becoming_topmost() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    h.app.settings.settings_theme=crate::settings_theme::SettingsTheme::Dark;
    h.ctx.set_embed_viewports(false);
    let expected_theme=if cfg!(target_os="macos"){egui::SystemTheme::Light}else{egui::SystemTheme::Dark};
    let frame=|h:&mut H,focused:bool| {
        h.ctx.data_mut(|d|d.insert_temp(egui::Id::new("settings-system-theme"),(std::time::Instant::now(),false)));
        egui::Context::set_immediate_viewport_renderer(move |ctx,mut viewport| {
            let id=viewport.ids.this;
            let mut raw=egui::RawInput{viewport_id:id,focused,system_theme:Some(egui::Theme::Light),..Default::default()};
            raw.viewports.insert(id,egui::ViewportInfo{focused:Some(focused),minimized:Some(!focused),..Default::default()});
            let _=ctx.run(raw,|ctx|(viewport.viewport_ui_cb)(ctx));
        });
        let id=egui::ViewportId::from_hash_of("qingyu-settings");
        let mut raw=egui::RawInput::default();
        raw.viewports.insert(id,egui::ViewportInfo{focused:Some(focused),..Default::default()});
        let mut output=h.ctx.run(raw,|ctx|h.app.settings_ui(ctx,false));
        let viewport=output.viewport_output.remove(&id).unwrap();
        assert!(viewport.commands.iter().any(|cmd|matches!(cmd,egui::ViewportCommand::SetTheme(theme) if *theme==expected_theme)),"浅色系统下窗口标题栏和边框也应使用深色");
        (viewport.builder,viewport.commands)
    };
    h.app.open_mail_settings();
    let (builder,commands)=frame(&mut h,false);
    assert_eq!(builder.visible,Some(false),"设置窗口应隐藏创建，避免首帧绘制前露出白色背景");
    assert_eq!(builder.active,Some(false),"隐藏窗口不能提前激活");
    let show=commands.iter().position(|cmd|matches!(cmd,egui::ViewportCommand::Visible(true))).expect("首帧绘制后应显示设置窗口");
    let focus=commands.iter().position(|cmd|matches!(cmd,egui::ViewportCommand::Focus)).unwrap();
    let theme=commands.iter().position(|cmd|matches!(cmd,egui::ViewportCommand::SetTheme(theme) if *theme==expected_theme)).unwrap();
    assert!(theme<show,"显示窗口前应设置深色边框，避免白边闪现");
    assert!(show<focus,"应先显示已绘制的窗口，再获取焦点");
    assert_eq!(builder.window_level,Some(egui::WindowLevel::Normal),"打开设置时应保持普通窗口层级");
    assert!(commands.iter().any(|cmd|matches!(cmd,egui::ViewportCommand::Minimized(false))),"应恢复已最小化的设置窗口：{commands:?}");
    assert!(commands.iter().any(|cmd|matches!(cmd,egui::ViewportCommand::Focus)),"应将焦点交给设置窗口");
    let (builder,commands)=frame(&mut h,true);
    assert!(!commands.iter().any(|cmd|matches!(cmd,egui::ViewportCommand::Visible(_))),"正常重绘不应重复显示窗口，避免打断最小化");
    assert_eq!(builder.window_level,Some(egui::WindowLevel::Normal),"操作设置时也不能切换到置顶层");
    assert!(!commands.iter().any(|cmd|matches!(cmd,egui::ViewportCommand::Focus)),"正常绘制不应反复抢焦点");
    let (builder,commands)=frame(&mut h,false);
    assert_eq!(builder.window_level,Some(egui::WindowLevel::Normal),"切回其他程序后应解除设置置顶");
    assert!(!commands.iter().any(|cmd|matches!(cmd,egui::ViewportCommand::Focus)));
    h.app.open_mail_settings();
    let (builder,commands)=frame(&mut h,false);
    assert_eq!(builder.window_level,Some(egui::WindowLevel::Normal),"再次唤起设置仍须保持普通窗口层级");
    assert!(commands.iter().any(|cmd|matches!(cmd,egui::ViewportCommand::Minimized(false))));
    assert!(commands.iter().any(|cmd|matches!(cmd,egui::ViewportCommand::Focus)),"设置已打开时仍应响应再次唤起");
    h.app.settings_open=false;
    let _=h.ctx.run(egui::RawInput::default(),|ctx|h.app.settings_ui(ctx,false));
    h.app.open_settings();
    let (builder,commands)=frame(&mut h,false);
    assert_eq!(builder.visible,Some(false),"关闭后重新创建也应隐藏到首帧完成");
    assert!(commands.iter().any(|cmd|matches!(cmd,egui::ViewportCommand::Visible(true))),"重开设置应正常显示");
}

#[test]
fn web_video_is_hidden_without_changing_music_preferences(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.app.show_page(crate::app::MUSIC_PAGE);
    h.app.opts.fetch_lyrics=true;h.app.show_lyrics=true;
    h.app.ly.key="Previous Song|Artist|120".into();
    h.app.ly.lyrics=Some(crate::lyrics::parse_lrc("[00:01.00]previous song"));
    h.app.ly.error=Some(crate::lyrics::FetchError::Unavailable("503".into()));
    let (tx,rx)=std::sync::mpsc::channel();h.app.ly.rx=Some(rx);
    h.media=crate::smtc::NowPlaying{title:"小唐的Token的抖音 - 抖音".into(),playing:true,duration:17.0,..Default::default()};
    h.frame(vec![]);
    assert!(h.app.media.title.is_empty() && !h.app.media.playing,"视频标题和播放状态不应进入岛内音乐页面");
    assert!(h.app.ly.key.is_empty(),"视频不应建立歌曲取词任务");
    assert!(h.app.ly.lyrics.is_none() && h.app.ly.error.is_none(),"切到视频应清除上一首歌词和歌词错误");
    assert!(tx.send(Ok((crate::lyrics::parse_lrc("[00:01.00]late song"),false))).is_err(),"视频不能接收旧歌曲迟到的查询结果");
    h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);
    h.click_control("m-lyr");
    assert!(h.app.show_lyrics,"视频页面的标签不应切换用户的歌曲歌词偏好");
    h.click_control("m-play");
    assert!(!h.cmds.iter().any(|cmd|matches!(cmd,crate::app::Cmd::Media(_))),"音乐页不应控制被隐藏的视频");
    h.app.opts.fetch_lyrics=false;
    h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),duration:120.0,..Default::default()};
    h.hover(Pos2::new(WIN_W/2.0,12.0),0.5);
    h.click_control("m-lyr");
    assert!(!h.app.show_lyrics,"回到歌曲时应恢复原来的词按钮");
}

#[test]
fn web_video_does_not_override_the_minimal_island(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.app.settings.minimal=true;
    h.media=crate::smtc::NowPlaying{title:"小唐的Token的抖音 - 抖音".into(),playing:true,duration:17.0,..Default::default()};
    for _ in 0..90 {h.frame(vec![]);}
    assert!(h.region.height()<20.0,"视频不能触发歌曲专用的收起态");
}

#[test]
fn minimal_mode_stays_a_line_with_music_and_after_media_changes(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    h.app.settings.minimal=true;
    h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),playing:true,duration:120.0,..Default::default()};
    for _ in 0..90 {h.frame(vec![]);}
    assert!(h.region.height()<20.0,"播放音乐时仍应遵守极简开关");
    assert!(h.hit.height()<=crate::app::BAR_H_MIN+0.1,"鼠标命中范围也应收回细线");
    h.media.title="Another Track".into();h.media.playing=false;
    for _ in 0..90 {h.frame(vec![]);}
    assert!(h.region.height()<20.0,"暂停或切歌不能覆盖极简开关");
    h.media=Default::default();
    for _ in 0..90 {h.frame(vec![]);}
    assert!(h.region.height()<20.0,"没有媒体时恢复用户选择的极简状态");
}

#[test]
fn toggling_minimal_changes_the_island_while_media_is_present(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),playing:true,duration:120.0,..Default::default()};
    for _ in 0..90 {h.frame(vec![]);}assert!(h.region.height()>30.0);
    h.app.settings.minimal=true;for _ in 0..90 {h.frame(vec![]);}
    assert!(h.region.height()<20.0,"开启极简后应收成细线");
    h.app.settings.minimal=false;for _ in 0..90 {h.frame(vec![]);}
    assert!(h.region.height()>30.0,"关闭极简后应恢复普通岛体");
}


#[test]
fn delayed_media_samples_keep_the_live_position_and_follow_pause_and_seek(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),position:9.2,duration:120.0,playing:true,sampled_at:Some(std::time::Instant::now()-std::time::Duration::from_millis(600)),..Default::default()};
    h.frame(vec![]);
    assert!(h.app.media.position>=9.79,"媒体位置必须包含送到界面之前经过的时间");
    h.media.playing=false;h.media.position=9.4;h.frame(vec![]);
    assert_eq!(h.app.media.position,9.4,"暂停不能继续外推");
    h.media.playing=true;h.media.position=4.0;h.media.sampled_at=Some(std::time::Instant::now());h.frame(vec![]);
    assert!((4.0..4.2).contains(&h.app.media.position),"跳转后应使用新的播放位置");
    h.media.position=119.9;h.media.sampled_at=Some(std::time::Instant::now()-std::time::Duration::from_secs(1));h.frame(vec![]);
    assert_eq!(h.app.media.position,120.0,"外推不能超过歌曲结尾");
}

#[test]
fn kugou_external_seek_updates_lyrics_in_both_directions(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.settings.island_lyrics=true;
    h.media=crate::smtc::NowPlaying{source:"KuGou.exe".into(),title:"Track".into(),artist:"Artist".into(),duration:120.0,playing:true,..Default::default()};
    h.frame(vec![]);
    h.app.set_lyrics("Track","Artist",120.0,crate::lyrics::parse_lrc("[00:00]开头\n[00:30]中段\n[01:00]后段"));
    for (position,line) in [(65.0,"后段"),(10.0,"开头"),(35.0,"中段")] {
        h.media.position=position;h.media.sampled_at=Some(std::time::Instant::now());h.frame(vec![]);
        assert_eq!(h.app.island_lyric().map(|(_,text)|text),Some(line),"酷狗跳转后应立即切换到对应歌词");
    }
}

#[test]
fn music_clients_keep_lyrics_across_two_repeat_cycles(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for source in ["kugou","QQMusic.exe","cloudmusic.exe","汽水音乐","Spotify.exe","kwmusic.exe"] {
        let mut h=H::new();h.app.settings.island_lyrics=true;
        h.media=crate::smtc::NowPlaying{source:source.into(),title:"循环测试".into(),artist:"歌手".into(),duration:120.0,playing:true,..Default::default()};
        h.frame(vec![]);h.app.set_lyrics("循环测试","歌手",120.0,crate::lyrics::parse_lrc("[00:00]开头\n[00:05]第一段\n[01:50]尾声"));
        for (position,line) in [(119.0,"尾声"),(0.0,"开头"),(8.0,"第一段"),(119.0,"尾声"),(0.0,"开头"),(8.0,"第一段")] {
            h.media.position=position;h.media.sampled_at=Some(std::time::Instant::now());h.frame(vec![]);
            assert_eq!(h.app.island_lyric().map(|(_,text)|text),Some(line),"{source} 第二、三遍也必须显示相应歌词");
        }
    }
}

#[test]
fn music_without_a_timeline_does_not_invent_elapsed_time(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.media=crate::smtc::NowPlaying{source:"QQMusic.exe".into(),title:"歌曲".into(),playing:true,playback_type:crate::smtc::MediaKind::Music,..Default::default()};
    for _ in 0..240{h.frame(vec![]);}
    assert_eq!(h.app.media.title,"歌曲");assert_eq!(h.app.media.position,0.0);assert_eq!(h.app.media.duration,0.0);
}

#[test]
fn kugou_position_advances_without_duration_and_respects_pause_and_seek(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.media=crate::smtc::NowPlaying{source:"kugou".into(),title:"Track".into(),artist:"Artist".into(),position:42.0,duration:0.0,playing:true,sampled_at:Some(std::time::Instant::now()-std::time::Duration::from_millis(600)),..Default::default()};
    h.frame(vec![]);
    assert!(h.app.media.position>=42.59,"未知总时长不能阻止真实进度外推");
    h.media.playing=false;h.frame(vec![]);assert_eq!(h.app.media.position,42.0);
    h.media.playing=true;h.media.position=12.0;h.media.sampled_at=Some(std::time::Instant::now());h.frame(vec![]);
    assert!((12.0..12.2).contains(&h.app.media.position),"拖动后必须跟随新的实际位置");
}

#[test]
fn music_volume_popup_opens_and_mute_restores_the_selected_players_level(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.app.show_page(crate::app::MUSIC_PAGE);
    h.media=crate::smtc::NowPlaying{source:"QQMusic.exe".into(),title:"Track".into(),artist:"Artist".into(),duration:120.0,volume:Some((60,false)),..Default::default()};
    h.app.volume_ui=Some((45,false));
    h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);
    h.click_control("m-volume");
    assert!(h.app.music_volume_open);
    assert!(h.cmds.is_empty(),"打开浮条不应改变音量或静音");
    let mute=h.response("m-volume-mute").rect.center();h.hover(mute,0.2);h.click_control("m-volume-mute");
    assert!(h.cmds.contains(&crate::app::Cmd::MusicVolume(crate::smtc::Transport::SetMute(true),"QQMusic.exe".into())));
    assert!(!h.cmds.contains(&crate::app::Cmd::ToggleMute));
    assert_eq!(h.app.music_volume_ui,Some((60,true)));
    assert_eq!(h.app.volume_ui,Some((45,false)));
    h.click_control("m-volume-mute");
    assert_eq!(h.app.music_volume_ui,Some((60,false)),"恢复时沿用静音前的音量");
    assert!(h.cmds.contains(&crate::app::Cmd::MusicVolume(crate::smtc::Transport::SetMute(false),"QQMusic.exe".into())));
}

#[test]
fn music_volume_popup_slider_unmutes_and_scrolls_without_changing_pages(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.app.show_page(crate::app::MUSIC_PAGE);
    h.media=crate::smtc::NowPlaying{source:"QQMusic.exe".into(),title:"Track".into(),artist:"Artist".into(),volume:Some((60,true)),..Default::default()};
    h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);h.click_control("m-volume");let track=h.drag_response("m-volume-track").rect;
    let at=|y:f32|Pos2::new(track.center().x,track.top()+y-27.0);
    h.hover(at(53.5),0.2);h.click(at(53.5));
    assert_eq!(h.app.music_volume_ui,Some((50,false)));
    assert!(h.cmds.contains(&crate::app::Cmd::MusicVolume(crate::smtc::Transport::SetVolume(50),"QQMusic.exe".into())));
    h.wheel(120.0);assert_eq!(h.app.music_volume_ui,Some((54,false)));assert_eq!(h.app.page,crate::app::MUSIC_PAGE);
    h.click(at(32.0));assert_eq!(h.app.music_volume_ui,Some((100,false)));
    h.click(at(75.0));assert_eq!(h.app.music_volume_ui,Some((0,false)));
    let surface=h.response("m-volume-surface").rect;h.click(surface.right_center()-egui::vec2(2.0,0.0));assert_eq!(h.app.page,crate::app::MUSIC_PAGE,"浮条遮挡部分不能穿透触发翻页");
    h.click(surface.right_top()+egui::vec2(-2.0,12.0));assert!(!h.app.settings_open,"浮条遮挡部分不能穿透打开设置");
}

#[test]
fn music_volume_popup_closes_on_escape_outside_click_source_change_and_collapse(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.app.show_page(crate::app::MUSIC_PAGE);
    h.media=crate::smtc::NowPlaying{source:"QQMusic.exe".into(),title:"Track".into(),artist:"Artist".into(),volume:Some((60,false)),..Default::default()};
    h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);h.click_control("m-volume");
    h.frame(vec![egui::Event::Key{key:egui::Key::Escape,physical_key:None,pressed:true,repeat:false,modifiers:Default::default()}]);
    assert!(!h.app.music_volume_open);assert!(h.app.open,"Esc 只关闭浮条，保留音乐面板");
    h.click_control("m-volume");h.click(H::local(180.0,45.0));assert!(!h.app.music_volume_open);
    h.click_control("m-volume");h.media.source="cloudmusic.exe".into();h.frame(vec![]);assert!(!h.app.music_volume_open);
    h.click_control("m-volume");h.app.show_page(1);assert!(!h.app.music_volume_open);
    h.app.show_page(crate::app::MUSIC_PAGE);h.hover(H::local(180.0,45.0),1.0);h.click_control("m-volume");
    h.hover(Pos2::new(0.0,250.0),5.0);assert!(!h.app.music_volume_open);
}

#[test]
#[ignore="按需导出音乐音量浮条实际画面：QINGYU_MUSIC_VOLUME_PREVIEW=目录"]
fn music_volume_popup_gallery(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let dest=std::path::PathBuf::from(std::env::var_os("QINGYU_MUSIC_VOLUME_PREVIEW").expect("需指定预览目录"));std::fs::create_dir_all(&dest).unwrap();
    for (name,muted,lyrics) in [("volume",false,false),("muted",true,false),("lyrics",false,true)] {
        let mut h=H::new();h.ctx.set_pixels_per_point(2.0);h.app.show_lyrics=lyrics;
        h.media=crate::smtc::NowPlaying{source:"cloudmusic.exe".into(),title:"晴天".into(),artist:"周杰伦".into(),position:92.0,duration:269.0,volume:Some((62,muted)),..Default::default()};
        h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);
        if lyrics{h.app.set_lyrics("晴天","周杰伦",269.0,crate::lyrics::parse_lrc("[00:00]午后的阳光落在窗边\n[01:30]风把回忆吹向远方\n[01:40]慢慢走过这段时光"));}
        h.click(H::local(367.0,h.app.music_control_y()));h.hover(H::local(330.0,112.0),0.25);
        let ppp=h.ctx.pixels_per_point();let mut raster=crate::preview_raster::Raster::new((crate::app::CANVAS_W*ppp) as usize,(WIN_H*ppp) as usize);
        raster.update_textures(&egui::TexturesDelta{set:vec![(egui::TextureId::default(),egui::epaint::ImageDelta::full(h.ctx.fonts(|f|f.image()),egui::TextureOptions::LINEAR))],..Default::default()});
        raster.buf.fill([0.10,0.12,0.16,1.0]);raster.draw(&h.ctx.tessellate(h.shapes.clone(),ppp),ppp);
        let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
        let image=image::RgbaImage::from_raw(raster.w as u32,raster.h as u32,rgba).unwrap();
        image::imageops::crop_imm(&image,((WIN_W-crate::app::PW)/2.0*ppp-12.0) as u32,0,((crate::app::PW+12.0)*ppp+12.0) as u32,((crate::app::PH+12.0)*ppp) as u32).to_image().save(dest.join(format!("{name}.png"))).unwrap();
    }
}

#[test]
fn music_volume_wheel_and_player_switch_keep_system_volume_untouched(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.app.show_page(crate::app::MUSIC_PAGE);
    h.media=crate::smtc::NowPlaying{source:"QQMusic.exe".into(),title:"Track".into(),artist:"Artist".into(),volume:Some((60,true)),..Default::default()};
    h.app.volume_ui=Some((45,false));h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);let volume=h.response("m-volume").rect.center();h.hover(volume,0.2);h.wheel(120.0);
    assert!(h.cmds.contains(&crate::app::Cmd::MusicVolume(crate::smtc::Transport::SetVolume(64),"QQMusic.exe".into())),"音量={:?} 命令={:?} 页={} 指针={:?}",h.app.music_volume_ui,h.cmds,h.app.page,h.cursor);
    assert_eq!(h.app.music_volume_ui,Some((64,false)));
    h.media.source="cloudmusic.exe".into();h.media.volume=Some((22,false));h.frame(vec![]);
    assert_eq!(h.app.music_volume_ui,Some((22,false)),"换播放器立即更新独立音量");
    assert_eq!(h.app.volume_ui,Some((45,false)));
    h.media.volume=None;h.cmds.clear();h.click_control("m-volume");h.wheel(120.0);
    assert!(h.cmds.is_empty(),"没有应用会话时不可回退到系统音量");
}

#[test]
fn music_uses_system_artwork_and_clears_it_when_the_next_song_has_none(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    let image=std::sync::Arc::new(egui::ColorImage::new([32,32],egui::Color32::BLUE));
    h.media=crate::smtc::NowPlaying{title:"First".into(),artist:"Artist".into(),artwork:Some(image.clone()),..Default::default()};
    h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);
    let (cached,texture)=h.app.music_artwork.as_ref().expect("播放页应使用系统封面");
    assert!(std::sync::Arc::ptr_eq(cached,&image));
    let texture_id=texture.id();
    h.frame(vec![]);
    assert_eq!(h.app.music_artwork.as_ref().unwrap().1.id(),texture_id,"进度更新不应重复上传纹理");
    h.media=crate::smtc::NowPlaying{title:"Second".into(),artist:"Artist".into(),..Default::default()};
    h.frame(vec![]);
    assert!(h.app.music_artwork.is_none(),"缺少封面的下一首歌应恢复占位图");
}

#[test]
fn failed_lyrics_can_be_retried_from_the_music_page(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.app.show_page(crate::app::MUSIC_PAGE);
    h.app.settings.island_lyrics=true;
    h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),duration:120.0,..Default::default()};
    h.app.ly.key="Track|Artist|120".into();
    h.app.ly.error=Some(crate::lyrics::FetchError::Unavailable("HTTP 502".into()));
    h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="暂未获取歌词，请重试")),"展开内应保留歌词错误提示");
    h.click_control("lyrics-retry");
    assert!(h.app.ly.key.is_empty(),"点击错误提示后应允许重新请求当前歌曲");
}

#[test]
fn transient_lyric_failures_retry_but_missing_songs_do_not_loop(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for (error,retry) in [(crate::lyrics::FetchError::Unavailable("503".into()),true),(crate::lyrics::FetchError::NotFound,false),(crate::lyrics::FetchError::IncompleteMetadata,false)] {
        let mut h=H::new();
        h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),duration:120.0,..Default::default()};
        h.app.opts.fetch_lyrics=true;h.app.show_lyrics=true;
        h.app.ly.key="Track|Artist|120".into();
        let (tx,rx)=std::sync::mpsc::channel();
        h.app.ly.rx=Some(rx);h.app.ly.loading=true;
        tx.send(Err(error.clone())).unwrap();
        h.frame(vec![]);
        assert_eq!(h.app.ly.error,Some(error));
        assert_eq!(h.app.ly.retry_at.is_some(),retry);
        assert!(!h.app.ly.loading);
        if let Some(at)=h.app.ly.retry_at {assert!((at-h.t-30.0).abs()<0.001,"网络恢复重试应有间隔");}
    }
}

#[test]
fn music_lyrics_show_in_a_separate_right_column() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.app.show_page(crate::app::MUSIC_PAGE);
    h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),position:7.0,duration:120.0,..Default::default()};
    h.app.set_lyrics("Track","Artist",120.0,crate::lyrics::parse_lrc("[00:01]上一句\n[00:05]当前歌词\n[00:10]下一句"));
    h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);
    let current=h.shapes.iter().find_map(|shape|match &shape.shape {
        egui::Shape::Text(text) if text.galley.text()=="当前歌词"=>Some(text.pos),_=>None,
    }).expect("展开页应显示同步歌词");
    assert!(current.x>=H::local(243.0,0.0).x,"歌词必须位于播放器右侧");
    let title=h.shapes.iter().find_map(|shape|match &shape.shape {
        egui::Shape::Text(text) if text.galley.text()=="Track"=>Some(text.pos),_=>None,
    }).unwrap();
    assert!(title.x<H::local(233.0,0.0).x);
    h.click(H::local(74.0,h.app.music_control_y()));
    assert!(!h.app.show_lyrics);
    h.frame(vec![]);
    assert!(!h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(text) if text.galley.text()=="当前歌词")));
}

#[test]
fn disabling_lyrics_disconnects_pending_workers(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    h.media=crate::smtc::NowPlaying{source:"KuGou.exe".into(),title:"测试歌曲".into(),artist:"歌手".into(),..Default::default()};
    h.frame(vec![]);h.app.opts.fetch_lyrics=true;
    h.app.show_lyrics=false;h.app.settings.island_lyrics=false;
    h.app.ly.key="测试歌曲|歌手|0".into();h.app.ly.loading=true;
    let (network,rx)=std::sync::mpsc::channel();h.app.ly.rx=Some(rx);
    let (native,rx)=std::sync::mpsc::channel();h.app.ly.native_rx=Some(rx);
    h.frame(vec![]);
    assert!(native.send(None).is_err(),"关闭歌词后应停止本机轮询");
    assert!(network.send(Err(crate::lyrics::FetchError::NotFound)).is_err());
    assert!(h.app.ly.key.is_empty(),"再次开启时应能重新加载歌词");
}

#[test]
fn late_native_lyrics_bypass_pending_network_for_both_clients(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for source in ["KuGou.exe","SodaMusic.exe"] {
        let mut h=H::new();
        h.media=crate::smtc::NowPlaying{source:source.into(),title:"测试歌曲".into(),artist:"歌手".into(),..Default::default()};
        h.frame(vec![]);h.app.opts.fetch_lyrics=true;h.app.show_lyrics=true;
        h.app.ly.key="测试歌曲|歌手|0".into();h.app.ly.loading=true;
        let (network,rx)=std::sync::mpsc::channel();h.app.ly.rx=Some(rx);
        let (native,rx)=std::sync::mpsc::channel();h.app.ly.native_rx=Some(rx);
        native.send(Some(crate::lyrics::parse_lrc("[00:01.00]本机歌词"))).unwrap();h.frame(vec![]);
        assert!(!h.app.ly.loading,"本机歌词不能等待在线请求结束");
        assert_eq!(h.app.ly.lyrics.as_ref().unwrap().lines[0].text,"本机歌词");
        assert!(network.send(Err(crate::lyrics::FetchError::NotFound)).is_err(),"迟到的网络结果必须丢弃");
    }
}

#[test]
fn kugou_fallback_waits_for_late_native_lyrics(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.media=crate::smtc::NowPlaying{source:"kugou".into(),title:"测试歌曲".into(),artist:"歌手".into(),position:22.8,..Default::default()};
    h.frame(vec![]);h.app.opts.fetch_lyrics=true;h.app.show_lyrics=true;h.app.ly.key="测试歌曲|歌手|0".into();
    let (tx,rx)=std::sync::mpsc::channel();h.app.ly.rx=Some(rx);
    tx.send(Ok((crate::lyrics::parse_lrc("[00:23.930]测试歌词"),true))).unwrap();h.frame(vec![]);
    assert!(h.app.ly.retry_at.is_some(),"临时通用歌词必须继续等待迟到的酷狗原生歌词");
    h.t=h.app.ly.retry_at.unwrap()+0.1;h.frame(vec![]);
    assert_eq!(h.app.ly.lyrics.as_ref().unwrap().lines[0].t,23.930,"接回原生歌词期间不能清空现有歌词");assert!(!h.app.ly.loading);
    let (tx,rx)=std::sync::mpsc::channel();h.app.ly.rx=Some(rx);
    tx.send(Ok((crate::lyrics::parse_lrc("[00:22.991]测试歌词"),false))).unwrap();h.frame(vec![]);
    assert!((h.app.ly.lyrics.as_ref().unwrap().lines[0].t-22.991).abs()<0.001);assert!(h.app.ly.retry_at.is_none(),"接回原生歌词后应停止重查");
}

#[test]
fn kugou_missing_lyrics_rechecks_late_cache_without_flashing_loading() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.media=crate::smtc::NowPlaying{source:"kugou".into(),title:"不存在的测试歌曲".into(),artist:"测试歌手".into(),duration:120.0,..Default::default()};
    h.frame(vec![]);h.app.opts.fetch_lyrics=true;h.app.show_lyrics=true;
    h.app.ly.key="不存在的测试歌曲|测试歌手|120".into();
    let (tx,rx)=std::sync::mpsc::channel();h.app.ly.rx=Some(rx);
    tx.send(Err(crate::lyrics::FetchError::NotFound)).unwrap();h.frame(vec![]);
    let retry=h.app.ly.retry_at.expect("酷狗尚未写入缓存时仍需自动检查");
    assert!((retry-h.t-5.0).abs()<0.001);
    // 歌词通道全进程只有 1 个名额，前面测试的歌词线程可能还在查网络；占着时这一帧会直接跳过。
    let until=std::time::Instant::now()+std::time::Duration::from_secs(20);
    while crate::resources::LYRICS.active()>0 && std::time::Instant::now()<until{std::thread::sleep(std::time::Duration::from_millis(10));}
    assert_eq!(crate::resources::LYRICS.active(),0,"歌词名额被其他测试长期占用");
    h.t=retry+0.1;h.frame(vec![]);
    assert!(!h.app.ly.loading,"检查本机缓存不能反复闪出加载提示");
    assert_eq!(h.app.ly.error,Some(crate::lyrics::FetchError::NotFound));
    let (tx,rx)=std::sync::mpsc::channel();h.app.ly.rx=Some(rx);
    tx.send(Ok((crate::lyrics::parse_lrc("[00:01.00]迟到的原生歌词"),false))).unwrap();h.frame(vec![]);
    assert!(h.app.ly.lyrics.is_some());assert!(h.app.ly.error.is_none());assert!(h.app.ly.retry_at.is_none());
}

#[test]
fn music_lyrics_island_toggle_is_independent_and_persistent() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.app.show_page(crate::app::MUSIC_PAGE);
    h.app.show_lyrics=false;h.app.settings.lyrics_enabled=false;
    h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),position:7.0,duration:120.0,..Default::default()};
    h.app.set_lyrics("Track","Artist",120.0,crate::lyrics::parse_lrc("[00:05]当前歌词\n[00:10]下一句"));
    h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);
    h.click(H::local(313.0,h.app.music_control_y()));
    assert!(h.app.settings.island_lyrics);
    assert!(!h.app.show_lyrics && !h.app.settings.lyrics_enabled);
    let saved=crate::model::Settings::load();
    assert!(saved.island_lyrics && !saved.lyrics_enabled);
    h.hover(Pos2::new(WIN_W/2.0,h.app.expanded_height()+70.0),5.0);
    assert!(!h.app.open);
    assert_eq!(h.app.island_lyric().map(|(_,line)|line),Some("当前歌词"));
    let pet_width=0.0;
    assert!((h.app.current_width()-crate::app::LYRIC_BAR_W-pet_width).abs()<2.0);
}

#[test]
fn music_lyric_timing_follows_saved_delay_and_preserves_playback_position(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.app.show_page(crate::app::MUSIC_PAGE);
    h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),position:9.8,duration:120.0,..Default::default()};
    h.app.set_lyrics("Track","Artist",120.0,crate::lyrics::parse_lrc("[00:00]第一句\n[00:05]当前歌词\n[00:10]下一句"));
    h.frame(vec![]);
    assert_eq!(h.app.settings.lyric_delay_ms,-350,"未设置时应保留原有提前 0.35 秒的预设");
    assert!((h.app.lyric_position()-10.15).abs()<0.001,"默认应提前 0.35 秒显示，且只应用一次");
    for (delay,position,expected) in [(500,10.2,Some("当前歌词")),(-500,9.8,Some("下一句")),(0,9.8,Some("当前歌词")),(500,0.2,None),(500,0.6,Some("第一句"))]{
        std::fs::write(crate::model::config_dir().join("settings.txt"),format!("lyric_delay_ms={delay}\nisland_lyrics=1\n")).unwrap();
        h.app.settings=crate::model::Settings::load();
        h.media.position=position;h.frame(vec![]);
        assert_eq!(h.app.island_lyric().map(|(_,line)|line),expected,"延时 {delay} ms，播放位置 {position}");
        assert!((h.app.media.position-position).abs()<0.001,"调整歌词不应改变播放进度");
        h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);
        for text in ["第一句","当前歌词","下一句"]{
            let active=h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(t) if t.galley.text()==text && t.galley.job.sections[0].format.font_id.size==12.0));
            assert_eq!(active,expected==Some(text),"展开歌词也应使用相同的显示时间");
        }
        h.app.settings.save();
        assert_eq!(crate::model::Settings::load().lyric_delay_ms,delay,"重启后应保留时间调整");
    }
    for (input,expected) in [("invalid",-350),("999999",5000),("-999999",-5000),("",-350)]{
        std::fs::write(crate::model::config_dir().join("settings.txt"),format!("lyric_delay_ms={input}\n")).unwrap();
        assert_eq!(crate::model::Settings::load().lyric_delay_ms,expected);
    }
}

#[test]
fn music_lyric_timing_controls_save_and_fit_the_small_settings_window(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.app.settings_tab=1;h.app.open_settings();h.ctx.set_embed_viewports(false);
    fn frame(h:&mut H,events:Vec<egui::Event>)->Vec<egui::epaint::ClippedShape>{
        let output=std::sync::Arc::new(Mutex::new(None));let capture=output.clone();
        egui::Context::set_immediate_viewport_renderer(move|ctx,mut viewport|{
            let id=viewport.ids.this;
            let raw=egui::RawInput{viewport_id:id,screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(600.0,450.0))),
                viewports:std::iter::once((id,egui::ViewportInfo{native_pixels_per_point:Some(1.25),focused:Some(true),..Default::default()})).collect(),
                focused:true,events:events.clone(),..Default::default()};
            *capture.lock().unwrap()=Some(ctx.run(raw,|ctx|(viewport.viewport_ui_cb)(ctx)));
        });
        let raw=egui::RawInput{viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(1.25),..Default::default()})).collect(),..Default::default()};
        let _=h.ctx.run(raw,|ctx|h.app.settings_ui(ctx,false));
        let child=output.lock().unwrap().take().unwrap();child.shapes
    }
    let text_rect=|shapes:&[egui::epaint::ClippedShape],text:&str|shapes.iter().find_map(|shape|match &shape.shape{
        egui::Shape::Text(t)if t.galley.text()==text=>Some(t.visual_bounding_rect()),_=>None,
    }).unwrap_or_else(||panic!("设置中缺少 {text}；当前文字：{:?}",shapes.iter().filter_map(|shape|match &shape.shape{egui::Shape::Text(t)=>Some(t.galley.text()),_=>None}).collect::<Vec<_>>()));
    for _ in 0..3{frame(&mut h,vec![]);}
    // Account and player settings precede lyric timing. Reach the controls
    // through the same scrollable pane used in the compact settings window.
    for _ in 0..6{frame(&mut h,vec![egui::Event::PointerMoved(egui::pos2(450.0,300.0)),egui::Event::MouseWheel{unit:egui::MouseWheelUnit::Point,delta:egui::vec2(0.0,-180.0),modifiers:Default::default()}]);}
    for _ in 0..8{frame(&mut h,vec![]);}
    let music=frame(&mut h,vec![]);
    text_rect(&music,"0.35 秒");
    assert_eq!(h.app.settings.lyric_delay_ms,-350,"仅打开设置不能将 0.35 秒舍入为 0.4 秒");
    for button in ["−","+"]{text_rect(&music,button);}
    h.app.settings_tab=1;
    for _ in 0..3{frame(&mut h,vec![]);}
    for (button,expected) in [("+",-450),("−",-350),("−",-250),("+",-350)]{
        let shapes=frame(&mut h,vec![]);
        let rect=text_rect(&shapes,button);
        assert!(Rect::from_min_max(egui::pos2(180.0,0.0),egui::pos2(600.0,450.0)).contains_rect(rect),"时间调整按钮应完整显示");
        let pos=rect.center();
        frame(&mut h,vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed:true,modifiers:Default::default()}]);
        frame(&mut h,vec![egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed:false,modifiers:Default::default()}]);
        assert_eq!(h.app.settings.lyric_delay_ms,expected);
        assert_eq!(crate::model::Settings::load().lyric_delay_ms,expected,"按钮调整后应自动保存");
        for _ in 0..2{frame(&mut h,vec![egui::Event::PointerMoved(egui::pos2(450.0,300.0)),egui::Event::MouseWheel{unit:egui::MouseWheelUnit::Point,delta:egui::vec2(0.0,-180.0),modifiers:Default::default()}]);}
        for _ in 0..8{frame(&mut h,vec![]);}
        text_rect(&frame(&mut h,vec![]),&format!("{:.2} 秒",-expected as f64/1000.0));
    }
}

#[test]
fn island_lyrics_arrive_just_before_vocals_and_finish_fading_before_the_timestamp(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.app.settings.island_lyrics=true;
    h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),position:9.4,duration:120.0,..Default::default()};
    h.app.set_lyrics("Track","Artist",120.0,crate::lyrics::parse_lrc("[00:05]当前歌词\n[00:10]下一句"));
    for _ in 0..60 {h.frame(vec![]);}
    h.media.playing=true;
    for step in 1..=30 {
        h.media.position=9.4+step as f64/60.0;
        h.frame(vec![]);
        if step==6 {assert_eq!(h.app.island_lyric().map(|(_,line)|line),Some("当前歌词"),"不能提前半秒就跳到下一句");}
    }
    assert_eq!(h.app.island_lyric().map(|(_,line)|line),Some("下一句"),"下一句应在开唱前提前出现");
    let text=h.shapes.iter().find_map(|shape|match &shape.shape {
        egui::Shape::Text(text) if text.galley.text()=="下一句"=>Some(text),_=>None,
    }).expect("开唱前应能看到下一句");
    assert!(text.opacity_factor>0.95,"开唱前应完成淡入，不能等到开唱后才清晰");
    h.media.playing=false;h.frame(vec![]);
    assert_eq!(h.app.island_lyric().map(|(_,line)|line),Some("下一句"),"暂停时不应跳回上一句");
    h.media.position=4.5;h.frame(vec![]);
    assert!(h.app.island_lyric().is_none(),"前奏不能过早显示第一句");
    h.media.position=4.75;h.frame(vec![]);
    assert_eq!(h.app.island_lyric().map(|(_,line)|line),Some("当前歌词"),"第一句也应提前出现");
}

#[test]
fn expanded_and_segmented_lyrics_share_the_early_display_time(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.app.show_page(crate::app::MUSIC_PAGE);h.app.settings.island_lyrics=true;
    h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),position:9.8,duration:120.0,..Default::default()};
    h.app.set_lyrics("Track","Artist",120.0,crate::lyrics::parse_lrc("[00:05]当前歌词\n[00:10]下一句"));
    h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);
    assert!(h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(text)if text.galley.text()=="下一句"&&text.galley.job.sections[0].format.font_id.size==12.0)),"展开歌词也应在开唱前高亮下一句");
    h.hover(Pos2::new(WIN_W/2.0,h.app.expanded_height()+70.0),1.0);
    let line="愿每一个认真生活的人，都能在平凡的日子里找到属于自己的光";
    h.media.position=6.0;h.app.set_lyrics("Track","Artist",120.0,crate::lyrics::parse_lrc(&format!("[00:05]{line}\n[00:15]下一句")));
    for _ in 0..60{h.frame(vec![]);}
    let shown=|h:&H|h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t)if line.contains(t.galley.text())&&!t.galley.text().is_empty()=>Some(t.galley.text().to_owned()),_=>None}).unwrap();
    let first=shown(&h);
    let boundary=5.0+10.0*first.chars().count()as f64/line.chars().count()as f64;
    h.media.position=boundary-0.2;for _ in 0..30{h.frame(vec![]);}
    assert_ne!(shown(&h),first,"长句的后半段也应使用相同的提前量");
}

#[test]
fn lyric_short_gap_and_line_switch_keep_text_visible(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.app.settings.island_lyrics=true;
    h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),position:2.0,..Default::default()};
    h.app.set_lyrics("Track","Artist",0.0,crate::lyrics::parse_lrc("[00:01]上一句\n[00:05]下一句"));
    for _ in 0..60{h.frame(vec![]);}
    for (position,expected) in [(3.0,"上一句"),(4.64,"上一句"),(4.7,"下一句"),(4.72,"下一句"),(4.9,"下一句")]{
        h.media.position=position;h.frame(vec![]);
        let text=h.shapes.iter().find_map(|shape|match &shape.shape{
            egui::Shape::Text(text)if text.galley.text()==expected=>Some(text),_=>None,
        }).expect("短停顿及换句时必须持续显示歌词");
        assert!(text.opacity_factor>0.95,"换句不能淡出歌词，位置 {position}，透明度 {}",text.opacity_factor);
    }
}

#[test]
fn island_lyrics_center_segments_in_a_fixed_height(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.app.settings.island_lyrics=true;
    h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),position:7.0,duration:120.0,..Default::default()};
    for line in ["晚风","愿每一个认真生活的人，都能在平凡的日子里找到属于自己的光","晚风"] {
        h.app.set_lyrics("Track","Artist",120.0,crate::lyrics::parse_lrc(&format!("[00:05]{line}\n[00:15]下一句")));
        for _ in 0..90 {
            h.frame(vec![]);
            assert!(!h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="09:41")),"切换显示段时应保持歌词布局");
        }
        let text=h.shapes.iter().find_map(|shape|match &shape.shape {
            egui::Shape::Text(text) if !text.galley.text().is_empty() && line.contains(text.galley.text())=>Some(text),_=>None,
        }).expect("岛上应显示当前歌词的一段");
        let center=text.pos.x+text.galley.rect.center().x;
        // The pet/cover uses 64pt on the left; the wider Agent group uses
        // 90pt on the right. Center in the remaining lyric lane.
        let offset=(64.0-90.0)/2.0;
        assert!((center-WIN_W/2.0-offset).abs()<1.5,"歌词应在内容区居中: center={center}, pos={:?}, rect={:?}, text={}",text.pos,text.galley.rect,text.galley.text());
        assert!(!text.galley.elided,"显示段应完整显示");
        assert_eq!(text.galley.rows.len(),1,"每次只显示一段歌词");
        assert!((h.hit.height()-crate::app::BAR_H).abs()<0.1,"岛应保持固定高度");
        assert!(text.visual_bounding_rect().bottom()<=h.hit.bottom(),"文字应完整显示在岛内");
    }
}

#[test]
fn island_lyric_segments_follow_playback_pause_and_seek(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.app.settings.island_lyrics=true;
    h.app.settings.pet=false;h.app.settings.visible_pages[crate::app::AGENT_PAGE]=false;
    let line="愿每一个认真生活的人，都能在平凡的日子里找到属于自己的光";
    h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),position:6.0,duration:120.0,..Default::default()};
    h.app.set_lyrics("Track","Artist",120.0,crate::lyrics::parse_lrc(&format!("[00:05]{line}\n[00:15]下一句")));
    let shown=|h:&H|h.shapes.iter().find_map(|s|match &s.shape {egui::Shape::Text(t)=>Some(t.galley.text().to_owned()),_=>None}).unwrap();
    for _ in 0..90 {h.frame(vec![]);}let first=shown(&h);
    h.media.position=14.0;for _ in 0..30 {h.frame(vec![]);}let last=shown(&h);
    assert_ne!(first,last,"播放到后半句时应切换显示段");
    assert_eq!(format!("{first}{last}"),line,"前后显示段应覆盖完整歌词");
    for _ in 0..120 {h.frame(vec![]);}assert_eq!(shown(&h),last,"暂停时显示段应保持不动");
    h.media.position=6.0;for _ in 0..30 {h.frame(vec![]);}assert_eq!(shown(&h),first,"拖回前半句应恢复对应显示段");
    h.media.position=16.0;for _ in 0..30 {h.frame(vec![]);}assert_eq!(shown(&h),"下一句");
}

#[test]
fn island_lyrics_keep_title_fallback_and_intro_and_gaps(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.app.settings.minimal=true;h.app.settings.island_lyrics=true;
    h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),position:1.0,duration:120.0,..Default::default()};
    for state in 0..6 {
        h.app.ly=Default::default();
        h.app.ly.key="Track|Artist|120".into();
        h.media.position=if state==1 {7.0}else{1.0};
        match state {
            0=>h.app.ly.lyrics=Some(crate::lyrics::parse_lrc("[00:05]第一句")),
            1=>h.app.ly.lyrics=Some(crate::lyrics::parse_lrc("[00:01]第一句\n[00:05]\n[00:10]下一句")),
            2=>h.app.ly.loading=true,
            3=>h.app.ly.error=Some(crate::lyrics::FetchError::NotFound),
            4=>h.app.ly.error=Some(crate::lyrics::FetchError::Unavailable("503".into())),
            _=>h.app.ly.lyrics=Some(crate::lyrics::from_plain("没有时间戳")),
        }
        for _ in 0..90 {h.frame(vec![]);}
        if let Some(error)=&h.app.ly.error {
            assert!(!h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()==error.label())),"歌词错误提示只应在展开内显示");
        }
        assert!((h.app.current_width()-crate::app::LYRIC_BAR_W).abs()<2.0,"状态 {state} 应保持音乐信息岛宽度");
        assert!(h.region.height()>30.0);
        if state>=2 {
            assert!(h.app.compact_notices(),"歌名兜底仍属于岛上歌词模式");
            assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="Track"&&t.opacity_factor>0.95)),"状态 {state} 应显示当前歌名");
        } else {
            let label=if state==0{"前奏"}else{"间奏"};
            assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()==label)),"同步歌词的前奏和间奏仍应显示状态");
        }
    }
}

#[test]
fn island_lyrics_title_fallback_shows_cover_and_elides_long_names(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.app.settings.minimal=true;h.app.settings.island_lyrics=true;
    h.media=crate::smtc::NowPlaying{title:"没有歌词的钢琴曲".repeat(20),artist:"示例演奏者".into(),artwork:Some(std::sync::Arc::new(egui::ColorImage::new([8,8],egui::Color32::BLUE))),..Default::default()};
    for _ in 0..90 {h.frame(vec![]);}
    let text=h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t)if t.galley.text()==h.media.title=>Some(t),_=>None}).expect("无歌词时应绘制当前歌名");
    assert_eq!(text.galley.rows.len(),1,"长歌名只显示一行");
    assert!(text.galley.elided,"长歌名应在末尾省略");
    assert!(h.region.contains_rect(text.visual_bounding_rect()),"长歌名不能超出岛体");
    let cover=h.app.music_artwork.as_ref().unwrap().1.id();
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Rect(r)if r.brush.as_ref().is_some_and(|b|b.fill_texture_id==cover)&&h.region.contains_rect(r.rect))),"无歌词时仍应绘制封面");
}

#[test]
fn island_lyrics_title_fallback_follows_track_changes_and_normal_mode(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.app.settings.minimal=true;h.app.settings.island_lyrics=true;
    h.media=crate::smtc::NowPlaying{title:"上一首".into(),artist:"示例歌手".into(),position:7.0,duration:120.0,playing:true,..Default::default()};
    h.app.set_lyrics("上一首","示例歌手",120.0,crate::lyrics::parse_lrc("[00:05]上一首的歌词"));
    for _ in 0..90 {h.frame(vec![]);}
    h.media.title="下一首纯音乐".into();h.media.playing=false;
    for _ in 0..90 {h.frame(vec![]);}
    assert!(h.app.island_lyric().is_none(),"换歌后不能显示上一首歌词");
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()==h.media.title)),"暂停或查词中仍应显示新歌名");
    h.app.set_lyrics("下一首纯音乐","示例歌手",120.0,crate::lyrics::parse_lrc("[00:05]新歌的当前歌词"));
    for _ in 0..30 {h.frame(vec![]);}
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()=="新歌的当前歌词")),"歌词到达后应自动切回歌词");
    for mode in 0..3 {
        h.app.settings.island_lyrics=mode!=0;
        h.app.settings.visible_pages[crate::app::MUSIC_PAGE]=mode!=1;
        if mode==2 {h.media=Default::default();}
        for _ in 0..90 {h.frame(vec![]);}
        assert!(h.region.height()<20.0,"关闭歌词、隐藏音乐页或播放器退出后应恢复极简常驻，状态 {mode}");
        assert!(!h.app.compact_notices());
    }
}

#[test]
#[ignore="按需导出岛上歌词的实际渲染画面"]
fn island_lyrics_layout_gallery(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let dest=std::path::PathBuf::from(std::env::var_os("QINGYU_LYRIC_PREVIEW_DIR").expect("需指定预览目录"));
    std::fs::create_dir_all(&dest).unwrap();
    let mut cover=egui::ColorImage::new([64,64],egui::Color32::BLACK);
    for y in 0..64 {for x in 0..64 {
        let moon=(x as i32-44).pow(2)+(y as i32-18).pow(2)<64;
        cover[(x,y)]=if moon {egui::Color32::from_rgb(255,226,174)}
            else if y>44+x/5 {egui::Color32::from_rgb(29,61,70)}
            else {egui::Color32::from_rgb(68+y as u8,114+y as u8,151+y as u8/2)};
    }}
    let cover=std::sync::Arc::new(cover);
    for name in ["lyrics","loading","missing","no-cover","long-title","paused","intro","gap","idle"] {
        let mut h=H::new();h.app.settings.island_lyrics=true;
        h.media=crate::smtc::NowPlaying{title:"晚风与星光".into(),artist:"示例歌手".into(),position:7.0,duration:120.0,playing:name!="paused",artwork:Some(cover.clone()),..Default::default()};
        if name=="long-title" {h.media.title="愿每一个认真生活的人都能在平凡的日子里找到属于自己的光（钢琴独奏版）".into();}
        h.app.ly.key=format!("{}|{}|120",h.media.title,h.media.artist);
        match name {
            "lyrics"=>h.app.set_lyrics("晚风与星光","示例歌手",120.0,crate::lyrics::parse_lrc("[00:05]愿你在平凡的日子里，找到自己的光")),
            "loading"=>h.app.ly.loading=true,
            "intro"=>h.app.set_lyrics("晚风与星光","示例歌手",120.0,crate::lyrics::parse_lrc("[00:20]晚风")),
            "gap"=>h.app.set_lyrics("晚风与星光","示例歌手",120.0,crate::lyrics::parse_lrc("[00:01]第一句\n[00:05]\n[00:10]下一句")),
            "idle"=>h.media=Default::default(),
            _=>h.app.ly.error=Some(crate::lyrics::FetchError::NotFound),
        }
        if name=="no-cover" {h.media.artwork=None;}
        let mut raster=crate::preview_raster::Raster::new(1280,540);
        for frame in 0..90 {
            let t=frame as f64/60.0;
            let env=Env{t,clock:"09:41:00".into(),today:"2026-11-06".into(),date:"11月6日 周五".into(),media:h.media.clone(),..Default::default()};
            let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(WIN_W,WIN_H))),time:Some(t),
                viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(2.0),..Default::default()})).collect(),..Default::default()};
            let output=h.ctx.run(raw,|ctx|{
                egui::CentralPanel::default().frame(egui::Frame::new()).show(ctx,|ui|{let win=ui.max_rect();h.app.frame(ui,win,&env);});
            });
            raster.update_textures(&output.textures_delta);
            if frame==89 {
                raster.buf.fill([0.95,0.96,0.98,1.0]);
                raster.draw(&h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);
                let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
                let image=image::RgbaImage::from_raw(1280,540,rgba).unwrap();
                image::imageops::crop_imm(&image,220,0,840,110).to_image().save(dest.join(format!("{name}.png"))).unwrap();
            }
            raster.free_textures(&output.textures_delta);
        }
    }
}

#[test]
fn music_lyrics_keep_syncing_when_only_the_collapsed_lyrics_are_enabled() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    h.app.opts.fetch_lyrics=true;h.app.show_lyrics=false;h.app.settings.island_lyrics=true;
    h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),position:7.0,duration:120.0,..Default::default()};
    let (tx,rx)=std::sync::mpsc::channel();
    h.app.ly.key="Track|Artist|120".into();h.app.ly.loading=true;h.app.ly.rx=Some(rx);
    tx.send(Ok((crate::lyrics::parse_lrc("[00:05]当前歌词\n[00:10]下一句"),false))).unwrap();
    h.frame(vec![]);
    assert!(!h.app.ly.loading);
    assert_eq!(h.app.island_lyric().map(|(_,line)|line),Some("当前歌词"));
    h.media.position=12.0;h.frame(vec![]);
    assert_eq!(h.app.island_lyric().map(|(_,line)|line),Some("下一句"));
}

#[test]
fn music_lyrics_respect_the_switch_and_do_not_show_stale_or_unsynced_lines() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    h.app.settings.minimal=true;h.app.settings.island_lyrics=true;
    h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),position:7.0,duration:120.0,..Default::default()};
    h.app.set_lyrics("Track","Artist",120.0,crate::lyrics::parse_lrc("[00:05]当前歌词"));
    for _ in 0..90 {h.frame(vec![]);}
    assert!(h.region.height()>30.0,"主动开启岛上歌词后应可在收起状态阅读");
    h.app.settings.island_lyrics=false;
    for _ in 0..90 {h.frame(vec![]);}
    assert!(h.app.island_lyric().is_none() && h.region.height()<20.0);
    h.app.settings.island_lyrics=true;
    h.media.title="Another track".into();h.frame(vec![]);
    assert!(h.app.island_lyric().is_none(),"换歌后不能显示上一首的歌词");
    h.app.set_lyrics("Another track","Artist",120.0,crate::lyrics::from_plain("没有时间戳的歌词"));h.frame(vec![]);
    assert!(h.app.island_lyric().is_none(),"不能将纯文本歌词伪装成当前句");
}

#[test]
fn hover_expands_with_overshoot_and_leave_collapses() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut h = H::new();
    h.hover(Pos2::new(WIN_W / 2.0, 12.0), 0.1);
    assert!(h.app.progress() < 0.1, "意图缓冲期内不应展开");
    let mut peak: f32 = 0.0;
    for _ in 0..60 {
        h.hover(Pos2::new(WIN_W / 2.0, 12.0), 1.0 / 60.0);
        peak = peak.max(h.app.progress());
    }
    assert!(peak > 1.01 && peak < 1.12, "展开应有轻微过冲, peak={peak}");
    assert!((h.app.progress() - 1.0).abs() < 0.02);
    h.cursor = Some(Pos2::new(WIN_W / 2.0, h.app.expanded_height() + 70.0));
    for _ in 0..65 {
        h.frame(vec![]);
    }
    assert!(h.app.progress() < 0.02, "移开后应收起");
}

#[test]
fn browser_minimal_preserves_preferences_and_restores_the_previous_mode(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.app.settings.minimal=false;h.browser_at_top=Some(true);
    for _ in 0..90 {h.frame(vec![]);}
    assert!(h.hit.height()>20.0,"开关默认关闭时不应自动改变模式");
    h.app.settings.browser_minimal=true;h.app.settings.save();h.app.settings.island_lyrics=true;
    h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),position:7.0,duration:120.0,..Default::default()};
    h.app.set_lyrics("Track","Artist",120.0,crate::lyrics::parse_lrc("[00:05]当前歌词"));
    for _ in 0..90 {h.frame(vec![]);}
    assert!(h.hit.height()<10.0,"浏览器顶部受遮挡时应收为细刘海，歌词不能撑大它");
    assert!(!h.app.settings.minimal,"自动极简不能修改手动模式");
    let saved=crate::model::Settings::load();
    assert!(saved.browser_minimal && !saved.minimal,"应只持久化开关，保留原模式");
    h.browser_at_top=None;for _ in 0..30 {h.frame(vec![]);}
    assert!(h.hit.height()<10.0,"打开轻屿设置或托盘时应保留浏览器避让状态");
    h.browser_at_top=Some(false);for _ in 0..90 {h.frame(vec![]);}
    assert!(h.hit.height()>20.0,"离开浏览器或还原窗口后应恢复");
    h.browser_at_top=Some(true);for _ in 0..90 {h.frame(vec![]);}
    h.app.settings.browser_minimal=false;for _ in 0..90 {h.frame(vec![]);}
    assert!(h.hit.height()>20.0,"关闭开关后应立即恢复");
    h.app.settings.island_lyrics=false;h.app.set_minimal(true);h.browser_at_top=Some(false);for _ in 0..90 {h.frame(vec![]);}
    assert!(h.app.settings.minimal && h.hit.height()<10.0,"离开浏览器不能撤销手动极简");
}

#[test]
fn browser_minimal_closes_the_open_island_but_still_allows_deliberate_hover(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.settings.browser_minimal=true;
    h.app.show_page(crate::app::MUSIC_PAGE);h.hover(H::local(300.0,60.0),0.8);
    assert!(h.app.open);
    h.browser_at_top=Some(true);h.frame(vec![]);
    assert!(!h.app.open,"切到浏览器时不能继续保留展开状态遮挡标签栏");
    assert!(!h.hit.contains(Pos2::new(WIN_W/2.0,12.0)),"收起动画期间也应让标签栏接收点击");
    h.hover(Pos2::new(WIN_W/2.0,2.0),0.25);
    assert!(!h.app.open,"短暂碰到刘海不能误触");
    h.hover(Pos2::new(WIN_W/2.0,2.0),0.6);
    assert!(h.app.open,"仍应能主动悬停打开岛");
}

#[test]
fn minimal_hover_excludes_the_surrounding_browser_tabs() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    h.app.settings.minimal=true;
    for _ in 0..90 {h.frame(vec![]);}
    for cursor in [
        Pos2::new(WIN_W/2.0,crate::app::BAR_H_MIN+1.0),
        Pos2::new(WIN_W/2.0+crate::app::BAR_W_MIN/2.0+2.0,2.0),
        Pos2::new(WIN_W/2.0-crate::app::BAR_W_MIN/2.0-2.0,2.0),
    ] {
        assert!(!h.hit.contains(cursor),"刘海外的标签栏应接收鼠标操作");
        h.hover(cursor,1.0);
        assert!(!h.app.open,"鼠标未触碰极简刘海时不得展开");
    }
}

#[test]
fn minimal_hover_requires_continuous_deliberate_contact() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    h.app.settings.minimal=true;
    for _ in 0..90 {h.frame(vec![]);}
    let inside=Pos2::new(WIN_W/2.0,2.0);
    h.hover(inside,0.25);
    assert!(!h.app.open,"短暂经过极简刘海不得展开");
    h.hover(Pos2::new(WIN_W/2.0,8.0),0.1);
    h.hover(inside,0.25);
    assert!(!h.app.open,"移出刘海应重新计算停留时间");
    h.hover(inside,0.7);
    assert!(h.app.open,"在刘海内持续停留仍应展开");
    let entry=h.ctx.read_response(egui::Id::new(("card",true,("control-module",crate::app::SHELF_PAGE)))).unwrap().rect.center();h.click(entry);
    assert_eq!(h.app.page,1,"展开后应能正常操作面板");
}

#[test]
fn minimal_hover_does_not_catch_tabs_during_collapse() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    h.app.settings.minimal=true;
    h.hover(Pos2::new(WIN_W/2.0,2.0),1.0);
    assert!(h.app.open);
    h.cursor=Some(Pos2::new(WIN_W/2.0,h.app.expanded_height()+70.0));
    for _ in 0..35 {
        h.frame(vec![]);
        if !h.app.open {break;}
    }
    assert!(!h.app.open);
    assert!(h.app.progress()>0.04,"检查收起动画仍在进行时的命中范围");
    let tab=Pos2::new(WIN_W/2.0,12.0);
    assert!(h.hit_shapes.iter().all(|shape|!crate::shape::contains_outline(&shape.outline(),tab)),"收起动画不得继续拦截标签栏");
    h.hover(tab,1.0);
    assert!(!h.app.open);
}

#[test]
fn music_header_shortcut_returns_from_every_page_without_controlling_playback() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for playing in [true,false] {
        let mut h=H::new();h.app.settings.page_order=[3,2,0,1,6,4,5,7,8];
        h.media=crate::smtc::NowPlaying{source:"cloudmusic.exe".into(),title:"Track".into(),artist:"Artist".into(),playing,duration:120.0,..Default::default()};
        for page in 1..crate::app::PAGE_NAMES.len() {
            h.app.show_page(page);h.hover(H::local(300.0,60.0),0.8);h.cmds.clear();
            h.click_control("header-music");
            assert_eq!(h.app.page,crate::app::MUSIC_PAGE,"播放状态 {playing} 时应从页面 {page} 直达音乐页");
            assert!(h.app.open);assert!(!h.app.settings_open);assert_eq!(h.app.media.playing,playing);
            assert!(h.cmds.iter().all(|cmd|!matches!(cmd,crate::app::Cmd::Media(_)|crate::app::Cmd::MusicVolume(..))),"返回音乐页不能改变播放或音量");
        }
    }
}

#[test]
fn music_header_shortcut_keeps_hidden_music_page_disabled() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),playing:true,duration:120.0,..Default::default()};
    h.app.set_page_visible(crate::app::MUSIC_PAGE,false);h.app.show_page(crate::app::TODO_PAGE);h.hover(H::local(300.0,60.0),0.8);
    h.click_control("header-music");
    assert_eq!(h.app.page,crate::app::TODO_PAGE);assert!(!h.app.settings.visible_pages[crate::app::MUSIC_PAGE]);
    assert!(!h.app.settings_open,"隐藏模块的快捷入口不得隐式打开设置");
    h.click_control("page-settings");assert!(h.app.settings_open);assert_eq!(h.app.settings_tab,crate::app::PAGE_SETTINGS_TABS[crate::app::TODO_PAGE]);
}

#[test]
fn music_header_shortcut_is_absent_without_music_and_keeps_settings_clickable() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for media in [crate::smtc::NowPlaying::default(),crate::smtc::NowPlaying{title:"Video".into(),artist:"Channel".into(),playing:true,playback_type:crate::smtc::MediaKind::Video,..Default::default()}] {
        let mut h=H::new();h.media=media;h.app.show_page(crate::app::TODO_PAGE);h.hover(H::local(300.0,60.0),0.8);
        h.click(H::local(367.0,18.0));assert_eq!(h.app.page,crate::app::TODO_PAGE);assert!(!h.app.settings_open);
        h.click(H::local(394.0,18.0));assert!(h.app.settings_open);assert_eq!(h.app.settings_tab,4);
    }
}

#[test]
fn module_tabs_and_main_return_replace_side_arrows() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.show_page(crate::app::MUSIC_PAGE);h.hover(H::local(210.0,18.0),1.0);
    for page in [crate::app::SHELF_PAGE,crate::app::MUSIC_PAGE,crate::app::CLIPBOARD_PAGE] {
        h.click_module(page);assert_eq!(h.app.page,page);assert!(!h.app.is_control_center());
        h.hover(H::local(210.0,18.0),0.7);
        for id in ["nav-l","nav-r"] {assert!(h.ctx.read_response(egui::Id::new(("card",true,id))).is_none(),"side navigation arrows were removed");}
    }
    let main=h.ctx.read_response(egui::Id::new(("card",true,"control-center-tab"))).unwrap().rect.center();h.click(main);
    assert!(h.app.is_control_center());
}

#[test]
fn wheel_navigation_reaches_every_page_in_both_directions(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.app.show_page(crate::app::MUSIC_PAGE);h.hover(H::local(210.0,18.0),1.0);
    for (delta,pages) in [(-40.0,vec![Some(1),Some(6),Some(2),Some(3),Some(4),Some(5),Some(7),Some(8),None]),(40.0,vec![Some(8),Some(7),Some(5),Some(4),Some(3),Some(2),Some(6),Some(1),Some(0)])]{
        for expected in pages{
            if h.app.overview{h.page_key(delta<0.0);}else{h.wheel(delta);}
            assert_eq!(h.app.overview,expected.is_none());if let Some(page)=expected{assert_eq!(h.app.page(),page);}
            h.hover(H::local(210.0,18.0),0.25);
        }
    }
}

#[test]
fn wheel_scrolls_lists_only_inside_content_and_respects_edges(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    let capacity=((h.app.content_rect().bottom()-51.0)/22.0).floor()as usize;
    h.app.todos.items=(0..capacity+4).map(|i|crate::model::TodoItem{text:format!("任务 {i}"),..Default::default()}).collect();
    h.app.shelf.items=(0..30).map(|i|std::path::PathBuf::from(format!("scroll-test-{i}.txt"))).collect();
    for page in [crate::app::SHELF_PAGE,crate::app::TODO_PAGE] {
        h.app.show_page(page);h.hover(H::local(300.0,60.0),1.0);h.wheel(-40.0);h.hover(H::local(300.0,60.0),0.25);
        assert_eq!(h.app.page,page,"有更多内容时应先滚动列表");
        if page==crate::app::SHELF_PAGE{assert!(h.shelf_scroll_offset()>0.0);}else{assert_eq!(h.app.todo_first,1);}
        h.wheel(-4000.0);h.hover(H::local(300.0,60.0),0.25);h.wheel(-40.0);
        assert_eq!(h.app.page,page,"内容溢出时滚到底也应留在本页");
        h.app.show_page(page);h.hover(H::local(210.0,18.0),0.3);h.wheel(-40.0);
        assert_eq!(h.app.page,if page==crate::app::SHELF_PAGE{crate::app::CLIPBOARD_PAGE}else{crate::app::MAIL_PAGE},"顶栏滚轮不应被内容列表拦截");
    }
}

#[test]
fn wheel_overflowing_shelf_stays_at_both_edges(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for count in [20,30]{
        let mut h=H::new();h.app.shelf.items=(0..count).map(|i|std::path::PathBuf::from(format!("scroll-test-{i}.txt"))).collect();
        h.app.show_page(crate::app::SHELF_PAGE);h.hover(H::local(300.0,60.0),1.0);
        h.wheel(4000.0);h.hover(H::local(300.0,60.0),0.4);assert!(h.shelf_scroll_offset()<0.1);
        h.wheel(-40.0);h.hover(H::local(300.0,60.0),0.4);assert!(h.shelf_scroll_offset()>0.0);
        h.wheel(-4000.0);h.hover(H::local(300.0,60.0),0.4);let bottom=h.shelf_scroll_offset();
        h.wheel(-4000.0);h.hover(H::local(300.0,60.0),0.4);assert!((h.shelf_scroll_offset()-bottom).abs()<0.1);
        assert_eq!(h.app.page,crate::app::SHELF_PAGE,"置物架溢出后滚到底也应留在本页");
        h.wheel(40.0);h.hover(H::local(300.0,60.0),0.4);assert!(h.shelf_scroll_offset()<bottom);
        h.wheel(4000.0);h.hover(H::local(300.0,60.0),0.4);assert!(h.shelf_scroll_offset()<0.1);
        h.wheel(40.0);assert_eq!(h.app.page,crate::app::SHELF_PAGE,"置物架溢出后滚到顶也应留在本页");
        for (delta,page) in [(-40.0,crate::app::CLIPBOARD_PAGE),(40.0,crate::app::MUSIC_PAGE)]{
            h.click_module(page);assert_eq!(h.app.page,page,"置物架溢出后仍应允许模块入口切页");
            h.app.show_page(crate::app::SHELF_PAGE);h.hover(H::local(210.0,18.0),0.3);h.wheel(delta);
            assert_eq!(h.app.page,page,"置物架顶栏仍应允许滚轮翻页");
            h.app.show_page(crate::app::SHELF_PAGE);h.hover(H::local(300.0,60.0),0.7);
        }
    }
}

#[test]
fn wheel_overflowing_todos_stay_at_both_edges(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    let capacity=((h.app.content_rect().bottom()-51.0)/22.0).floor()as usize;
    for count in [capacity+1,capacity+5]{
        h.app.todos.items=(0..count).map(|i|crate::model::TodoItem{text:format!("任务 {i}"),..Default::default()}).collect();
        h.app.todo_first=0;h.app.show_page(crate::app::TODO_PAGE);
        let last=count-capacity;
        for (delta,first) in [(40.0_f32,0),(-40.0,1),(-400.0,last),(-40.0,last),(40.0,last-1),(400.0,0),(40.0,0)]{
            h.hover(H::local(300.0,60.0),0.3);
            for _ in 0..(delta.abs() as usize/40){h.wheel(delta.signum()*40.0);}
            assert_eq!(h.app.page,crate::app::TODO_PAGE,"待办溢出后滚动到首尾都不应翻页，条数 {count}");
            assert_eq!(h.app.todo_first,first);
        }
        for (delta,page) in [(-40.0,crate::app::MAIL_PAGE),(40.0,crate::app::CLIPBOARD_PAGE)]{
            h.app.show_page(crate::app::TODO_PAGE);h.hover(H::local(100.0,60.0),0.3);h.wheel(delta);
            assert_eq!(h.app.page,page,"待办列表外仍应允许滚轮翻页");
        }
    }
}

#[test]
fn wheel_clipboard_stays_at_both_edges() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    h.app.show_page(crate::app::CLIPBOARD_PAGE);
    
    for count in [crate::pages::CLIP_ROWS+1] {
        h.app.clipboard.lock().items.clear();
        for i in 0..count { h.app.clipboard.lock().push(&i.to_string(), "09:00"); }
        let last=count.saturating_sub(crate::pages::CLIP_ROWS);
        for (delta, first) in [(40.0,0),(-40.0,last),(-40.0,last),(40.0,0),(40.0,0)] {
            h.hover(H::local(300.0,60.0),0.4);
            h.wheel(delta);
            assert_eq!(h.app.page,crate::app::CLIPBOARD_PAGE);
            assert_eq!(h.app.clip_first,first);
        }
        for (delta,page) in [(-40.0,crate::app::TODO_PAGE),(40.0,crate::app::SHELF_PAGE)]{
            h.app.show_page(crate::app::CLIPBOARD_PAGE);h.hover(H::local(210.0,18.0),0.4);h.wheel(delta);
            assert_eq!(h.app.page,page,"剪贴板列表外仍应允许滚轮翻页");
        }
    }
}
#[test]
fn wheel_handles_small_deltas_without_skipping_pages_on_bursts(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.app.show_page(crate::app::MUSIC_PAGE);
    h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);h.hover(H::local(210.0,18.0),0.3);
    for _ in 0..16 {h.wheel(-0.5);}
    assert_eq!(h.app.page,1,"高精度滚轮的小幅输入应能累积成一次翻页");
    h.hover(H::local(210.0,18.0),0.3);
    for _ in 0..5 {h.wheel(-40.0);}
    assert_eq!(h.app.page,crate::app::CLIPBOARD_PAGE,"一次连续滚动不能瞬间跳过多个页面");
}

#[test]
fn navigation_wraps_between_all_pages_and_general_settings_remain_accessible(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.app.show_control_center();h.hover(H::local(210.0,18.0),1.0);
    h.page_key(false);assert_eq!(h.app.page(),crate::app::NOTIFICATIONS_PAGE);assert!(!h.app.overview);
    h.page_key(true);assert!(h.app.overview);h.page_key(true);assert_eq!(h.app.page(),crate::app::MUSIC_PAGE);assert!(!h.app.overview);
    h.app.open_settings_category(3);assert!(h.app.settings_open);assert_eq!(h.app.settings_tab,0);
}

#[test]
fn page_visibility_is_saved_and_sets_the_starting_page() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    assert_eq!(h.app.settings.visible_pages,[true;crate::app::PAGE_COUNT]);
    assert!(!h.app.settings.show_battery);
    h.app.set_page_visible(crate::app::MUSIC_PAGE,false);
    h.app.set_page_visible(crate::app::SHELF_PAGE,false);
    h.app.settings.save();
    let settings=crate::model::Settings::load();
    assert!(!settings.show_battery);
    assert_eq!(settings.visible_pages,[false,false,true,true,true,true,true,true,true]);
    let restarted=App::new(&h.ctx,Options { fetch_lyrics: false, monitor_agents: false });
    assert_eq!(restarted.page(),crate::app::CLIPBOARD_PAGE);
    assert_eq!(restarted.settings.visible_pages,settings.visible_pages);
    assert!(!restarted.settings.show_battery);
    std::fs::write(crate::model::config_dir().join("settings.txt"),"visible_pages=000000\n").unwrap();
    assert_eq!(crate::model::Settings::load().visible_pages,[true,false,false,false,false,false,false,false,false]);
    std::fs::write(crate::model::config_dir().join("settings.txt"),"visible_pages=1000000\n").unwrap();
    assert_eq!(crate::model::Settings::load().visible_pages,[true,false,false,false,true,true,false,true,true],"旧版页面开关应保留，并开启新增的天气页");
    std::fs::write(crate::model::config_dir().join("settings.txt"),"visible_pages=invalid\n").unwrap();
    assert_eq!(crate::model::Settings::load().visible_pages,[true;crate::app::PAGE_COUNT]);
}

#[test]
fn stale_page_targets_preserve_the_current_page_and_visibility(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.show_page(crate::app::TODO_PAGE);
    for page in [crate::app::PAGE_COUNT,9,usize::MAX]{
        h.app.show_page(page);h.app.set_page_visible(page,false);
        assert_eq!(h.app.page(),crate::app::TODO_PAGE);
        assert_eq!(h.app.settings.visible_pages,[true;crate::app::PAGE_COUNT]);
        assert!(!h.app.settings_open);
    }
}

#[test]
fn page_visibility_skips_hidden_pages_in_every_navigation_control() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    for page in [crate::app::SHELF_PAGE,crate::app::CLIPBOARD_PAGE,crate::app::WEATHER_PAGE,crate::app::MAC_STATS_PAGE,crate::app::NOTIFICATIONS_PAGE] {h.app.set_page_visible(page,false);}
    h.app.show_page(crate::app::MUSIC_PAGE);h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);
    for expected in [crate::app::TODO_PAGE,crate::app::MAIL_PAGE,crate::app::AGENT_PAGE,crate::app::MUSIC_PAGE] {
        h.click_module(expected);
        assert_eq!(h.app.page,expected);
        h.hover(H::local(300.0,60.0),0.7);
    }
    let id=egui::Id::new(("card",true,(if cfg!(target_os="macos"){"page-tab"}else{"dot"},crate::app::TODO_PAGE)));
    let point=h.ctx.read_response(id).unwrap().rect.center();h.click(point);
    assert_eq!(h.app.page,2,"底部圆点应仅包含启用的页面");
    h.hover(H::local(300.0,60.0),0.7);
    h.frame(vec![egui::Event::Key{key:egui::Key::ArrowRight,physical_key:None,pressed:true,repeat:false,modifiers:Default::default()}]);
    assert_eq!(h.app.page,crate::app::MAIL_PAGE);
    h.frame(vec![egui::Event::Key{key:egui::Key::ArrowRight,physical_key:None,pressed:false,repeat:false,modifiers:Default::default()}]);
    h.hover(H::local(300.0,60.0),0.7);
    h.frame(vec![egui::Event::MouseWheel{unit:egui::MouseWheelUnit::Point,delta:Vec2::new(0.0,40.0),modifiers:Default::default()}]);
    assert_eq!(h.app.page,2,"滚轮应跳过隐藏页面");
    h.click_module(crate::app::MUSIC_PAGE);
    assert_eq!(h.app.page,crate::app::MUSIC_PAGE);
}

#[test]
fn page_visibility_moves_away_from_hidden_page_and_keeps_one_page(){
    use crate::app::{MUSIC_PAGE,SHELF_PAGE,TODO_PAGE,MAIL_PAGE};
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.show_page(TODO_PAGE);h.app.adding=true;h.app.set_page_visible(TODO_PAGE,false);
    assert_eq!(h.app.page,MUSIC_PAGE);assert!(!h.app.is_editing());
    for page in (0..crate::app::PAGE_COUNT).filter(|page|*page!=SHELF_PAGE){h.app.set_page_visible(page,false);}
    assert_eq!(h.app.page,SHELF_PAGE);h.app.set_page_visible(SHELF_PAGE,false);assert!(h.app.module_on(SHELF_PAGE));
    h.app.show_page(SHELF_PAGE);h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);h.page_key(true);h.page_key(false);assert_eq!(h.app.page,SHELF_PAGE);
    h.app.set_page_visible(MAIL_PAGE,true);h.hover(H::local(300.0,60.0),0.7);h.click_module(MAIL_PAGE);assert_eq!(h.app.page,MAIL_PAGE);
}

#[test]
fn page_visibility_does_not_reopen_hidden_pages_through_shortcuts_or_drops() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    h.app.set_page_visible(crate::app::MAIL_PAGE,false);
    h.app.show_page(crate::app::MAIL_PAGE);
    assert_eq!(h.app.page,crate::app::MUSIC_PAGE);
    assert!(!h.app.settings_open,"隐藏页面的入口不能隐式打开设置");
    assert_eq!(h.app.settings_tab,0);
    h.app.set_page_visible(crate::app::SHELF_PAGE,false);
    let file=crate::model::config_dir().join("shelf-hidden.txt");
    std::fs::create_dir_all(crate::model::config_dir()).unwrap();
    std::fs::write(&file,"test").unwrap();
    h.app.on_drop(vec![file]);
    assert_eq!(h.app.page,crate::app::MUSIC_PAGE);
    assert!(h.app.shelf.items.is_empty());
}

#[test]
fn alphabetic_mail_codes_reach_the_card_without_changing_case() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for (kind,value) in ["text/plain","text/html"].into_iter().flat_map(|kind|["AbCdEf","WWWWWWWW"].map(|value|(kind,value))) {
        let mut h=H::new();
        h.app.mail.accounts=vec![crate::mail::Account{custom:None,email:"test@qq.com".into(),provider:crate::mail::Provider::Qq,enabled:true}];
        let body=if kind=="text/html"{format!("<p>Your verification code:</p><p><b>{value}</b></p>")}else{format!("Your verification code:\n{value}")};
        let raw=format!("From: Example <security@example.invalid>\r\nSubject: Your verification code\r\nContent-Type: {kind}; charset=utf-8\r\n\r\n{body}");
        let parsed=crate::mail::parse_message(raw.as_bytes());
        let code=crate::mail::code_of(&parsed);
        assert_eq!(code.as_deref(),Some(value));
        h.app.mail_event(crate::mail::Event::Mail{new:true,item:crate::mail::Item{html:String::new().into(),received_unix:None,account:"test@qq.com".into(),from:"Example".into(),subject:parsed.subject.clone(),time:"09:41".into(),code,summary:crate::mail::body_of(&parsed).into(),links:vec![],uid:43}});
        h.app.show_page(crate::app::MAIL_PAGE);h.hover(H::local(300.0,70.0),1.0);
        let text=h.shapes.iter().find_map(|s|match &s.shape {egui::Shape::Text(t) if t.galley.text()==value=>Some(t),_=>None}).expect("字母验证码应出现在大卡上");
        assert!(text.galley.size().x<=120.5,"最长字母码不能挤入邮件列表：{}",text.galley.size().x);
        if value=="AbCdEf"{assert!(text.galley.job.sections.iter().any(|s|s.format.font_id.size>=23.0));}
        h.cmds.clear();h.click(H::local(60.0,70.0));
        assert!(h.cmds.iter().any(|c|matches!(c,crate::app::Cmd::Copy(copied) if copied==value)));
        h.t+=61.0;h.frame(vec![]);
        assert!(!h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()==value)));
    }
}

#[test]
fn regression_microsoft_traditional_mail_creates_code_card() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for kind in ["text/plain","text/html"] {
        let mut h=H::new();
        h.app.mail.accounts=vec![crate::mail::Account{custom:None,email:"test@qq.com".into(),provider:crate::mail::Provider::Qq,enabled:true}];
        let body="我們收到您要申請一次性驗證碼來與 Microsoft 帳戶搭配使用。\n您的一次性驗證碼為：431872\n僅在官方網站或應用程式上輸入此代碼。不要與任何人分享。";
        let body=if kind=="text/html" {format!("<p>{}</p>",body.replace('\n',"</p><p>"))}else{body.into()};
        let raw=format!("From: Microsoft <account-security@example.invalid>\r\nSubject: 您的一次性驗證碼\r\nMIME-Version: 1.0\r\nContent-Type: {kind}; charset=utf-8\r\nContent-Transfer-Encoding: 8bit\r\n\r\n{body}");
        let parsed=crate::mail::parse_message(raw.as_bytes());
        let code=crate::mail::code_of(&parsed);
        assert_eq!(code.as_deref(),Some("431872"),"{kind} 邮件应提取出验证码");
        h.app.mail_event(crate::mail::Event::Mail{new:true,item:crate::mail::Item{html:String::new().into(),received_unix:None,account:"test@qq.com".into(),from:parsed.from.clone(),subject:parsed.subject.clone(),time:"08:59".into(),code,summary:crate::mail::body_of(&parsed).into(),links:vec![],uid:42}});
        h.app.show_page(crate::app::MAIL_PAGE);h.hover(H::local(300.0,70.0),1.0);
        assert!(h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(t) if t.galley.text()=="431872"&&t.galley.job.sections.iter().any(|s|s.format.font_id.size>=23.0))),"新收到的繁体验证码应出现在大卡里");
        h.cmds.clear();h.click(H::local(60.0,70.0));
        assert!(h.cmds.iter().any(|cmd|matches!(cmd,crate::app::Cmd::Copy(value) if value=="431872")),"大卡应支持点击复制");
        h.t+=61.0;h.frame(vec![]);
        assert!(!h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(t) if t.galley.text()=="431872")),"繁体验证码同样应按隐私设置自动隐藏");
    }
}

#[test]
fn mail_page_shows_code_and_copies_on_click() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut h = H::new();
    h.app.mail.accounts = vec![crate::mail::Account {
        custom: None, email: "a@qq.com".into(),
        provider: crate::mail::Provider::Qq,
        enabled: true,
    }];
    h.app.mail_event(crate::mail::Event::Mail {
        item: crate::mail::Item {
            received_unix: None, account: "a@qq.com".into(),
            from: "GitHub".into(),
            subject: "verify".into(),
            time: "09:41".into(),
            code: Some("482913".into()),
            html: String::new().into(), summary: "Enter 482913 to finish signing in.".into(),
            links: vec![],

            uid: 42,
        },
        new: true,
    });
    assert!(h.app.notification_bell_until > 0.0, "新邮件应点亮铃铛");
    assert!(h.app.alerts.iter().any(|a|a.title=="新邮件"&&a.page==crate::app::MAIL_PAGE),"新邮件应进入通知队列，不能只有收起时才可见的铃铛");
    h.app.show_page(crate::app::MAIL_PAGE);
    h.hover(Pos2::new(WIN_W / 2.0, 12.0), 0.6);
    h.hover(H::local(300.0, 60.0), 0.4);
    h.click(H::local(60.0, 70.0));
    assert!(
        h.cmds.iter().any(|c| matches!(c, crate::app::Cmd::Copy(t) if t == "482913")),
        "点验证码应复制: {:?}",
        h.cmds
    );
}

#[test]
fn mail_row_opens_summary_and_back_returns() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut h = H::new();
    h.app.mail.accounts = vec![crate::mail::Account {
        custom: None, email: "a@qq.com".into(),
        provider: crate::mail::Provider::Qq,
        enabled: true,
    }];
    h.app.mail_event(crate::mail::Event::Mail {
        item: crate::mail::Item {
            received_unix: None, account: "a@qq.com".into(),
            from: "GitHub".into(),
            subject: "verify".into(),
            time: "09:41".into(),
            code: Some("482913".into()),
            html: String::new().into(), summary: "Enter 482913 to finish signing in.".into(),
            links: vec![],

            uid: 42,
        },
        new: true,
    });
    h.app.show_page(crate::app::MAIL_PAGE);
    h.hover(Pos2::new(WIN_W / 2.0, 12.0), 0.6);
    h.hover(H::local(300.0, 50.0), 0.4);
    h.click_control(("mail-row",42u32,"a@qq.com"));
    assert_eq!(h.app.mail.open_uid, Some(("a@qq.com".to_string(), 42)), "点邮件行应展开摘要");
    h.frame(vec![]);h.click_control(("mail-back",42u32));
    assert_eq!(h.app.mail.open_uid, None, "点返回应回到列表");
}

#[test]
fn empty_todo_draft_can_be_cancelled_by_clicking_elsewhere(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for text in ["","   "]{
        let mut h=H::new();let mut r=feature_raster();h.app.todos.items=vec![crate::model::TodoItem{text:"保留任务".into(),due:None,done_on:None,reminded:false,..Default::default()}];h.app.todos.save();
        h.app.show_page(crate::app::TODO_PAGE);h.hover(H::local(300.0,60.0),1.0);h.click(H::local(70.0,109.0));
        assert!(h.app.adding,"点击添加后应保留空白输入框");
        if !text.is_empty(){h.frame(vec![egui::Event::Text(text.into())]);}
        h.cmds.clear();h.click(H::local(65.0,60.0));
        assert!(!h.app.is_editing(),"空白草稿点击岛内其他位置应退出");
        assert!(h.app.buf.is_empty());assert_eq!(h.app.todos.items.len(),1);assert_eq!(h.app.todos.items[0].text,"保留任务");
        assert_eq!(crate::model::TodoList::load().items.len(),1,"不能保存空白待办");
        h.hover(H::local(65.0,60.0),0.3);
        assert!(!h.cmds.iter().any(|cmd|matches!(cmd,crate::app::Cmd::Focus)),"取消后不能反复激活窗口");
    }
}

#[test]
fn todo_ime_first_backspace_keeps_committed_text(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.show_page(crate::app::TODO_PAGE);h.hover(H::local(300.0,60.0),1.0);h.click(H::local(70.0,109.0));
    h.frame(vec![egui::Event::Text("保留文字".into())]);
    h.frame(vec![egui::Event::Key{key:egui::Key::Backspace,physical_key:None,pressed:true,repeat:false,modifiers:Default::default()},egui::Event::Ime(egui::ImeEvent::Enabled)]);
    assert_eq!(h.app.buf,"保留文字","开始组合输入的同一帧，退格只能交给输入法");
    h.frame(vec![egui::Event::Ime(egui::ImeEvent::Disabled)]);
    h.frame(vec![egui::Event::Key{key:egui::Key::Backspace,physical_key:None,pressed:true,repeat:false,modifiers:Default::default()}]);
    assert_eq!(h.app.buf,"保留文","结束组合输入后普通退格仍应正常删除");
}

#[test]
fn todo_ime_focus_change_preserves_empty_draft(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.show_page(crate::app::TODO_PAGE);h.hover(H::local(300.0,60.0),1.0);h.click(H::local(70.0,109.0));h.cmds.clear();
    h.frame(vec![egui::Event::Ime(egui::ImeEvent::Enabled),egui::Event::WindowFocused(false)]);
    assert!(h.app.adding,"输入法候选窗口造成的失焦不能取消空白待办");
    assert!(h.app.open);
    assert!(!h.cmds.iter().any(|cmd|matches!(cmd,crate::app::Cmd::Focus)),"不能和输入法窗口反复争抢焦点");
    h.app.foreground_changed();h.frame(vec![]);
    assert!(h.app.adding,"输入法组合输入期间的前台切换应保留草稿");
    h.frame(vec![egui::Event::WindowFocused(true),egui::Event::Ime(egui::ImeEvent::Preedit("zhong".into()))]);
    h.frame(vec![egui::Event::Ime(egui::ImeEvent::Commit("中文".into())),egui::Event::Ime(egui::ImeEvent::Disabled)]);
    assert_eq!(h.app.buf,"中文");assert!(h.app.todos.items.is_empty());
}

#[test]
fn todo_ime_confirmation_does_not_submit_or_cancel_the_draft(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for key in [egui::Key::Enter,egui::Key::Escape]{
        let mut h=H::new();h.app.show_page(crate::app::TODO_PAGE);h.hover(H::local(300.0,60.0),1.0);h.click(H::local(70.0,109.0));
        h.frame(vec![egui::Event::Ime(egui::ImeEvent::Enabled)]);
        h.frame(vec![egui::Event::Key{key,physical_key:None,pressed:true,repeat:false,modifiers:Default::default()},egui::Event::Ime(egui::ImeEvent::Commit("中文".into())),egui::Event::Ime(egui::ImeEvent::Disabled)]);
        assert!(h.app.adding,"输入法确认或取消候选不能同时结束待办编辑");
        assert_eq!(h.app.buf,"中文","输入法确认后应保留正文，按键 {key:?}");assert!(h.app.todos.items.is_empty());
        h.frame(vec![egui::Event::Key{key,physical_key:None,pressed:false,repeat:false,modifiers:Default::default()}]);
        h.frame(vec![egui::Event::Key{key:egui::Key::Enter,physical_key:None,pressed:true,repeat:false,modifiers:Default::default()}]);
        assert_eq!(h.app.todos.items.len(),1,"退出组合输入后回车应正常保存");assert_eq!(h.app.todos.items[0].text,"中文");
    }
}

#[test]
fn todo_ime_candidate_closing_does_not_discard_the_empty_draft(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.show_page(crate::app::TODO_PAGE);h.hover(H::local(300.0,60.0),1.0);h.click(H::local(70.0,109.0));
    h.frame(vec![egui::Event::Ime(egui::ImeEvent::Enabled)]);
    h.frame(vec![egui::Event::Ime(egui::ImeEvent::Disabled),egui::Event::WindowFocused(false)]);
    assert!(h.app.adding,"关闭输入法候选窗口的同一帧不能取消空白草稿");
    h.frame(vec![egui::Event::WindowFocused(true)]);h.app.foreground_changed();h.frame(vec![]);
    assert!(!h.app.adding,"之后真正切换应用时仍应取消空白草稿");
}

#[test]
fn todo_ime_event_variants_preserve_mixed_text_and_resume_plain_input(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let cases=vec![
        ("无显式开始事件",vec![vec![egui::ImeEvent::Preedit("ni".into())],vec![egui::ImeEvent::Commit("你".into()),egui::ImeEvent::Disabled]],"你"),
        ("一次提交多个字符",vec![vec![egui::ImeEvent::Enabled,egui::ImeEvent::Preedit("にほん".into())],vec![egui::ImeEvent::Commit("日本".into()),egui::ImeEvent::Disabled]],"日本"),
        ("同帧提交后继续组合",vec![vec![egui::ImeEvent::Enabled,egui::ImeEvent::Preedit("하".into())],vec![egui::ImeEvent::Commit("한".into()),egui::ImeEvent::Disabled,egui::ImeEvent::Enabled,egui::ImeEvent::Preedit("그".into())],vec![egui::ImeEvent::Commit("글".into()),egui::ImeEvent::Disabled]],"한글"),
        ("取消候选",vec![vec![egui::ImeEvent::Enabled,egui::ImeEvent::Preedit("pin".into())],vec![egui::ImeEvent::Preedit(String::new()),egui::ImeEvent::Disabled]],""),
    ];
    for (name,frames,expected) in cases {
        let mut h=H::new();h.app.show_page(crate::app::TODO_PAGE);h.hover(H::local(300.0,60.0),1.0);h.click(H::local(70.0,109.0));
        h.frame(vec![egui::Event::Text("中文 ABC ".into())]);
        for events in frames {h.frame(events.into_iter().map(egui::Event::Ime).collect());}
        assert_eq!(h.app.buf,format!("中文 ABC {expected}"),"事件模式：{name}");
        assert!(h.app.adding);assert!(h.app.todos.items.is_empty());
        h.frame(vec![egui::Event::Text("x".into())]);
        h.frame(vec![egui::Event::Key{key:egui::Key::Backspace,physical_key:None,pressed:true,repeat:false,modifiers:Default::default()}]);
        assert_eq!(h.app.buf,format!("中文 ABC {expected}"),"组合输入结束后应恢复普通退格：{name}");
        h.frame(vec![egui::Event::Key{key:egui::Key::Enter,physical_key:None,pressed:true,repeat:false,modifiers:Default::default()}]);
        assert_eq!(h.app.todos.items.len(),1,"组合输入结束后应能保存：{name}");
    }
}

#[test]
fn blank_todo_draft_closes_when_window_focus_is_lost(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.show_page(crate::app::TODO_PAGE);h.hover(H::local(300.0,60.0),1.0);h.click(H::local(70.0,109.0));
    h.cmds.clear();h.frame(vec![egui::Event::WindowFocused(false)]);
    assert!(!h.app.is_editing(),"点击其他软件后空白草稿应自动退出");
    assert!(h.app.todos.items.is_empty());
    assert!(!h.cmds.iter().any(|cmd|matches!(cmd,crate::app::Cmd::Focus)),"失去焦点时不能把窗口再次激活");
}

#[test]
fn todo_input_requests_window_focus_once_and_preserves_typed_drafts(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.show_page(crate::app::TODO_PAGE);h.hover(H::local(300.0,60.0),1.0);h.cmds.clear();h.click(H::local(70.0,109.0));
    assert_eq!(h.cmds.iter().filter(|cmd|matches!(cmd,crate::app::Cmd::Focus)).count(),1,"新建时只应请求一次窗口焦点");
    h.click(H::local(280.0,46.0));assert!(h.app.adding,"点击输入框不能取消草稿");
    h.frame(vec![egui::Event::Text("尚未保存的内容".into())]);h.cmds.clear();
    h.click(H::local(65.0,60.0));h.frame(vec![egui::Event::WindowFocused(false)]);
    for _ in 0..20{h.frame(vec![]);}
    assert!(h.app.adding);assert_eq!(h.app.buf,"尚未保存的内容");assert!(h.app.todos.items.is_empty());
    assert!(!h.cmds.iter().any(|cmd|matches!(cmd,crate::app::Cmd::Focus)),"已有内容失焦后应保留草稿，不能抢回焦点");
}

#[test]
fn switching_apps_cancels_only_empty_new_todos(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for text in ["","   ","保留未提交的内容"]{
        let mut h=H::new();h.app.show_page(crate::app::TODO_PAGE);h.hover(H::local(300.0,60.0),1.0);h.click(H::local(70.0,109.0));
        if !text.is_empty(){h.frame(vec![egui::Event::Text(text.into())]);}
        h.cmds.clear();h.app.foreground_changed();h.frame(vec![]);
        assert_eq!(h.app.adding,!text.trim().is_empty());
        assert_eq!(h.app.buf,if text.trim().is_empty(){""}else{text});
        assert!(h.app.todos.items.is_empty());
        assert!(!h.cmds.iter().any(|cmd|matches!(cmd,crate::app::Cmd::Focus)));
    }
}

#[test]
fn todo_can_be_added_from_empty_page() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut h = H::new();
    h.hover(Pos2::new(WIN_W / 2.0, 12.0), 1.0);
    h.app.go(crate::app::TODO_PAGE, 1.0);
    h.hover(H::local(300.0, 60.0), 0.8);
    h.click(H::local(70.0, 109.0));
    assert!(h.app.adding);
    h.hover(H::local(300.0, 60.0), 0.4);
    h.frame(vec![egui::Event::Text("写周报".into())]);
    h.frame(vec![egui::Event::Key { key: egui::Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Default::default() }]);
    h.hover(H::local(300.0, 60.0), 0.1);
    assert_eq!(h.app.todos.items.first().map(|t| t.text.as_str()), Some("写周报"));
    assert!(!h.app.adding);
}


#[test]
fn shelf_remove_keeps_original_file() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    let file=crate::model::config_dir().join("保留文件.txt");std::fs::create_dir_all(file.parent().unwrap()).unwrap();std::fs::write(&file,"内容").unwrap();
    h.app.shelf.add(&file).unwrap();h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);h.app.go(crate::app::SHELF_PAGE,1.0);h.hover(H::local(300.0,60.0),0.8);
    h.click_control(("shelf-remove",&file));assert!(h.app.shelf.items.is_empty());assert!(file.exists());assert!(crate::model::Shelf::load().items.is_empty());
}

#[test]
fn shelf_duplicate_drop_reports_existing_files_and_preserves_the_current_page() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    let dir=crate::model::config_dir();std::fs::create_dir_all(&dir).unwrap();
    let files:Vec<_>=(0..crate::app::PAGE_COUNT).map(|i|{let path=dir.join(format!("文件-{i}.txt"));std::fs::write(&path,"内容").unwrap();path}).collect();
    h.app.on_drop(files.clone());h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);h.app.shelf_first=0;
    h.app.on_drop(vec![files[0].clone()]);h.hover(H::local(300.0,60.0),0.3);
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="该文件已在置物架中")));
    h.app.on_drop(vec![files[0].clone(),files[1].clone()]);h.hover(H::local(300.0,60.0),0.3);
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="这些文件已在置物架中")));
    assert_eq!(h.app.shelf_first,0,"重复添加不应将列表跳到最后");
    assert_eq!(h.app.shelf.items,files);assert_eq!(crate::model::Shelf::load().items,files);
}

#[test]
fn shelf_drop_distinguishes_new_duplicate_and_missing_files() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    let dir=crate::model::config_dir();std::fs::create_dir_all(&dir).unwrap();
    let old=dir.join("已添加.txt");let new=dir.join("新文件.txt");let missing=dir.join("已移动.txt");
    for file in [&old,&new]{std::fs::write(file,"内容").unwrap();}
    h.app.on_drop(vec![old.clone()]);h.app.on_drop(vec![old.clone(),new.clone(),new.clone(),missing.clone()]);
    h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="已添加 1 个文件，2 个已存在，1 个无法添加")));
    assert_eq!(h.app.shelf.items,vec![old.clone(),new]);
    std::fs::remove_file(&old).unwrap();h.app.on_drop(vec![old]);h.hover(H::local(300.0,60.0),0.3);
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="文件不存在或无法访问")));
}
#[test]
fn reminders_are_visible_while_collapsed_and_can_be_disabled(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.frame(vec![]);
    h.app.alert("待办到时间了","完成评审",crate::app::TODO_PAGE,true);h.frame(vec![]);
    assert!(h.app.notice.is_some());assert!(h.cmds.iter().any(|c|matches!(c,crate::app::Cmd::Notify(t,_)if t=="待办到时间了")));
    h.app.settings.notifications=false;h.frame(vec![]);assert!(h.app.notice.is_none());
    h.cmds.clear();h.app.alert("待办到时间了","再检查一次",crate::app::TODO_PAGE,true);h.frame(vec![]);
    assert!(h.app.notice.is_none());assert!(h.cmds.is_empty());
}


/// 微信语音来电：响铃期间任务栏持续闪动，按秒渲染岛体，排查样式问题。

#[test]
fn todo_due_roundtrip_and_fires_only_once(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.todos.items.push(crate::model::TodoItem{text:"会议 | 准备".into(),done_on:None,due:Some("2026-10-28 09:40".into()),reminded:false,..Default::default()});h.app.todos.save();
    let loaded=crate::model::TodoList::load();assert_eq!(loaded.items[0].text,"会议 | 准备");assert_eq!(loaded.items[0].due.as_deref(),Some("2026-10-28 09:40"));
    h.frame(vec![]);assert!(h.app.todos.items[0].reminded);let n=h.cmds.len();h.frame(vec![]);assert_eq!(n,h.cmds.len());assert!(crate::model::TodoList::load().items[0].reminded);
}


#[test]
fn todo_completed_items_move_down_and_undo_restores_the_original_order(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.todos.items=["已完成甲","待做甲","周一重复","已完成乙","每日待办"].into_iter().map(crate::model::TodoItem::new).collect();
    h.app.todos.items[0].done_on=Some("2026-10-28".into());h.app.todos.items[3].done_on=Some("2026-10-28".into());h.app.todos.items[2].repeat=1;h.app.todos.items[4].repeat=crate::model::REPEAT_ALL;
    let original=h.app.todos.items.iter().map(|t|t.text.clone()).collect::<Vec<_>>();assert_eq!(h.app.todos.display_order("2026-10-28"),vec![1,4,2,0,3],"今天未完成优先，其次其他日期任务，已完成沉底");
    h.app.todos.items[0].toggle("2026-10-28");assert_eq!(h.app.todos.display_order("2026-10-28"),vec![0,1,4,2,3],"取消完成应恢复原有相对顺序");
    h.app.todos.items[4].toggle("2026-10-28");assert_eq!(h.app.todos.display_order("2026-10-28"),vec![0,1,2,3,4]);assert_eq!(h.app.todos.display_order("2026-10-29"),vec![0,1,4,2,3],"重复任务到下一天应回到未完成区");
    assert_eq!(h.app.todos.items.iter().map(|t|t.text.clone()).collect::<Vec<_>>(),original,"只改变显示顺序，不能移动原数据");
}

#[test]
fn todo_edit_choice_can_edit_other_rows_and_completed_items(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.todos.items=["第一项","第二项","已完成任务"].into_iter().map(crate::model::TodoItem::new).collect();h.app.todos.items[2].done_on=Some("2026-10-28".into());
    h.app.show_page(crate::app::TODO_PAGE);h.hover(H::local(300.0,60.0),1.0);h.click_text("改一下");
    assert!(h.app.editing.is_none(),"多条待办应先选择，不能默认修改第一项");h.frame(vec![]);h.click_text("第二项");
    assert_eq!(h.app.editing,Some(1));h.app.buf="第二项已修改".into();h.frame(vec![egui::Event::Key{key:egui::Key::Enter,physical_key:None,pressed:true,repeat:false,modifiers:Default::default()}]);
    assert_eq!(h.app.todos.items[0].text,"第一项");assert_eq!(h.app.todos.items[1].text,"第二项已修改");assert!(h.app.todos.items[2].done_on.is_some());
    h.frame(vec![]);h.click_text("改一下");h.frame(vec![]);h.click_text("已完成任务");assert_eq!(h.app.editing,Some(2));
    h.frame(vec![egui::Event::Key{key:egui::Key::Escape,physical_key:None,pressed:true,repeat:false,modifiers:Default::default()}]);assert!(!h.app.is_editing());assert!(h.app.todos.items[2].done_on.is_some());
}

#[test]
fn todo_edit_choice_keeps_a_single_completed_task_editable(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.app.todos.items=vec![crate::model::TodoItem::new("已完成任务")];h.app.todos.items[0].done_on=Some("2026-10-28".into());
    h.app.show_page(crate::app::TODO_PAGE);h.hover(H::local(300.0,60.0),1.0);h.click_text("改一下");assert_eq!(h.app.editing,Some(0),"只有一条时可直接编辑，完成状态不能禁用编辑");
}

#[test]
fn todo_edit_choice_reaches_scrolled_items_and_cancels_without_changes(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.app.todos.items=(0..8).map(|i|crate::model::TodoItem::new(format!("任务 {i}"))).collect();
    h.app.show_page(crate::app::TODO_PAGE);h.hover(H::local(300.0,60.0),1.0);h.click_text("改一下");assert!(h.app.editing.is_none());
    h.hover(H::local(300.0,60.0),0.2);for _ in 0..8{h.wheel(-24.0);}assert_eq!(h.app.page(),crate::app::TODO_PAGE,"选择事项时滚到末尾也不能翻页");h.frame(vec![]);h.click_text("任务 6");assert_eq!(h.app.editing,Some(6));
    h.frame(vec![egui::Event::Key{key:egui::Key::Escape,physical_key:None,pressed:true,repeat:false,modifiers:Default::default()}]);assert!(!h.app.is_editing());assert_eq!(h.app.todos.items[6].text,"任务 6");assert!(h.app.todos.items.iter().all(|t|t.done_on.is_none()));
}

#[test]
fn todo_completed_reordering_does_not_complete_the_next_item_on_a_double_click(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.app.todos.items=["任务甲","任务乙","任务丙"].into_iter().map(crate::model::TodoItem::new).collect();h.app.show_page(crate::app::TODO_PAGE);h.hover(H::local(300.0,60.0),1.0);
    let point=H::local(212.0,46.0);h.click(point);h.frame(vec![]);h.click(point);h.frame(vec![]);
    assert!(h.app.todos.items[0].done_on.is_some());assert!(h.app.todos.items[1].done_on.is_none(),"首项下移后，快速连点不能误完成下一项");
    h.click_text("任务甲");h.frame(vec![]);assert!(h.app.todos.items.iter().all(|t|t.done_on.is_none()));assert_eq!(h.app.todos.display_order("2026-10-28"),vec![0,1,2]);
}

#[test]
fn todo_edit_choice_can_cancel_without_toggling_or_deleting_items(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.app.todos.items=["任务甲","任务乙"].into_iter().map(crate::model::TodoItem::new).collect();h.app.show_page(crate::app::TODO_PAGE);h.hover(H::local(300.0,60.0),1.0);
    h.click_text("改一下");h.frame(vec![]);h.click_text("取消");assert!(!h.app.is_editing());assert_eq!(h.app.todos.items.len(),2);assert!(h.app.todos.items.iter().all(|t|t.done_on.is_none()));
    h.frame(vec![]);h.click_text("改一下");h.frame(vec![egui::Event::Key{key:egui::Key::Escape,physical_key:None,pressed:true,repeat:false,modifiers:Default::default()}]);assert!(!h.app.is_editing());assert_eq!(h.app.todos.items.len(),2);
}

#[test]
fn todo_edit_choice_keeps_the_selected_item_when_an_earlier_item_is_deleted(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.app.todos.items=["第一项","第二项","第三项"].into_iter().map(crate::model::TodoItem::new).collect();h.app.show_page(crate::app::TODO_PAGE);h.hover(H::local(300.0,60.0),1.0);
    h.click_text("改一下");h.frame(vec![]);h.click_text("第二项");h.frame(vec![]);h.click(h.ctx.read_response(egui::Id::new(("card",true,("todo-delete",0usize)))).unwrap().rect.center());
    assert_eq!(h.app.editing,Some(0),"删除前面的条目后，编辑目标仍须是第二项");h.app.buf="第二项已修改".into();h.frame(vec![egui::Event::Key{key:egui::Key::Enter,physical_key:None,pressed:true,repeat:false,modifiers:Default::default()}]);assert_eq!(h.app.todos.items.iter().map(|t|t.text.as_str()).collect::<Vec<_>>(),["第二项已修改","第三项"]);
}

#[test]
#[ignore="按需导出待办完成排序和点选编辑的实际画面"]
fn todo_edit_choice_gallery(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let dest=std::path::PathBuf::from(std::env::var_os("QINGYU_TODO_PREVIEW_DIR").expect("需指定预览目录"));std::fs::create_dir_all(&dest).unwrap();
    for skin in [crate::model::Skin::Classic,crate::model::Skin::Cream]{for choosing in [false,true]{
        let mut h=H::new();h.app.settings.skin=skin;h.app.todos.items=["整理会议资料","确认本周安排","已经完成的任务"].into_iter().map(crate::model::TodoItem::new).collect();h.app.todos.items[2].done_on=Some("2026-11-06".into());h.app.show_page(crate::app::TODO_PAGE);h.app.todo_choosing=choosing;
        let mut raster=feature_raster();shoot_feature(&mut h,&mut raster,&dest,&format!("todo-{}-{}",skin.key(),if choosing{"choose"}else{"list"}),90,Some(H::local(175.0,70.0)),"2026-11-06");
    }}
}

#[test]
fn task_editor_saves_and_cancels_inside_island(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);
    h.app.todos.items.push(crate::model::TodoItem{text:"准备会议".into(),done_on:None,due:None,reminded:false,..Default::default()});h.app.go(crate::app::TODO_PAGE,1.0);h.hover(H::local(300.0,60.0),0.8);h.app.open_task_dialog(0);
    h.hover(H::local(300.0,60.0),0.2);assert!(h.app.is_editing());h.click(H::local(326.0,110.0));assert!(!h.app.is_editing());assert_eq!(h.app.todos.items[0].text,"准备会议");
    h.app.open_task_dialog(0);h.frame(vec![egui::Event::Key{key:egui::Key::Escape,physical_key:None,pressed:true,repeat:false,modifiers:Default::default()}]);assert!(!h.app.is_editing());
}


#[test]
fn fluid_music_seek_draws_solid_played_segment(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for lyrics in [false,true] {
        let mut h=H::new();h.app.show_page(crate::app::MUSIC_PAGE);
        h.app.settings.skin=crate::model::Skin::Fluid;
        h.app.show_lyrics=lyrics;
        h.media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),position:30.0,duration:120.0,..Default::default()};
        h.hover(Pos2::new(WIN_W/2.0,12.0),1.2);
        let track=h.music_seek_track();let start=track.left_center();let w=track.width();
        assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Rect(r)
            if (r.rect.left()-start.x).abs()<1.0 && (r.rect.center().y-start.y).abs()<1.0
            && (r.rect.width()-w*0.25).abs()<1.0 && r.fill.a()==255 && r.fill.r()>160)));
    }
}

#[test]
fn music_seek_drag_previews_and_commits_on_release(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for (source,lyrics) in [("",false),("",true),("cloudmusic.exe",false),("cloudmusic.exe",true),("bodian_pc.exe",false),("bodian_pc.exe",true),("kwmusic.exe",false),("kwmusic.exe",true)] {
        let mut h=H::new();h.app.show_page(crate::app::MUSIC_PAGE);h.app.show_lyrics=lyrics;
        h.media=crate::smtc::NowPlaying{source:source.into(),title:"Track".into(),artist:"Artist".into(),position:12.0,duration:120.0,..Default::default()};
        h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);
        let track=h.music_seek_track();
        let press=Pos2::new(track.left()+track.width()*0.1,track.center().y);h.cursor=Some(press);
        h.frame(vec![egui::Event::PointerMoved(press)]);
        h.frame(vec![egui::Event::PointerButton{pos:press,button:egui::PointerButton::Primary,pressed:true,modifiers:Default::default()}]);
        let to=Pos2::new(track.left()+track.width()*0.75,track.center().y);h.cursor=Some(to);
        h.frame(vec![egui::Event::PointerMoved(to)]);
        assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="01:30")),"{source} 拖动中应显示目标时间");
        assert!(!h.cmds.iter().any(|c|matches!(c,crate::app::Cmd::Media(crate::smtc::Transport::Seek(_)))),"拖动中不应反复发送跳转");
        h.frame(vec![egui::Event::PointerButton{pos:to,button:egui::PointerButton::Primary,pressed:false,modifiers:Default::default()}]);
        assert!(h.cmds.iter().any(|c|matches!(c,crate::app::Cmd::Media(crate::smtc::Transport::Seek(t)) if (*t-90.0).abs()<0.1)),"释放应提交目标进度");
    }
}


#[test]
fn display_choice_saves_and_keeps_disconnected_selection(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.app.settings_open=true;h.ctx.set_embed_viewports(false);
    let bounds=crate::display_layout::Bounds{left:0,top:0,right:1920,bottom:1080};
    h.app.displays=vec![
        crate::display_layout::Display{id:"internal".into(),label:"显示器 1 · 主屏".into(),bounds,work_top:0,primary:true},
        crate::display_layout::Display{id:r"\\?\DISPLAY#external".into(),label:"显示器 2 · 外接屏".into(),bounds,work_top:0,primary:false},
    ];
    let mut raster=crate::preview_raster::Raster::new(690,560);
    fn frame(h:&mut H,raster:&mut crate::preview_raster::Raster,events:Vec<egui::Event>){
        h.t+=1.0/60.0;let time=h.t;
        let output=std::sync::Arc::new(Mutex::new(None));let capture=output.clone();
        egui::Context::set_immediate_viewport_renderer(move|ctx,mut viewport|{
            let id=viewport.ids.this;
            *capture.lock().unwrap()=Some(ctx.run(egui::RawInput{time:Some(time),viewport_id:id,screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(690.0,560.0))),events:events.clone(),viewports:std::iter::once((id,egui::ViewportInfo{native_pixels_per_point:Some(1.0),..Default::default()})).collect(),..Default::default()},|ctx|(viewport.viewport_ui_cb)(ctx)));
        });
        let root=h.ctx.run(egui::RawInput{time:Some(time),..Default::default()},|ctx|h.app.settings_ui(ctx,false));let child=output.lock().unwrap().take().unwrap();
        raster.update_textures(&root.textures_delta);raster.update_textures(&child.textures_delta);h.shapes=child.shapes;
    }
    // 设置页内容超过一屏时先向下滚动，直到目标完整出现在窗口内。
    fn reveal(h:&mut H,raster:&mut crate::preview_raster::Raster,label:&str)->Option<Rect>{
        let find=|h:&H|h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t)if t.galley.text()==label=>Some(t.visual_bounding_rect()),_=>None}).filter(|r|r.bottom()<550.0);
        for _ in 0..20 {
            if let Some(r)=find(h){return Some(r);}
            frame(h,raster,vec![egui::Event::PointerMoved(Pos2::new(450.0,300.0)),egui::Event::MouseWheel{unit:egui::MouseWheelUnit::Point,delta:Vec2::new(0.0,-60.0),modifiers:Default::default()}]);
            for _ in 0..6{frame(h,raster,vec![]);}
        }
        find(h)
    }
    fn click(h:&mut H,raster:&mut crate::preview_raster::Raster,label:&str){
        let pos=reveal(h,raster,label).map(|r|r.center()).unwrap_or_else(||panic!("未找到：{label}"));
        for pressed in [true,false]{frame(h,raster,vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()}]);}
        frame(h,raster,vec![]);
    }
    for _ in 0..3{frame(&mut h,&mut raster,vec![]);}
    assert!(h.app.settings.display_id.is_empty());click(&mut h,&mut raster,if cfg!(target_os="macos"){"主显示器"}else{"跟随主屏"});
    for _ in 0..20{frame(&mut h,&mut raster,vec![]);}
    if let Some(path)=std::env::var_os("QINGYU_DISPLAY_PREVIEW"){
        raster.buf.fill([0.0,0.0,0.0,1.0]);raster.draw(&h.ctx.tessellate(h.shapes.clone(),1.0),1.0);
        let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);image::save_buffer(path,&rgba,690,560,image::ColorType::Rgba8).unwrap();
    }
    click(&mut h,&mut raster,"显示器 2 · 外接屏");assert_eq!(h.app.settings.display_id,r"\\?\DISPLAY#external");
    assert_eq!(crate::model::Settings::load().display_id,h.app.settings.display_id);
    h.app.displays.pop();frame(&mut h,&mut raster,vec![]);
    let disconnected=if cfg!(target_os="macos"){"显示器已断开"}else{"所选屏幕未连接"};
    reveal(&mut h,&mut raster,disconnected);
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()==disconnected)));
    assert_eq!(crate::display_layout::selected(&h.app.displays,&h.app.settings.display_id).unwrap().id,"internal");
    click(&mut h,&mut raster,disconnected);click(&mut h,&mut raster,if cfg!(target_os="macos"){"主显示器"}else{"跟随主屏"});
    assert!(crate::model::Settings::load().display_id.is_empty());
}


#[test]
fn todo_notice_view_locates_the_reminded_task() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    let capacity=((h.app.content_rect().bottom()-51.0)/22.0).floor()as usize;let target=capacity+6;
    h.app.todos.items=(0..capacity+8).map(|i|crate::model::TodoItem{text:format!("任务 {i}"),done_on:None,due:if i==target{Some("2026-10-28 09:40".into())}else{None},reminded:false,..Default::default()}).collect();
    for _ in 0..60 {h.frame(vec![]);}
    h.click_control("notice-summary");h.hover(H::local(160.0,180.0),1.0);
    assert_eq!(h.app.page,crate::app::TODO_PAGE);
    let text=format!("任务 {target}");let row=h.response(("todo",target,text.as_str())).rect;
    assert!(row.is_positive()&&h.hit.contains_rect(row),"打开待办页时应能看到收到提醒的任务");
}

#[test]
fn mail_hyperlink_click_requests_the_default_browser() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    h.app.mail.accounts=vec![crate::mail::Account {custom: None, email:"a@qq.com".into(),provider:crate::mail::Provider::Qq,enabled:true}];
    h.app.mail.recent=vec![crate::mail::Item {received_unix: None, account:"a@qq.com".into(),from:"产品团队".into(),subject:"评审文档".into(),time:"09:41".into(),code:None,html: String::new().into(), summary:"请查看以下文档。".into(),links:vec![crate::mail::Link{label:"查看文档".into(),url:"https://example.com/review?a=1&b=2".into()}],uid:10}];
    h.app.mail.open_uid=Some(("a@qq.com".into(),10));h.app.show_page(crate::app::MAIL_PAGE);
    h.hover(H::local(300.0,60.0),1.0);
    let link=h.shapes.iter().find_map(|shape|match &shape.shape{egui::Shape::Text(text)if text.galley.text().contains("查看文档")=>Some(text.visual_bounding_rect()),_=>None}).expect("应展示邮件的超链接");
    h.click(link.center());
    assert!(h.cmds.iter().any(|cmd|matches!(cmd,crate::app::Cmd::OpenUrl(url)if url=="https://example.com/review?a=1&b=2")),"点击后应通过默认浏览器打开完整地址");
}


#[test]
fn code_copy_confirmation_stays_below_the_island_and_does_not_capture_hover() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for detail in [false,true] {
        let mut h=H::new();
        h.app.mail.accounts=vec![crate::mail::Account {custom: None, email:"a@qq.com".into(),provider:crate::mail::Provider::Qq,enabled:true}];
        h.app.mail.recent=vec![crate::mail::Item {received_unix: None, account:"a@qq.com".into(),from:"GitHub".into(),subject:"登录验证码".into(),time:"09:41".into(),code:Some("482913".into()),html: String::new().into(), summary:"请勿向他人透露验证码。".into(),links:vec![],uid:42}];
        for item in h.app.mail.recent.clone(){h.app.mail.privacy.observe(&item,true,h.app.t,chrono::Utc::now().timestamp());}
        if detail {h.app.mail.open_uid=Some(("a@qq.com".into(),42));}
        h.app.show_page(crate::app::MAIL_PAGE);h.hover(H::local(300.0,60.0),4.2);
        let copy=H::local(80.0,if detail{212.0}else{70.0});
        h.click(copy);h.hover(copy,0.35);
        let confirmation=|h:&H|h.shapes.iter().find_map(|shape|match &shape.shape {
            egui::Shape::Text(text) if text.galley.text()=="验证码已复制"=>Some(text.visual_bounding_rect()),_=>None,
        });
        let bounds=confirmation(&h).expect("复制后应显示确认提示");
        assert!(bounds.top()>PH+4.0,"确认提示应在岛下方：{bounds:?}");
        assert!(h.region.contains_rect(bounds),"原生窗口区域应包含提示，避免被裁掉");
        assert!(!h.hit.intersects(bounds),"确认提示不应拦截鼠标或维持岛的展开");
        h.hover(Pos2::new(600.0,180.0),0.8);
        assert!(h.app.progress()<0.02);
        assert!(confirmation(&h).is_some(),"岛收起后确认提示仍应显示到正常结束");
        h.hover(Pos2::new(600.0,180.0),2.0);
        assert!(confirmation(&h).is_none());
        assert!(h.region.bottom()<100.0,"提示结束后应释放额外窗口区域");
    }
}

#[test]
fn adding_a_todo_saves_content_then_offers_an_optional_reminder() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for remind in [false,true] {
        let mut h=H::new();h.app.show_page(crate::app::TODO_PAGE);h.hover(H::local(300.0,60.0),1.0);
        h.click(H::local(70.0,109.0));h.frame(vec![egui::Event::Text("提交产品方案".into())]);
        h.frame(vec![egui::Event::Key{key:egui::Key::Enter,physical_key:None,pressed:true,repeat:false,modifiers:Default::default()}]);
        assert_eq!(h.app.todos.items[0].text,"提交产品方案");
        assert!(h.app.todos.items[0].due.is_none());
        assert!(h.app.task_dialog.is_some(),"保存内容后应进入可选提醒步骤");
        assert_eq!(crate::model::TodoList::load().items[0].text,"提交产品方案");
        h.frame(vec![egui::Event::Key{key:egui::Key::Enter,physical_key:None,pressed:false,repeat:false,modifiers:Default::default()}]);
        if remind {
            let initial=chrono::NaiveDateTime::parse_from_str(&h.app.task_dialog.as_ref().unwrap().picker.due(),"%Y-%m-%d %H:%M").unwrap();
            h.hover(H::local(235.0,77.0),0.4);h.wheel(-40.0);h.picker_click("完成");
            let expected=(initial+chrono::Duration::days(1)).format("%Y-%m-%d %H:%M").to_string();
            assert_eq!(h.app.todos.items[0].due.as_deref(),Some(expected.as_str()));
        }else{h.picker_click("暂不提醒");assert!(h.app.todos.items[0].due.is_none());}
        assert!(h.app.task_dialog.is_none());
        assert!(h.app.todos.items[0].done_on.is_none());
        assert_eq!(crate::model::TodoList::load().items[0].due,h.app.todos.items[0].due);
    }
}

#[test]
fn todo_clock_button_sets_a_reminder_without_completing_the_task() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.todos.items=vec![crate::model::TodoItem{text:"已有任务".into(),done_on:None,due:None,reminded:false,..Default::default()}];
    h.app.show_page(crate::app::TODO_PAGE);h.hover(H::local(300.0,60.0),1.0);h.click_control(("todo-reminder",0usize));
    assert!(h.app.task_dialog.is_some(),"每项待办应有可见的提醒入口");
    assert!(h.app.todos.items[0].done_on.is_none(),"时钟入口不能同时勾选任务");
    h.frame(vec![egui::Event::Key{key:egui::Key::Escape,physical_key:None,pressed:true,repeat:false,modifiers:Default::default()}]);
    assert!(h.app.task_dialog.is_none());assert_eq!(h.app.todos.items[0].text,"已有任务");
}

#[test]
fn mail_details_show_the_receiving_account_when_two_accounts_share_a_uid() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.mail.accounts=vec![crate::mail::Account {custom: None, email:"first@qq.com".into(),provider:crate::mail::Provider::Qq,enabled:true},crate::mail::Account {custom: None, email:"work@icloud.com".into(),provider:crate::mail::Provider::ICloud,enabled:true}];
    h.app.mail.recent=["first@qq.com","work@icloud.com"].into_iter().map(|account|crate::mail::Item {received_unix: None, account:account.into(),from:"产品团队".into(),subject:"评审安排".into(),time:"09:41".into(),code:None,html: String::new().into(), summary:"请查看会议安排。".into(),links:vec![],uid:42}).collect();
    h.app.show_page(crate::app::MAIL_PAGE);h.hover(H::local(300.0,60.0),1.0);
    for (row,account) in ["first@qq.com","work@icloud.com"].into_iter().enumerate(){
        h.hover(H::local(280.0,64.0+row as f32*21.5),0.6);h.click(H::local(280.0,64.0+row as f32*21.5));h.hover(H::local(300.0,90.0),0.1);
        assert_eq!(h.app.mail.open_uid,Some((account.into(),42)));
        assert!(h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(text)if text.galley.text()==format!("收件邮箱：{account}"))),"详情必须明确标出这封邮件所属的收件邮箱");
        h.click(H::local(350.0,48.0));
    }
}

#[test]
fn scrolling_mail_details_keeps_the_current_message_open() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.mail.accounts=vec![crate::mail::Account {custom: None, email:"work@qq.com".into(),provider:crate::mail::Provider::Qq,enabled:true}];
    h.app.mail.recent=vec![crate::mail::Item {received_unix: None, account:"work@qq.com".into(),from:"产品团队".into(),subject:"项目方案".into(),time:"09:41".into(),code:None,html: String::new().into(), summary:"需要滚动阅读的邮件正文。".repeat(20).into(),links:vec![],uid:42}];
    h.app.mail.open_uid=Some(("work@qq.com".into(),42));h.app.show_page(crate::app::MAIL_PAGE);h.hover(H::local(270.0,150.0),1.0);
    let body_clip=h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t) if t.galley.text().starts_with("需要滚动阅读")=>Some(s.clip_rect),_=>None}).expect("正文应显示");
    assert!(body_clip.height()>=80.0,"详情正文应有足够阅读空间，而不是仅能容纳一行");
    h.frame(vec![egui::Event::MouseWheel{unit:egui::MouseWheelUnit::Point,delta:egui::vec2(0.0,-120.0),modifiers:Default::default()}]);
    assert_eq!(h.app.page(),crate::app::MAIL_PAGE,"邮件正文滚动不能切换功能页");
    assert_eq!(h.app.mail.open_uid,Some(("work@qq.com".into(),42)));
}

#[test]
fn todo_reminder_picker_is_embedded_in_the_island(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.todos.items=vec![crate::model::TodoItem{text:"准备会议".into(),done_on:None,due:None,reminded:false,..Default::default()}];
    h.app.show_page(crate::app::TODO_PAGE);h.hover(H::local(300.0,60.0),1.0);
    let expanded_hit=h.hit;
    h.ctx.set_embed_viewports(false);
    egui::Context::set_immediate_viewport_renderer(|_,_|panic!("待办提醒必须在岛内选择，不能创建独立窗口"));
    h.click_control(("todo-reminder",0usize));
    for _ in 0..90 {h.frame(vec![]);}
    assert_eq!(h.app.expanded_height(),crate::control_center::TOP_H,"提醒滚轮沿用统一展开高度");
    assert!((h.hit.height()-expanded_hit.height()).abs()<2.0,"选择时间不能撑大岛体");
    for label in ["准备会议","已完成","取消","完成","暂不提醒"] {
        let (rect,clip)=h.shapes.iter().find_map(|s|match &s.shape {
            egui::Shape::Text(t) if t.galley.text()==label=>Some((t.visual_bounding_rect(),s.clip_rect)),_=>None,
        }).unwrap_or_else(||panic!("岛内缺少选项：{label}"));
        assert!(h.hit.contains_rect(rect),"选项 {label} 应在岛体命中区域内");
        assert!(h.region.contains_rect(rect) && clip.contains_rect(rect),"选项 {label} 不能被窗口或岛体裁剪");
    }
    h.frame(vec![egui::Event::Key{key:egui::Key::Escape,physical_key:None,pressed:true,repeat:false,modifiers:Default::default()}]);
    assert!(h.app.task_dialog.is_none());assert!(h.app.todos.items[0].due.is_none());
}

#[test]
fn reminder_wheels_select_date_hour_and_minute_without_typing(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    let day=chrono::Local::now().date_naive()+chrono::Duration::days(2);let original=format!("{day} 09:00");
    h.app.todos.items=vec![crate::model::TodoItem{text:"准备方案".into(),due:Some(original.clone()),done_on:None,reminded:false,..Default::default()}];
    h.app.open_task_reminder(0);
    for x in [235.0,290.0,346.0] {h.hover(H::local(x,77.0),0.4);h.wheel(-40.0);}
    assert_eq!(h.app.todos.items[0].due,Some(original),"调整时不能提前写入提醒");
    h.picker_click("完成");
    let next=day+chrono::Duration::days(1);
    assert_eq!(h.app.todos.items[0].due,Some(format!("{next} 10:01")));
    assert!(h.app.task_dialog.is_none());
}

#[test]
fn reminder_wheels_support_clicks_and_dragging(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    let day=chrono::Local::now().date_naive()+chrono::Duration::days(2);
    h.app.todos.items=vec![crate::model::TodoItem{text:"确认方案".into(),due:Some(format!("{day} 10:30")),done_on:None,reminded:false,..Default::default()}];
    h.app.open_task_reminder(0);h.hover(H::local(300.0,60.0),1.0);
    h.click(H::local(346.0,96.0));h.click(H::local(290.0,58.0));
    let start=H::local(235.0,77.0);h.cursor=Some(start);
    h.frame(vec![egui::Event::PointerMoved(start),egui::Event::PointerButton{pos:start,button:egui::PointerButton::Primary,pressed:true,modifiers:Default::default()}]);
    for y in [59.0,41.0]{let point=H::local(235.0,y);h.cursor=Some(point);h.frame(vec![egui::Event::PointerMoved(point)]);}
    let end=H::local(235.0,41.0);
    h.frame(vec![egui::Event::PointerButton{pos:end,button:egui::PointerButton::Primary,pressed:false,modifiers:Default::default()}]);
    h.picker_click("完成");
    assert_eq!(h.app.todos.items[0].due,Some(format!("{} 09:31",day+chrono::Duration::days(2))));
    assert!(h.app.todos.items[0].done_on.is_none());
}

#[test]
fn reminder_wheels_accumulate_small_trackpad_movements(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    let day=chrono::Local::now().date_naive()+chrono::Duration::days(2);
    h.app.todos.items=vec![crate::model::TodoItem{text:"检查进度".into(),due:Some(format!("{day} 10:30")),done_on:None,reminded:false,..Default::default()}];
    h.app.open_task_reminder(0);h.hover(H::local(346.0,77.0),1.0);
    for _ in 0..80{h.wheel(-0.5);}
    h.picker_click("完成");
    assert_eq!(h.app.todos.items[0].due,Some(format!("{day} 10:31")));
    assert_eq!(h.app.page(),crate::app::TODO_PAGE,"操作时间滚轮不能触发翻页");
}

#[test]
fn reminder_picker_rejects_past_time_and_cancel_preserves_the_task(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    let old=format!("{} 09:00",chrono::Local::now().date_naive()-chrono::Duration::days(1));
    h.app.todos.items=vec![crate::model::TodoItem{text:"保留任务".into(),due:Some(old.clone()),done_on:None,reminded:true,..Default::default()}];
    h.app.open_task_reminder(0);h.picker_click("完成");h.picker_frame(vec![]);
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()=="请选择晚于当前的时间")));
    assert_eq!(h.app.todos.items[0].due,Some(old.clone()));
    h.picker_click("取消");assert!(h.app.task_dialog.is_none());assert_eq!(h.app.todos.items[0].due,Some(old));
}

#[test]
fn full_mail_reader_keeps_the_entire_body_and_scrolls_independently(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    let body=format!("开头：这是第一段。\n\n{}\n最后一段：全文结束", "这是一段需要阅读的完整邮件正文。\n\n".repeat(200));
    let html=body.split('\n').map(|line|format!("<p>{line}</p>")).collect::<String>();
    let item=crate::mail::Item {received_unix: None, account:"work@qq.com".into(),from:"产品团队".into(),subject:"完整方案".into(),time:"09:41".into(),code:None,html:html.into(), summary:body.clone().into(),links:vec![],uid:42};
    h.app.mail.accounts=vec![crate::mail::Account {custom: None, email:item.account.clone(),provider:crate::mail::Provider::Qq,enabled:true}];
    h.app.mail.recent=vec![item];h.app.mail.open_uid=Some(("work@qq.com".into(),42));h.app.show_page(crate::app::MAIL_PAGE);h.hover(H::local(300.0,90.0),1.0);
    h.click(H::local(293.0,48.0));assert_eq!(h.app.mail.reading.as_ref().unwrap().summary.as_ref(),body);
    h.app.mail.recent.clear();
    let render=|h:&mut H,events|{
        h.ctx.set_embed_viewports(true);h.t+=1.0/60.0;
        let out=h.ctx.run(egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(700.0,620.0))),time:Some(h.t),events,focused:true,..Default::default()},|ctx|h.app.mail_reader_ui(ctx));h.shapes=out.shapes;
    };
    // 正文按块渲染：视口外的块不生成文字形状，所以「末尾可见」只能滚动到底再断言。
    let text_top=|h:&H,needle:&str|h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t)if t.galley.text()==needle=>Some(t.visual_bounding_rect().top()),_=>None});
    let wheel=|dy:f32|vec![egui::Event::PointerMoved(egui::pos2(350.0,300.0)),egui::Event::MouseWheel{unit:egui::MouseWheelUnit::Point,delta:egui::vec2(0.0,dy),modifiers:Default::default()}];
    render(&mut h,vec![]);render(&mut h,vec![]);
    let first=text_top(&h,"开头：这是第一段。").expect("首屏应显示正文开头");
    assert!(text_top(&h,"最后一段：全文结束").is_none(),"长邮件末尾应在首屏之外");
    for _ in 0..5{render(&mut h,wheel(-400.0));}
    let moved=text_top(&h,"开头：这是第一段。");
    assert!(moved.is_none_or(|top|top<first-100.0),"阅读窗口的滚轮应向下滚动正文");
    for _ in 0..60{render(&mut h,wheel(-400.0));}
    assert!(text_top(&h,"最后一段：全文结束").is_some(),"滚动到底应能看到邮件末尾");
    assert!(h.app.mail.reading.is_some(),"新邮件列表变化不能打断已经打开的正文");
}

#[test]
#[ignore="按需导出多格式邮件正文的实际画面"]
fn mail_reader_rich_html_gallery(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let dest=std::path::PathBuf::from(std::env::var_os("QINGYU_MAIL_GALLERY_DIR").expect("需指定预览目录"));std::fs::create_dir_all(&dest).unwrap();
    let mut h=H::new();h.ctx.set_embed_viewports(true);
    let html=r#"<html><head><style>body{font-family:Arial}</style></head><body>
        <div style="display:none;max-height:0;overflow:hidden">&#8203;&#8203; 这是一段只为收件箱预览存在的填充文字 &#8203;</div>
        <table role="presentation" cellpadding="0" cellspacing="0" width="100%"><tr><td><img src="https://example.com/logo.png" alt="logo"> 通知中心</td><td>在浏览器中查看</td></tr></table>
        <h1>订单已确认</h1>
        <p>您好，<strong>订单 A20261107</strong> 已支付成功，我们会在 24 小时内发货。</p>
        <p><img src="https://example.com/order.png" alt="订单物流状态截图" width="600"></p>
        <h3>订单明细</h3>
        <table><tr><th>商品</th><th>数量</th><th>金额</th></tr><tr><td>机械键盘</td><td>1</td><td>¥499</td></tr><tr><td>腕托</td><td>2</td><td>¥78</td></tr></table>
        <h3>接下来会发生什么</h3>
        <ol><li>仓库拣货并出库</li><li>快递揽收后短信通知</li><li>签收后 7 天内可申请退换</li></ol>
        <ul><li>发票为电子发票，发货后发到本邮箱</li><li>如需修改地址，请在发货前回复本邮件</li></ul>
        <blockquote>注意：本邮件由系统自动发送，请勿直接回复。</blockquote>
        <hr>
        <p>感谢支持 · <a href="https://example.com/orders">查看订单详情</a></p>
    </body></html>"#;
    h.app.mail.reading=Some(crate::mail::Item{received_unix:None,account:"reader@example.com".into(),from:"通知中心".into(),
        subject:"订单 A20261107 已确认".into(),time:"11-07 09:41".into(),code:None,
        html:html.into(),
        summary:"订单已确认 您好，订单 A20261107 已支付成功，我们会在 24 小时内发货。订单明细 商品 数量 金额 机械键盘 1 ¥499 腕托 2 ¥78".into(),
        links:vec![crate::mail::Link{label:"查看订单详情".into(),url:"https://example.com/orders".into()}],uid:7});
    let (w,height)=(700,620);let ppp=1.5;
    let mut raster=crate::preview_raster::Raster::new((w as f32*ppp)as usize,(height as f32*ppp)as usize);
    for f in 0..=8{
        let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(w as f32,height as f32))),time:Some(f as f64/60.0),viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(ppp),..Default::default()})).collect(),..Default::default()};
        let out=h.ctx.run(raw,|ctx|h.app.mail_reader_ui(ctx));raster.update_textures(&out.textures_delta);
        if f==8{raster.buf.fill([0.0,0.0,0.0,1.0]);raster.draw(&h.ctx.tessellate(out.shapes,out.pixels_per_point),out.pixels_per_point);let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);image::save_buffer(dest.join("mail-reader-rich.png"),&rgba,(w as f32*ppp)as u32,(height as f32*ppp)as u32,image::ColorType::Rgba8).unwrap();}
        raster.free_textures(&out.textures_delta);
    }
}

#[test]
fn mail_plain_text_keeps_literal_stars_in_the_reader(){
    let h=H::new();h.ctx.set_embed_viewports(true);
    let lines=crate::mail_format::lines("","保留 ** 原始字符");
    let out=h.ctx.run(egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(700.0,620.0))),..Default::default()},|ctx|{
        egui::CentralPanel::default().show(ctx,|ui|crate::mail_reader::body_ui(ui,&lines,14.0));
    });
    assert!(out.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()=="保留 ** 原始字符")));
}

#[test]
fn mail_detail_and_reader_keep_their_own_cached_body(){
    let mut h=H::new();
    let item=|uid,body:&str|crate::mail::Item{html:format!("<p>{body}</p>").into(),summary:body.to_string().into(),uid,
        account:"test@example.com".into(),from:String::new(),subject:String::new(),time:String::new(),code:None,received_unix:None,links:vec![]};
    let first=item(1,"第一封邮件");let second=item(2,"第二封邮件");
    let before=h.app.mail_body_lines(&first);h.app.mail_body_lines(&second);
    let after=h.app.mail_body_lines(&first);
    assert!(std::sync::Arc::ptr_eq(&before,&after),"详情与独立阅读窗口不能让另一封邮件的排版缓存每帧失效");
}

#[test]
fn mail_html_headings_scale_with_the_body_font(){
    let h=H::new();let lines=crate::mail_format::lines("<h1>缩放标题</h1>","");
    for (base,expected) in [(14.0,18.0),(28.0,36.0)]{
        let out=h.ctx.run(egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(700.0,620.0))),..Default::default()},|ctx|{
            egui::CentralPanel::default().show(ctx,|ui|crate::mail_reader::body_ui(ui,&lines,base));
        });
        let size=out.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t)if t.galley.text()=="缩放标题"=>Some(t.galley.job.sections[0].format.font_id.size),_=>None}).unwrap();
        assert_eq!(size,expected,"正文缩放时标题应保持相同的字号比例");
    }
}

#[test]
fn mail_microsoft_settings_only_explains_unsupported_access(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.open_mail_settings();h.ctx.set_embed_viewports(false);
    h.app.mail.accounts.clear();h.app.mail.form.provider=crate::mail::Provider::Microsoft;
    let output=std::sync::Arc::new(Mutex::new(None));let capture=output.clone();
    egui::Context::set_immediate_viewport_renderer(move|ctx,mut viewport|{
        let id=viewport.ids.this;
        let raw=egui::RawInput{viewport_id:id,screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(690.0,1300.0))),viewports:std::iter::once((id,egui::ViewportInfo::default())).collect(),..Default::default()};
        *capture.lock().unwrap()=Some(ctx.run(raw,|ctx|(viewport.viewport_ui_cb)(ctx)));
    });
    for _ in 0..3{let _=h.ctx.run(Default::default(),|ctx|h.app.settings_ui(ctx,false));}
    let child=output.lock().unwrap().take().unwrap();
    let texts=child.shapes.iter().filter_map(|s|match &s.shape{egui::Shape::Text(t)=>Some(t.galley.text()),_=>None}).collect::<Vec<_>>();
    assert!(texts.iter().any(|t|t.contains("OAuth")&&t.contains("暂不支持")),"应明确说明微软邮箱的支持状态");
    for absent in ["使用 Microsoft 登录","可留空，在浏览器选择账号","输入邮箱授权码","测试连接","保存并开始监控","应用 ID"]{
        assert!(!texts.iter().any(|t|t.contains(absent)),"暂不支持时不应出现误导入口：{absent}");
    }
    assert!(h.app.mail.accounts.is_empty());
    assert!(!h.app.pending.iter().any(|cmd|matches!(cmd,crate::app::Cmd::MailTest(..)|crate::app::Cmd::OpenUrl(_))));
}

#[test]
fn mail_microsoft_custom_host_and_address_disable_password_actions(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.open_mail_settings();h.ctx.set_embed_viewports(false);h.app.mail.accounts.clear();
    let output=std::sync::Arc::new(Mutex::new(None));let capture=output.clone();
    let events=std::sync::Arc::new(Mutex::new(Vec::new()));let input=events.clone();
    egui::Context::set_immediate_viewport_renderer(move|ctx,mut viewport|{
        let id=viewport.ids.this;
        let raw=egui::RawInput{viewport_id:id,focused:true,events:std::mem::take(&mut *input.lock().unwrap()),
            screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(690.0,1300.0))),
            viewports:std::iter::once((id,egui::ViewportInfo::default())).collect(),..Default::default()};
        *capture.lock().unwrap()=Some(ctx.run(raw,|ctx|(viewport.viewport_ui_cb)(ctx)));
    });
    for (provider,email,host) in [(crate::mail::Provider::Custom,"student@example.edu","outlook.office365.com"),(crate::mail::Provider::Qq,"user@hotmail.com","")]{
        h.app.mail.form.provider=provider;h.app.mail.form.email=email.into();h.app.mail.form.custom.host=host.into();
        h.app.mail.form.password.clear();h.app.mail.note.clear();
        for _ in 0..3{let _=h.ctx.run(Default::default(),|ctx|h.app.settings_ui(ctx,false));}
        let child=output.lock().unwrap().take().unwrap();
        assert!(child.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()==crate::mail::MICROSOFT_UNSUPPORTED)));
        for label in ["测试连接","保存并开始监控"]{
            let point=child.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t)if t.galley.text()==label=>Some(t.visual_bounding_rect().center()),_=>None}).unwrap();
            for pressed in [true,false]{
                *events.lock().unwrap()=vec![egui::Event::PointerMoved(point),egui::Event::PointerButton{pos:point,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()}];
                let _=h.ctx.run(Default::default(),|ctx|h.app.settings_ui(ctx,false));
            }
            assert!(h.app.mail.note.is_empty(),"微软云连接的按钮应停用，点击不应再提示填写授权码");
        }
        assert!(h.app.mail.accounts.is_empty());
        assert!(!h.app.pending.iter().any(|cmd|matches!(cmd,crate::app::Cmd::MailTest(..)|crate::app::Cmd::MailSync)));
    }
}

#[test]
#[ignore="按需导出微软邮箱暂不支持提示的实际画面"]
fn mail_microsoft_settings_gallery(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let dest=std::path::PathBuf::from(std::env::var_os("QINGYU_MAIL_GALLERY_DIR").expect("需指定预览目录"));std::fs::create_dir_all(&dest).unwrap();
    for custom in [false,true]{
        let mut h=H::new();h.app.open_mail_settings();h.ctx.set_embed_viewports(false);
        h.app.mail.accounts.clear();
        h.app.mail.form.provider=if custom{crate::mail::Provider::Custom}else{crate::mail::Provider::Microsoft};
        h.app.mail.form.email="student@example.edu".into();h.app.mail.form.custom.host="outlook.office365.com".into();
        let output=std::sync::Arc::new(Mutex::new(None));let capture=output.clone();
        egui::Context::set_immediate_viewport_renderer(move|ctx,mut viewport|{
            let id=viewport.ids.this;let raw=egui::RawInput{viewport_id:id,screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(690.0,1300.0))),
                viewports:std::iter::once((id,egui::ViewportInfo{native_pixels_per_point:Some(1.5),..Default::default()})).collect(),..Default::default()};
            *capture.lock().unwrap()=Some(ctx.run(raw,|ctx|(viewport.viewport_ui_cb)(ctx)));
        });
        let mut raster=crate::preview_raster::Raster::new(1035,1950);
        for frame in 0..4{
            let root=h.ctx.run(Default::default(),|ctx|h.app.settings_ui(ctx,false));raster.update_textures(&root.textures_delta);
            let child=output.lock().unwrap().take().unwrap();raster.update_textures(&child.textures_delta);
            if frame==3{
                raster.buf.fill([0.08,0.08,0.09,1.0]);raster.draw(&h.ctx.tessellate(child.shapes,child.pixels_per_point),child.pixels_per_point);
                let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);image::save_buffer(dest.join(if custom{"microsoft-custom-unsupported.png"}else{"microsoft-unsupported.png"}),&rgba,1035,1950,image::ColorType::Rgba8).unwrap();
            }raster.free_textures(&child.textures_delta);
        }
    }
}

#[test]
#[ignore="按需导出提醒选择器与邮件全文的实际画面"]
fn reminder_and_reader_layout_gallery(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let dest=std::path::PathBuf::from(std::env::var_os("QINGYU_READER_PREVIEW_DIR").expect("需指定预览目录"));std::fs::create_dir_all(&dest).unwrap();
    for picker in [true,false]{
        let mut h=H::new();h.ctx.set_embed_viewports(true);
        h.app.todos.items=vec![crate::model::TodoItem{text:"提交产品方案".into(),due:None,done_on:None,reminded:false,..Default::default()}];h.app.open_task_reminder(0);
        let body=(1..=12).map(|n|format!("第 {n} 项 · 工作安排\n请确认需求与交付范围，整理设计方案和本周待办，并在评审前同步进度。相关意见请汇总后反馈给产品团队。\n\n")).collect::<String>();
        h.app.mail.reading=Some(crate::mail::Item {received_unix: None, account:"work@icloud.com".into(),from:"产品团队".into(),subject:"项目方案与下阶段工作安排".into(),time:"10-31 09:41".into(),code:None,html: String::new().into(), summary:format!("各位好，\n\n以下是项目安排与交付要求，请逐项确认。\n\n{body}最后一段\n请在周五前确认，感谢配合。").into(),links:vec![crate::mail::Link{label:"查看项目资料".into(),url:"https://example.com/project".into()}],uid:42});
        let (w,height)=if picker{(WIN_W as u32,WIN_H as u32)}else{(700,620)};let ppp=1.5;
        let mut raster=crate::preview_raster::Raster::new((w as f32*ppp)as usize,(height as f32*ppp)as usize);
        let last=if picker{89}else{7};
        for f in 0..=last{
            let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(w as f32,height as f32))),time:Some(f as f64/60.0),viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(ppp),..Default::default()})).collect(),..Default::default()};
            let out=h.ctx.run(raw,|ctx|if picker{
                let env=Env{t:f as f64/60.0,clock:"09:41:00".into(),today:chrono::Local::now().date_naive().to_string(),date:"周日".into(),..Default::default()};
                egui::CentralPanel::default().frame(egui::Frame::new()).show(ctx,|ui|{let win=ui.max_rect();h.app.frame(ui,win,&env);});
            }else{h.app.mail_reader_ui(ctx)});raster.update_textures(&out.textures_delta);
            if f==last{raster.buf.fill(if picker{[0.07,0.09,0.13,1.0]}else{[0.0,0.0,0.0,1.0]});raster.draw(&h.ctx.tessellate(out.shapes,out.pixels_per_point),out.pixels_per_point);let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);image::save_buffer(dest.join(if picker{"reminder-picker.png"}else{"mail-reader.png"}),&rgba,(w as f32*ppp)as u32,(height as f32*ppp)as u32,image::ColorType::Rgba8).unwrap();}
            raster.free_textures(&out.textures_delta);
        }
    }
}

#[test]
fn remove_icons_are_centered_in_their_buttons_at_each_scale(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for scale in [1.0,1.25,1.5,2.0]{
        let mut h=H::new();h.ctx.set_pixels_per_point(scale);
        h.app.todos.items=["第一项","第二项","第三项"].into_iter().map(|text|crate::model::TodoItem{text:text.into(),due:None,done_on:None,reminded:false,..Default::default()}).collect();
        h.app.show_page(crate::app::TODO_PAGE);h.hover(H::local(300.0,60.0),1.0);
        for i in 0..3usize{
            let button=h.ctx.read_response(egui::Id::new(("card",true,("todo-delete",i)))).unwrap().rect;
            let ink=h.shapes.iter().filter_map(|s|match &s.shape{
                egui::Shape::Text(t) if t.galley.text()=="×" && button.contains(t.visual_bounding_rect().center())=>Some(t.visual_bounding_rect()),
                egui::Shape::LineSegment{points,..} if points.iter().all(|p|button.shrink(2.0).contains(*p)) && (points[0].x-points[1].x).abs()>2.0 && (points[0].y-points[1].y).abs()>2.0=>Some(Rect::from_two_pos(points[0],points[1])),
                egui::Shape::Mesh(mesh) if mesh.texture_id!=egui::TextureId::default() && button.shrink(2.0).contains_rect(mesh.calc_bounds())=>Some(mesh.calc_bounds()),
                _=>None,
            }).fold(Rect::NOTHING,|a,b|a.union(b));
            assert!(ink.is_positive(),"删除按钮应有可见图案");
            assert!(ink.center().distance(button.center())<0.4,"缩放 {scale}：删除图案中心 {:?} 偏离按钮中心 {:?}",ink.center(),button.center());
        }
        h.click_control(("todo-delete",1usize));
        assert_eq!(h.app.todos.items.iter().map(|t|t.text.as_str()).collect::<Vec<_>>(),["第一项","第三项"]);
        assert!(h.app.todos.items.iter().all(|t|t.done_on.is_none()),"删除入口不能同时勾选其他待办");
    }
}

#[test]
#[ignore="按需检查全部页面与设置的实际排版"]
fn ui_alignment_layout_gallery(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let dest=std::path::PathBuf::from(std::env::var_os("QINGYU_UI_PREVIEW_DIR").expect("需指定预览目录"));std::fs::create_dir_all(&dest).unwrap();
    fn fixture()->H{
        let mut h=H::new();h.app.settings.island_lyrics=false;h.power=(Some(76),false);
        h.media=crate::smtc::NowPlaying{title:"愿每一个认真生活的人，都能在平凡的日子里找到属于自己的光".into(),artist:"The Lanterns · Live Session".into(),duration:240.0,position:65.0,playing:true,..Default::default()};
        h.app.todos.items=["整理会议资料","Review design details","确认项目需求与下一阶段开发排期"].into_iter().map(|text|crate::model::TodoItem{text:text.into(),due:None,done_on:None,reminded:false,..Default::default()}).collect();
        h.app.shelf.items=["项目设计方案与交付说明.pdf","design-preview.png","本周工作安排.docx","project-files.zip","notes.md"].into_iter().map(std::path::PathBuf::from).collect();
        h.app.mail.accounts=vec![crate::mail::Account {custom: None, email:"product-team@example.com".into(),provider:crate::mail::Provider::Qq,enabled:true}];
        h.app.mail.recent=(0..3).map(|uid|crate::mail::Item {received_unix: None, account:"product-team@example.com".into(),from:"产品与设计协作团队".into(),subject:"请确认下阶段的设计方案和开发排期，评审前同步最新进度".into(),time:"11-01 09:41".into(),code:if uid==0{Some("123456".into())}else{None},html: String::new().into(), summary:"这是一段邮件正文，需要确认当前需求与后续的产品设计安排。".repeat(20).into(),links:vec![],uid}).collect();
        h
    }
    let collapsed=["collapsed"];
    let scenes=["page-0","page-1","page-2","page-3","page-4","lyrics","todo-empty","shelf-empty","mail-empty","mail-detail","reminder"];
    for ppp in [1.25,2.0]{for name in scenes.into_iter().chain(collapsed){
        let mut h=fixture();let page=name.strip_prefix("page-").and_then(|p|p.parse::<usize>().ok()).unwrap_or(match name{"lyrics"=>0,"shelf-empty"=>crate::app::SHELF_PAGE,"mail-empty"|"mail-detail"=>crate::app::MAIL_PAGE,_=>crate::app::TODO_PAGE});
        if name=="todo-empty"{h.app.todos.items.clear();}if name=="shelf-empty"{h.app.shelf.items.clear();}if name=="mail-empty"{h.app.mail.accounts.clear();}
        if !collapsed.contains(&name){h.app.show_page(page);}

        if name=="lyrics"{h.app.show_lyrics=true;h.app.set_lyrics(&h.media.title,&h.media.artist,240.0,crate::lyrics::parse_lrc("[00:00]慢慢走过这段时光\n[01:00]愿每个认真生活的人都能找到属于自己的光\n[01:10]愿风带你去远方"));}
        if name=="mail-detail"{h.app.mail.open_uid=Some(("product-team@example.com".into(),0));}
        if name=="reminder"{h.app.open_task_reminder(0);}
        let (width,height)=((WIN_W*ppp)as u32,(WIN_H*ppp)as u32);let mut raster=crate::preview_raster::Raster::new(width as usize,height as usize);
        for f in 0..90{
            let t=f as f64/60.0;let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(WIN_W,WIN_H))),time:Some(t),viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(ppp),..Default::default()})).collect(),..Default::default()};
            let env=Env{t,clock:"09:41:00".into(),today:"2026-11-01".into(),date:"11月1日 周日".into(),cursor:if collapsed.contains(&name){None}else{Some(H::local(150.0,40.0))},media:h.media.clone(),battery:h.power.0,..Default::default()};
            let out=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx,|ui|{let win=ui.max_rect();h.app.frame(ui,win,&env);});});raster.update_textures(&out.textures_delta);
            if f==89{raster.buf.fill([0.07,0.09,0.13,1.0]);raster.draw(&h.ctx.tessellate(out.shapes,out.pixels_per_point),out.pixels_per_point);let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);image::save_buffer(dest.join(format!("{name}-{}.png",(ppp*100.0)as u32)),&rgba,width,height,image::ColorType::Rgba8).unwrap();}
            raster.free_textures(&out.textures_delta);
        }
    }}
    for width in [600.0,690.0]{for tab in SETTINGS_TABS{
        let mut h=fixture();h.app.settings_tab=tab;h.app.open_settings();h.ctx.set_embed_viewports(false);
        let size=egui::vec2(width,if width==600.0{450.0}else{560.0});let ppp=1.25;
        let output=std::sync::Arc::new(Mutex::new(None));let capture=output.clone();
        egui::Context::set_immediate_viewport_renderer(move|ctx,mut viewport|{let id=viewport.ids.this;let raw=egui::RawInput{viewport_id:id,screen_rect:Some(Rect::from_min_size(Pos2::ZERO,size)),viewports:std::iter::once((id,egui::ViewportInfo{native_pixels_per_point:Some(ppp),focused:Some(true),..Default::default()})).collect(),focused:true,..Default::default()};*capture.lock().unwrap()=Some(ctx.run(raw,|ctx|(viewport.viewport_ui_cb)(ctx)));});
        let (w,height)=((size.x*ppp)as u32,(size.y*ppp)as u32);let mut raster=crate::preview_raster::Raster::new(w as usize,height as usize);
        for f in 0..5{let root=h.ctx.run(egui::RawInput::default(),|ctx|h.app.settings_ui(ctx,false));raster.update_textures(&root.textures_delta);let child=output.lock().unwrap().take().unwrap();raster.update_textures(&child.textures_delta);
            if f==4{raster.buf.fill([0.0,0.0,0.0,1.0]);raster.draw(&h.ctx.tessellate(child.shapes,child.pixels_per_point),child.pixels_per_point);let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);image::save_buffer(dest.join(format!("settings-{tab}-{}.png",width as u32)),&rgba,w,height,image::ColorType::Rgba8).unwrap();}
            raster.free_textures(&root.textures_delta);raster.free_textures(&child.textures_delta);
        }
    }}
}


#[test]
fn shelf_remove_icon_stays_centered_on_hover_and_preserves_the_file(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for ppp in [1.0,1.25,1.5,2.0]{
        let mut h=H::new();h.ctx.set_pixels_per_point(ppp);
        let path=crate::model::config_dir().join("设计说明.txt");std::fs::create_dir_all(path.parent().unwrap()).unwrap();std::fs::write(&path,"保留原文件").unwrap();
        h.app.shelf.items=vec![path.clone()];h.app.show_page(crate::app::SHELF_PAGE);
        h.hover(H::local(300.0,60.0),1.0);
        let row=h.drag_response(("shelf",&path)).rect.center();let remove=h.response(("shelf-remove",&path)).rect.center();
        for pointer in [row,remove]{
            h.hover(pointer,1.0);
            let button=h.ctx.read_response(egui::Id::new(("card",true,("shelf-remove",&path)))).unwrap().rect;
            let ink=h.shapes.iter().filter_map(|s|match &s.shape{
                egui::Shape::LineSegment{points,..} if points.iter().all(|p|button.shrink(2.0).contains(*p)) && (points[0].x-points[1].x).abs()>2.0 && (points[0].y-points[1].y).abs()>2.0=>Some(Rect::from_two_pos(points[0],points[1])),
                egui::Shape::Mesh(mesh) if mesh.texture_id!=egui::TextureId::default() && button.shrink(2.0).contains_rect(mesh.calc_bounds())=>Some(mesh.calc_bounds()),_=>None,
            }).fold(Rect::NOTHING,|a,b|a.union(b));
            assert!(ink.is_positive() && ink.center().distance(button.center())<0.4,"置物架移除图案应与按钮同心");
        }
        h.click_control(("shelf-remove",&path));assert!(h.app.shelf.items.is_empty());assert!(path.exists());
    }
}

#[test]
fn resident_components_can_be_dragged_saved_and_reset(){
    use crate::bar_layout::{MUSIC,CLOCK};
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    fn draw(h:&mut H,events:Vec<egui::Event>)->[Pos2;2]{
        let mut points=[Pos2::ZERO;2];
        h.t+=0.02;
        let out=h.ctx.run(egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(600.0,300.0))),time:Some(h.t),events,..Default::default()},|ctx|{egui::CentralPanel::default().show(ctx,|ui|{h.app.resident_order_settings(ui);for (index,slot) in [MUSIC,CLOCK].into_iter().enumerate(){points[index]=ui.ctx().read_response(ui.id().with(("resident-icon",slot))).unwrap().rect.center();}});});
        h.shapes=out.shapes;points
    }
    fn point(h:&H,label:&str)->Pos2{h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t) if t.galley.text()==label=>Some(t.visual_bounding_rect().center()),_=>None}).unwrap()}
    draw(&mut h,vec![]);let [from,to]=draw(&mut h,vec![]);
    let button=|pos,pressed|egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()};
    draw(&mut h,vec![egui::Event::PointerMoved(from),button(from,true)]);
    draw(&mut h,vec![egui::Event::PointerMoved(from+egui::vec2(0.0,10.0))]);
    draw(&mut h,vec![egui::Event::PointerMoved(to)]);
    for _ in 0..3{draw(&mut h,vec![]);assert_eq!(h.app.settings.resident_order.first(),Some(&MUSIC),"静止指针不能让宽窄不同的组件反复换位");}
    draw(&mut h,vec![button(to,false)]);
    assert_eq!(h.app.settings.resident_order.first(),Some(&MUSIC));
    assert!(h.app.settings.resident_order.contains(&CLOCK));
    h.app.settings.save();assert_eq!(crate::model::Settings::load().resident_order,h.app.settings.resident_order);
    let [from,to]=draw(&mut h,vec![]);
    draw(&mut h,vec![egui::Event::PointerMoved(from),button(from,true)]);
    draw(&mut h,vec![egui::Event::PointerMoved(from+egui::vec2(0.0,10.0))]);
    draw(&mut h,vec![egui::Event::PointerMoved(to)]);
    for _ in 0..3{draw(&mut h,vec![]);assert_eq!(h.app.settings.resident_order.first(),Some(&CLOCK),"向右换位后也应保持稳定");}
    draw(&mut h,vec![button(to,false)]);
    let reset=point(&h,"恢复默认");draw(&mut h,vec![egui::Event::PointerMoved(reset),button(reset,true)]);draw(&mut h,vec![button(reset,false)]);
    assert!(h.app.settings.resident_order.is_empty());
}

#[test]
fn mailbox_scroll_reaches_older_mail_without_changing_pages() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.mail.accounts=vec![crate::mail::Account {custom: None, email:"test@qq.com".into(),provider:crate::mail::Provider::Qq,enabled:true}];
    h.app.mail.recent=(1..=8).map(|uid|crate::mail::Item {received_unix: None, account:"test@qq.com".into(),from:format!("来源 {uid}"),subject:format!("邮件 {uid}"),time:"11-03 09:41".into(),code:Some(format!("{uid:06}")),html: String::new().into(), summary:String::new().into(),links:vec![],uid}).collect();
    for item in h.app.mail.recent.clone(){h.app.mail.privacy.observe(&item,true,h.app.t,chrono::Utc::now().timestamp());}
    h.app.show_page(crate::app::MAIL_PAGE);h.hover(H::local(200.0,90.0),1.0);
    h.frame(vec![egui::Event::MouseWheel{unit:egui::MouseWheelUnit::Point,delta:egui::vec2(0.0,-900.0),modifiers:Default::default()}]);
    h.hover(H::local(200.0,90.0),0.6);
    assert_eq!(h.app.page(),crate::app::MAIL_PAGE,"滚动邮箱列表不能切换页面");
    let row=h.ctx.read_response(egui::Id::new(("card",true,("mail-row",8u32,"test@qq.com")))).expect("第八封邮件应可访问");
    h.click(row.rect.center());
    assert_eq!(h.app.mail.open_uid,Some(("test@qq.com".into(),8)));
}

#[test]
#[ignore="按需检查邮箱列表和验证码来源的实际画面"]
fn mailbox_readability_gallery(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let dest=std::path::PathBuf::from(std::env::var_os("QINGYU_MAIL_PREVIEW").expect("需指定预览目录"));std::fs::create_dir_all(&dest).unwrap();
    for variant in 0..5 {
        let multiple=variant==1;let detail=variant==2;
        let mut h=H::new();h.app.settings.mail_sync_operations=true;
        h.app.mail.accounts=vec![crate::mail::Account {custom: None, email:"work@qq.com".into(),provider:crate::mail::Provider::Qq,enabled:true}];
        if multiple {h.app.mail.accounts.push(crate::mail::Account {custom: None, email:"personal@gmail.com".into(),provider:crate::mail::Provider::Gmail,enabled:true});}
        h.app.mail.recent=[("GitHub","登录验证码 · 本次登录有效期 10 分钟",Some("274630")),("Figma","产品设计评审 · 查看本轮修改意见",None),("Notion","确认你的邮箱地址",Some("815926")),("产品团队","周五项目安排与交付清单",None),("Microsoft","你的账户安全通知",None)].into_iter().enumerate().map(|(i,(from,subject,code))|crate::mail::Item {html:String::new().into(),received_unix: None, account:if multiple&&i%2==1{"personal@gmail.com"}else{"work@qq.com"}.into(),from:from.into(),subject:subject.into(),time:format!("11-03 09:{:02}",41-i),code:code.map(str::to_string),summary:String::new().into(),links:vec![],uid:i as u32+1}).collect();
        if detail {
            h.app.mail.recent[0].summary="你好，\n\n你的登录验证码是 274630，有效期为 10 分钟。\n\n如果这不是你本人的操作，请忽略这封邮件。请勿将验证码透露给他人。\n\n这封邮件由系统自动发送，无需回复。".into();
            h.app.mail.open_uid=Some(("work@qq.com".into(),1));
        }
        if variant==4{for item in h.app.mail.recent.clone(){h.app.mail.privacy.observe(&item,true,0.0,chrono::Utc::now().timestamp());}}
        for account in h.app.mail.accounts.clone(){
            h.app.mail.manage.identities.insert(account.email.clone(),account.clone());
            let entries=h.app.mail.recent.iter().filter(|m|m.account==account.email).map(|m|(m.uid,m.uid%2==0)).collect();
            h.app.mail.manage.data.insert((account.email,crate::mail_manage::Folder::Inbox),crate::mail_manage::Snapshot{validity:7,entries,items:vec![]});
        }
        if variant==3{
            h.app.mail.manage.trash=h.app.mail.recent.iter().take(2).cloned().map(|mut m|{m.code=None;m}).collect();
            h.app.mail.manage.trash_open=true;
            h.app.mail.manage.data.insert(("work@qq.com".into(),crate::mail_manage::Folder::Trash),crate::mail_manage::Snapshot{validity:9,entries:vec![(1,true),(2,true)],items:vec![]});
        }
        h.app.show_page(crate::app::MAIL_PAGE);
        let ppp=1.5;let mut raster=crate::preview_raster::Raster::new((crate::app::CANVAS_W*ppp)as usize,(WIN_H*ppp)as usize);
        for frame in 0..100 {
            let t=frame as f64/60.0;let env=Env{t,clock:"09:41:00".into(),cursor:Some(H::local(if variant==1{350.0}else{200.0},64.0)),..Default::default()};
            let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(crate::app::CANVAS_W,WIN_H))),time:Some(t),events:vec![egui::Event::PointerMoved(H::local(if variant==1{350.0}else{200.0},64.0))],viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(ppp),..Default::default()})).collect(),..Default::default()};
            let out=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::new()).show(ctx,|ui|{h.app.frame(ui,Rect::from_min_size(Pos2::ZERO,egui::vec2(WIN_W,WIN_H)),&env);});});
            raster.update_textures(&out.textures_delta);
            if frame==99 {raster.buf.fill([0.05,0.07,0.10,1.0]);raster.draw(&h.ctx.tessellate(out.shapes,out.pixels_per_point),out.pixels_per_point);let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);let image=image::RgbaImage::from_raw((crate::app::CANVAS_W*ppp)as u32,(WIN_H*ppp)as u32,rgba).unwrap();image::imageops::crop_imm(&image,((WIN_W-PW)/2.0*ppp-10.0)as u32,0,(PW*ppp+20.0)as u32,if detail{370}else{225}).to_image().save(dest.join(if detail{"邮件详情.png"}else if variant==3{"回收站.png"}else if variant==4{"验证码分栏.png"}else if multiple{"悬停操作.png"}else{"邮箱列表.png"})).unwrap();}
            raster.free_textures(&out.textures_delta);
        }
    }
}

#[test]
fn mailbox_large_code_card_preserves_copy_and_identifies_source(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.mail.accounts=["work@qq.com","personal@gmail.com"].into_iter().map(|email|crate::mail::Account {custom: None, email:email.into(),provider:crate::mail::Provider::Qq,enabled:true}).collect();
    h.app.mail.recent=[("work@qq.com","GitHub","274630"),("personal@gmail.com","Notion","815926")].into_iter().map(|(account,from,code)|crate::mail::Item {received_unix: None, account:account.into(),from:from.into(),subject:"登录验证码".into(),time:"09:41".into(),code:Some(code.into()),html: String::new().into(), summary:String::new().into(),links:vec![],uid:42}).collect();
    for item in h.app.mail.recent.clone(){h.app.mail.privacy.observe(&item,true,h.app.t,chrono::Utc::now().timestamp());}
    h.app.show_page(crate::app::MAIL_PAGE);h.hover(H::local(200.0,90.0),1.0);
    assert!(h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(t) if t.galley.text()=="GitHub")));
    h.cmds.clear();h.click(H::local(60.0,70.0));
    assert!(h.cmds.iter().any(|cmd|matches!(cmd,crate::app::Cmd::Copy(value) if value=="274630")));
    assert!(h.app.mail.open_uid.is_none(),"复制验证码不能误开邮件详情");
    assert_eq!(h.app.expanded_height(),crate::control_center::TOP_H,"邮箱沿用统一展开高度");
}

#[test]
fn hidden_components_do_not_keep_static_pages_animating(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.app.settings.pet=false;
    h.app.show_page(crate::app::TODO_PAGE);h.hover(H::local(300.0,60.0),2.0);
    assert!(!h.animating);
    let mut moving=crate::anim::Spring::new(0.0,230.0,0.9);moving.target=200.0;
    h.app.shelf_x.insert("hidden.txt".into(),moving);
    h.frame(vec![]);
    assert!(!h.animating,"离开置物页后，未完成的文件动画不能让其他页面持续刷新");
    h.app.shelf_x.clear();
    h.hover(H::local(300.0,60.0),1.5);
    assert!(!h.animating,"待办页静止时不应强制连续刷新");
}

#[test]
fn minimal_music_stops_hidden_playback_animations(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.set_minimal(true);
    h.media=crate::smtc::NowPlaying{title:"歌曲".into(),artist:"歌手".into(),playing:true,..Default::default()};
    for _ in 0..180{h.frame(vec![]);}
    assert!(!h.app.open);
    assert!(!h.animating,"极简模式隐藏媒体组件后，不应继续为播放动画刷新");
}

#[test]
fn native_app_icon_uses_bounded_resolution(){
    let icon=crate::theme::app_icon();
    assert!(icon.width<=128 && icon.height<=128,"原生窗口图标不需要常驻完整的 1024 像素品牌原图");
    assert_eq!(icon.rgba.len(),(icon.width*icon.height*4)as usize);
    assert!(std::sync::Arc::ptr_eq(&icon,&crate::theme::app_icon()));
}

#[test]
fn long_mail_lists_only_layout_the_visible_rows(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.mail.accounts=vec![crate::mail::Account {custom: None, email:"work@qq.com".into(),provider:crate::mail::Provider::Qq,enabled:true}];
    h.app.mail.recent=(0..60).map(|i|crate::mail::Item {received_unix: None, account:"work@qq.com".into(),from:format!("发件人 {i}"),subject:format!("第 {i} 封邮件标题"),time:"09:41".into(),code:None,html: String::new().into(), summary:String::new().into(),links:vec![],uid:i}).collect();
    h.app.show_page(crate::app::MAIL_PAGE);h.hover(H::local(300.0,60.0),1.0);
    let count=h.ctx.fonts(|fonts|fonts.num_galleys_in_cache());
    assert!(count<100,"60 封邮件不应全部排版，当前缓存了 {count} 个文字布局");
    for _ in 0..15{h.wheel(-40.0);h.hover(H::local(300.0,60.0),0.1);}
    assert_eq!(h.app.page,crate::app::MAIL_PAGE,"列表虚拟化后仍应在邮箱页滚动");
}


#[test]
fn page_order_persists_and_controls_navigation() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    h.app.settings.page_order=[3,2,0,1,6,4,5,7,8];
    h.app.settings.save();
    let settings=crate::model::Settings::load();
    assert_eq!(settings.page_order,[3,2,0,1,6,4,5,7,8]);
    let restarted=App::new(&h.ctx,Options { fetch_lyrics: false, monitor_agents: false });
    assert_eq!(restarted.page(),crate::app::MAIL_PAGE);
    for page in [crate::app::SHELF_PAGE,crate::app::CLIPBOARD_PAGE,crate::app::WEATHER_PAGE] {h.app.set_page_visible(page,false);}
    h.app.show_page(crate::app::MUSIC_PAGE);h.hover(Pos2::new(WIN_W/2.0,12.0),1.0);
    for expected in [Some(crate::app::AGENT_PAGE),Some(crate::app::MAC_STATS_PAGE),Some(crate::app::NOTIFICATIONS_PAGE),None,Some(crate::app::MAIL_PAGE),Some(crate::app::TODO_PAGE),Some(crate::app::MUSIC_PAGE)] {
        h.page_key(true);
        assert_eq!(h.app.overview,expected.is_none());if let Some(page)=expected{assert_eq!(h.app.page,page);}
        h.hover(H::local(300.0,60.0),0.7);
    }
    h.page_key(false);
    assert_eq!(h.app.page,2);
    h.app.set_page_visible(2,false);
    assert_eq!(h.app.page,crate::app::MAIL_PAGE);
    for invalid in ["0,0,1,2,3,4,5,6","8,1,2,3,4,5,6,7","0,1","invalid"] {
        std::fs::write(crate::model::config_dir().join("settings.txt"),format!("page_order={invalid}\n")).unwrap();
        assert_eq!(crate::model::Settings::load().page_order,[0,1,6,2,3,4,5,7,8]);
    }
}

#[test]
fn page_order_drag_applies_on_release_and_saves() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.settings_tab=10;h.app.open_settings();h.ctx.set_embed_viewports(false);
    fn draw(h:&mut H,events:Vec<egui::Event>){
        let output=std::sync::Arc::new(Mutex::new(None));let capture=output.clone();
        h.t+=0.02;let time=h.t;
        egui::Context::set_immediate_viewport_renderer(move|ctx,mut viewport|{
            let id=viewport.ids.this;
            let raw=egui::RawInput{viewport_id:id,screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(690.0,1000.0))),time:Some(time),events:events.clone(),viewports:std::iter::once((id,egui::ViewportInfo::default())).collect(),..Default::default()};
            *capture.lock().unwrap()=Some(ctx.run(raw,|ctx|(viewport.viewport_ui_cb)(ctx)));
        });
        let _=h.ctx.run(egui::RawInput::default(),|ctx|h.app.settings_ui(ctx,false));
        let child=output.lock().unwrap().take().unwrap();h.shapes=child.shapes;
    }
    for _ in 0..3 {draw(&mut h,vec![]);}
    let point=|h:&H,label:&str|h.shapes.iter().find_map(|s|match &s.shape {egui::Shape::Text(t) if t.galley.text()==label&&t.visual_bounding_rect().left()>200.0=>Some(t.visual_bounding_rect().left_center()),_=>None}).unwrap();
    let from=point(&h,crate::app::PAGE_NAMES[crate::app::MUSIC_PAGE])+egui::vec2(-18.0,0.0);
    let to=point(&h,crate::app::PAGE_NAMES[crate::app::TODO_PAGE]);
    let button=|pos,pressed|egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()};
    draw(&mut h,vec![egui::Event::PointerMoved(from),button(from,true)]);
    draw(&mut h,vec![egui::Event::PointerMoved(from+egui::vec2(0.0,10.0))]);
    draw(&mut h,vec![egui::Event::PointerMoved(to)]);
    assert_eq!(h.app.settings.page_order,[0,1,6,2,3,4,5,7,8]);
    draw(&mut h,vec![button(to,false)]);
    assert_eq!(h.app.settings.page_order,[1,6,2,0,3,4,5,7,8]);
    assert_eq!(crate::model::Settings::load().page_order,h.app.settings.page_order);
}

#[test]
fn settings_theme_follows_system_and_preserves_island_style() {
    use crate::settings_theme::SettingsTheme;
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.app.open_settings();h.ctx.set_embed_viewports(false);
    assert_eq!(h.app.settings.settings_theme,SettingsTheme::System);
    assert_eq!(SettingsTheme::parse("unknown"),SettingsTheme::System);
    let mut raster=crate::preview_raster::Raster::new(690,1240);
    for (mode,system,dark) in [(SettingsTheme::System,false,false),(SettingsTheme::System,true,true),
        (SettingsTheme::System,false,false),(SettingsTheme::Dark,false,true),(SettingsTheme::Light,true,false)] {
        // macOS appearance follows the system; the persisted selector is retained for Windows compatibility.
        let dark=if cfg!(target_os="macos"){system}else{dark};
        h.app.settings.settings_theme=mode;
        h.app.settings.save();assert_eq!(crate::model::Settings::load().settings_theme,mode);
        h.ctx.data_mut(|d|d.insert_temp(egui::Id::new("settings-system-theme"),(std::time::Instant::now(),system)));
        let output=std::sync::Arc::new(std::sync::Mutex::new(None));let capture=output.clone();
        egui::Context::set_immediate_viewport_renderer(move|ctx,mut viewport|{
            let id=viewport.ids.this;
            let raw=egui::RawInput{viewport_id:id,screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(690.0,1240.0))),
                viewports:std::iter::once((id,egui::ViewportInfo{native_pixels_per_point:Some(1.0),..Default::default()})).collect(),..Default::default()};
            *capture.lock().unwrap()=Some(ctx.run(raw,|ctx|(viewport.viewport_ui_cb)(ctx)));
        });
        for tab in SETTINGS_TABS {
            h.app.settings_tab=tab;
            for frame in 0..3 {
                h.ctx.data_mut(|d|d.insert_temp(egui::Id::new("settings-system-theme"),(std::time::Instant::now(),system)));
                let root=h.ctx.run(Default::default(),|ctx|h.app.settings_ui(ctx,false));
                assert!(h.ctx.style().visuals.dark_mode,"设置页主题不能泄漏到岛体");
                let commands=&root.viewport_output[&egui::ViewportId::from_hash_of("qingyu-settings")].commands;
                assert!(commands.iter().any(|c|matches!(c,egui::ViewportCommand::SetTheme(t) if *t==if dark{egui::SystemTheme::Dark}else{egui::SystemTheme::Light})),"原生边框必须与页面一致");
                let child=output.lock().unwrap().take().unwrap();
                if frame==2 {
                    let pairs:&[(&str,&str)]=match tab {
                        1=>&[("时间偏移","0.35 秒")],
                        7=>&[("验证码隐藏","60 秒"),("检查间隔","120 秒"),("邮箱","输入邮箱地址"),("授权码","输入邮箱授权码")],
                        _=>&[],
                    };
                    for (label,value) in pairs {
                        let centers=|needle:&str|child.shapes.iter().filter_map(|s|match &s.shape{egui::Shape::Text(t)if t.galley.text()==needle=>Some(t.visual_bounding_rect().center().y),_=>None}).collect::<Vec<_>>();
                        let distance=centers(label).iter().flat_map(|a|centers(value).into_iter().map(move|b|(a-b).abs())).fold(f32::INFINITY,f32::min);
                        assert!(distance<=1.0,"设置页 {tab} 的 {label} 与 {value} 未居中：{distance}");
                    }
                }
                if tab==0 && frame==2 && !cfg!(target_os="macos") {
                    let center=|label:&str|child.shapes.iter().find_map(|s|match &s.shape {
                        egui::Shape::Text(t) if t.galley.text()==label=>Some(t.visual_bounding_rect().center().y),_=>None,
                    }).unwrap();
                    for (label,choices) in [("显示屏",vec!["跟随主屏"]),("时间制式",vec!["24 小时","12 小时"]),("时段文字",vec!["不显示","上午 / 下午","详细时段"])] {
                        for choice in choices {assert!((center(label)-center(choice)).abs()<1.0,"{label}与{choice}应垂直居中：{} / {}",center(label),center(choice));}
                    }
                }
                #[cfg(target_os="macos")]
                let background=crate::mac_design::palette_for(&h.ctx,dark).window;
                #[cfg(not(target_os="macos"))]
                let background=if dark{egui::Color32::from_rgb(25,25,27)}else{egui::Color32::from_rgb(246,247,249)};
                assert!(child.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Rect(r) if r.fill==background && r.rect.width()>400.0)));
                raster.update_textures(&root.textures_delta);raster.update_textures(&child.textures_delta);
                if frame==2 {
                    if let Some(dest)=std::env::var_os("QINGYU_THEME_PREVIEW_DIR") {
                        let dest=std::path::PathBuf::from(dest);std::fs::create_dir_all(&dest).unwrap();
                        raster.buf.fill([0.0,0.0,0.0,1.0]);raster.draw(&h.ctx.tessellate(child.shapes,child.pixels_per_point),child.pixels_per_point);
                        let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
                        image::save_buffer(dest.join(format!("settings-{}-{tab}.png",if dark{"dark"}else{"light"})),&rgba,690,1240,image::ColorType::Rgba8).unwrap();
                    }
                }
                raster.free_textures(&root.textures_delta);raster.free_textures(&child.textures_delta);
            }
        }
    }
}


#[test]
fn mail_report_open_settings_matrix() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.ctx.set_embed_viewports(false);
    for scale in [1.0,1.25,1.5,2.0] {
        for size in [egui::vec2(600.0,450.0),egui::vec2(690.0,560.0)] {
            egui::Context::set_immediate_viewport_renderer(move|ctx,mut viewport|{
                let id=viewport.ids.this;
                let raw=egui::RawInput{viewport_id:id,screen_rect:Some(Rect::from_min_size(Pos2::ZERO,size)),viewports:std::iter::once((id,egui::ViewportInfo{native_pixels_per_point:Some(scale),..Default::default()})).collect(),..Default::default()};
                let _=ctx.run(raw,|ctx|(viewport.viewport_ui_cb)(ctx));
            });
            for count in [0,1,6] {
                h.app.mail.accounts=crate::mail::Provider::ALL.into_iter().take(count).enumerate().map(|(i,provider)|crate::mail::Account {custom: None, email:format!("probe{i}@example.invalid"),provider,enabled:false}).collect();
                for provider in crate::mail::Provider::ALL {
                    h.app.mail.form.provider=provider;h.app.open_mail_settings();
                    for _ in 0..3 {let _=h.ctx.run(Default::default(),|ctx|h.app.settings_ui(ctx,false));}
                    assert!(h.app.settings_open);assert_eq!(h.app.settings_tab,7);
                    h.app.settings_open=false;let _=h.ctx.run(Default::default(),|ctx|h.app.settings_ui(ctx,false));
                }
            }
        }
    }
}

#[test]
fn mail_report_initial_fetch_unicode_message() {
    for space in ['\u{00a0}','\u{2002}','\u{2003}','\u{202f}'] {
        for kind in ["text/plain","text/html"] {
            let raw=format!("From: probe@example.invalid\r\nSubject: verification\r\nMIME-Version: 1.0\r\nContent-Type: {kind}; charset=utf-8\r\nContent-Transfer-Encoding: 8bit\r\n\r\n验证码：{space}834921");
            let parsed=crate::mail::parse_message(raw.as_bytes());
            assert_eq!(crate::mail::code_of(&parsed),Some("834921".into()));
        }
    }
}

#[test]
fn mail_report_save_and_failure_paths() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.app.open_mail_settings();h.ctx.set_embed_viewports(false);
    fn frame(h:&mut H,events:Vec<egui::Event>){
        let output=std::sync::Arc::new(Mutex::new(None));let capture=output.clone();
        egui::Context::set_immediate_viewport_renderer(move|ctx,mut viewport|{
            let id=viewport.ids.this;
            let raw=egui::RawInput{viewport_id:id,screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(900.0,1800.0))),events:events.clone(),viewports:std::iter::once((id,egui::ViewportInfo::default())).collect(),..Default::default()};
            *capture.lock().unwrap()=Some(ctx.run(raw,|ctx|(viewport.viewport_ui_cb)(ctx)));
        });
        let _=h.ctx.run(Default::default(),|ctx|h.app.settings_ui(ctx,false));
        h.shapes=output.lock().unwrap().take().unwrap().shapes;
    }
    fn click(h:&mut H,label:&str){
        frame(h,vec![]);frame(h,vec![]);
        let point=h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t)if t.galley.text()==label=>Some(t.visual_bounding_rect().center()),_=>None}).unwrap();
        frame(h,vec![egui::Event::PointerMoved(point)]);
        for pressed in [true,false]{frame(h,vec![egui::Event::PointerButton{pos:point,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()}]);}
    }
    for button in ["测试连接","保存并开始监控"] {
        h.app.mail.form.email.clear();h.app.mail.form.password.clear();click(&mut h,button);
        assert_eq!(h.app.mail.note,"请先填邮箱地址");
        h.app.mail.form.email="probe@example.invalid".into();click(&mut h,button);
        assert!(h.app.mail.note.contains("请先填授权码"));assert!(h.app.mail.accounts.is_empty());
    }
    let email=format!("qingyu-crash-probe-{}@example.invalid",std::process::id());
    struct Cleanup(String);impl Drop for Cleanup{fn drop(&mut self){crate::mail::secrets::delete(&self.0);}}
    let _cleanup=Cleanup(email.clone());
    h.app.mail.form.email=email.clone();h.app.mail.form.password="synthetic-test-secret".into();
    click(&mut h,"保存并开始监控");
    assert_eq!(h.app.mail.accounts.len(),1);assert!(h.app.mail.form.password.is_empty());
    assert_eq!(crate::mail::load_accounts()[0].email,email);
    assert_eq!(crate::mail::secrets::load(&email).unwrap(),"synthetic-test-secret");
    h.app.mail_event(crate::mail::Event::Status{account:email.clone(),status:crate::mail::Status::Failed("模拟认证失败".into())});frame(&mut h,vec![]);
    h.app.mail.form.password="synthetic-test-secret".into();click(&mut h,"测试连接");
    assert_eq!(h.app.mail.testing.as_deref(),Some(email.as_str()));
    h.app.mail_event(crate::mail::Event::Test{account:email,ok:false,note:"模拟网络超时".into()});frame(&mut h,vec![]);
    assert!(h.app.mail.testing.is_none());assert!(h.app.mail.note.contains("模拟网络超时"));
}

#[test]
fn hover_tooltips_fit_native_region_and_release_it_after_leaving(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for shelf in [false,true] {
        let mut h=H::new();
        let label;
        let pointer;
        if shelf {
            let path=crate::model::config_dir().join("开发项目源代码与当前维护的项目文件夹");
            std::fs::create_dir_all(&path).unwrap();
            label=path.display().to_string();h.app.shelf.add(&path).unwrap();
            h.app.show_page(crate::app::SHELF_PAGE);h.hover(H::local(300.0,60.0),1.0);
            pointer=h.drag_response(("shelf",&path)).rect.center();
        } else {
            h.app.settings.island_lyrics=true;
            h.media.title="测试歌曲".into();h.media.artist="歌手".into();h.media.duration=240.0;h.media.playing=true;
            h.app.set_lyrics("测试歌曲","歌手",240.0,crate::lyrics::parse_lrc("[00:00]测试歌词"));
            h.app.alert("邮件验证码","收到邮件，验证码 123456，请在十分钟内使用",crate::app::MAIL_PAGE,false);
            h.hover(Pos2::new(600.0,220.0),1.0);
            assert!(!h.app.settings.notification_preview);
            label="邮箱提醒\n打开相关功能查看\n邮箱\n点击打开对应内容".to_string();
            pointer=h.response("notice-summary").rect.center();
        }
        h.hover(pointer,0.4);
        let hit_before_tooltip=h.hit;
        h.hover(pointer,1.1);
        let bounds=h.shapes.iter().find_map(|s|match &s.shape {
            egui::Shape::Text(t) if t.galley.text()==label=>Some(t.visual_bounding_rect()),_=>None,
        }).expect("悬停后应显示完整提示");
        if !shelf {
            assert!(h.app.history.iter().chain(&h.app.alerts).chain(h.app.notice.iter()).all(|a|!a.title.contains("123456")&&!a.body.contains("123456")),"关闭预览时通知内存中不能留下验证码明文");
            assert!(!h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text().contains("123456"))),"工具提示也应使用隐私文案");
        }
        assert!(h.region.contains_rect(bounds),"悬停提示不能被原生窗口裁掉：{bounds:?} / {:?}",h.region);
        assert_eq!(h.hit,hit_before_tooltip,"提示不应扩大岛体交互区域");
        h.hover(Pos2::new(900.0,250.0),3.0);
        assert!(h.region.bottom()<100.0,"提示消失后应恢复收起区域");
    }
}

#[test]
fn clock_12h_formats_boundaries_and_persists() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    assert!(!h.app.settings.clock_12h);
    assert_eq!(h.app.settings.clock_text("23:59:00"),"23:59");
    assert_eq!(h.app.settings.clock_period,0);
    h.app.settings.clock_12h=true;
    assert_eq!(h.app.settings.clock_text("00:00"),"12:00");
    assert_eq!(h.app.settings.clock_text("23:59"),"11:59");
    h.app.settings.clock_period=1;
    for (input,expected) in [("00:00:00","上午 12:00"),("11:59:00","上午 11:59"),("12:00:00","下午 12:00"),("13:05:00","下午 1:05"),("23:59:00","下午 11:59")] {
        assert_eq!(h.app.settings.clock_text(input),expected);
    }
    assert_eq!(h.app.settings.clock_text("13:05"),"下午 1:05");
    h.app.settings.save();
    assert!(crate::model::Settings::load().clock_12h);

    h.app.settings.resident_order=vec![crate::bar_layout::CLOCK,crate::bar_layout::MUSIC];
    for _ in 0..90 { h.frame(vec![]); }
    let text_rect=|label:&str|h.shapes.iter().find_map(|s|match &s.shape {
        egui::Shape::Text(t) if t.galley.text()==label=>Some(t.visual_bounding_rect()),_=>None,
    }).unwrap();
    assert!(text_rect("上午").right()<text_rect("9:41").left(),"时段和时钟不能重叠");
    h.hover(H::local(200.0,40.0),1.0);
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="9:41")));
    h.app.settings.clock_period=0;
    h.app.settings.clock_12h=false;
    h.app.settings.save();
    assert!(!crate::model::Settings::load().clock_12h);
    h.frame(vec![]);
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="09:41")));
}

#[test]
fn clock_period_boundaries_and_both_formats() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    for twelve in [false,true] {
        h.app.settings.clock_12h=twelve;
        h.app.settings.clock_period=2;
        for (time,label) in [("00:00","凌晨"),("05:59","凌晨"),("06:00","上午"),("10:59","上午"),("11:00","中午"),("12:59","中午"),("13:00","下午"),("16:59","下午"),("17:00","傍晚"),("18:59","傍晚"),("19:00","晚上"),("23:59","晚上")] {
            assert!(h.app.settings.clock_text(time).starts_with(&format!("{label} ")));
        }
        assert_eq!(h.app.settings.clock_text("19:30"),if twelve {"晚上 7:30"}else{"晚上 19:30"});
        for mode in 0..=2 {
            h.app.settings.clock_period=mode;
            h.app.settings.save();
            let loaded=crate::model::Settings::load();
            assert_eq!(loaded.clock_period,mode);
            assert_eq!(loaded.clock_12h,twelve);
        }
    }
}

#[test]
fn clock_period_visually_centers_with_digits() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let h=H::new();
    let mut raster=crate::preview_raster::Raster::new(420,230);
    for scale in [1.0,1.25,1.5,2.0] {
        let output=h.ctx.run(egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(420.0,230.0))),..Default::default()},|ctx|{
            egui::CentralPanel::default().show(ctx,|ui|{
                let mut c=crate::paint::Card::new(ui,Pos2::ZERO,ui.max_rect());c.s=scale;
                for (i,period) in ["凌晨","上午","中午","下午","傍晚","晚上"].iter().enumerate() {
                    for (j,size) in [12.5,15.5].iter().enumerate() {
                        crate::paint::clock(&c,10.0+j as f32*100.0,18.0+i as f32*18.0,&format!("{period} 12:59"),*size);
                    }
                }
            });
        });
        let texts:Vec<_>=output.shapes.iter().filter_map(|s|match &s.shape{egui::Shape::Text(t)=>Some(t),_=>None}).collect();
        for pair in texts.chunks_exact(2) {
            assert!((pair[0].visual_bounding_rect().center().y-pair[1].visual_bounding_rect().center().y).abs()<0.1,"时段文字与数字应按可见字形垂直居中，缩放 {scale}");
        }
        raster.update_textures(&output.textures_delta);
        if scale==2.0 {
            if let Some(dest)=std::env::var_os("QINGYU_CLOCK_ALIGNMENT_PREVIEW") {
                raster.buf.fill([0.0,0.0,0.0,1.0]);raster.draw(&h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);
                let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
                image::save_buffer(dest,&rgba,420,230,image::ColorType::Rgba8).unwrap();
            }
        }
    }
}

#[test]
fn avoidance_picker_add_remove_and_save() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    h.ctx.set_visuals(egui::Visuals::dark());
    assert!(h.app.settings.avoid_browsers);
    assert!(h.app.settings.avoid_apps.is_empty());
    h.app.settings.browser_minimal=true;
    h.app.avoidance.entries=vec![
        crate::avoidance::Candidate{path:r"D:\Quark\quark.exe".into(),name:"夸克浏览器".into(),title:"新标签页".into(),browser:true},
        crate::avoidance::Candidate{path:r"C:\Windows\System32\notepad.exe".into(),name:"记事本".into(),title:"未命名 - 记事本".into(),browser:false},
    ];
    fn draw(h:&mut H,events:Vec<egui::Event>)->egui::FullOutput {
        h.t+=1.0/60.0;
        h.ctx.run(egui::RawInput{time:Some(h.t),screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(600.0,560.0))),events,..Default::default()},|ctx|{
            egui::CentralPanel::default().show(ctx,|ui|{
                ui.heading("常规");
                h.app.avoidance.settings(ui,&mut h.app.settings);
            });
            h.app.avoidance.dialog(ctx,&mut h.app.settings);
        })
    }
    fn click(h:&mut H,point:Pos2){
        draw(h,vec![egui::Event::PointerMoved(point),egui::Event::PointerButton{pos:point,button:egui::PointerButton::Primary,pressed:true,modifiers:Default::default()}]);
        draw(h,vec![egui::Event::PointerMoved(point),egui::Event::PointerButton{pos:point,button:egui::PointerButton::Primary,pressed:false,modifiers:Default::default()}]);
    }
    fn point(out:&egui::FullOutput,label:&str)->Pos2 {
        out.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t) if t.galley.text()==label=>Some(t.visual_bounding_rect().center()),_=>None}).unwrap_or_else(||panic!("未找到 {label}"))
    }
    h.app.avoidance.open=true;
    let mut raster=crate::preview_raster::Raster::new(600,560);
    let mut out=draw(&mut h,vec![]);raster.update_textures(&out.textures_delta);
    for _ in 0..40 {std::thread::sleep(std::time::Duration::from_millis(10));out=draw(&mut h,vec![]);raster.update_textures(&out.textures_delta);}
    assert!(out.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="浏览器规则已覆盖")));
    if let Some(dest)=std::env::var_os("QINGYU_AVOID_PREVIEW_DIR") {
        std::fs::create_dir_all(&dest).unwrap();raster.buf.fill([0.0,0.0,0.0,1.0]);
        raster.draw(&h.ctx.tessellate(out.shapes.clone(),out.pixels_per_point),out.pixels_per_point);
        let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);image::save_buffer(std::path::Path::new(&dest).join("添加应用.png"),&rgba,600,560,image::ColorType::Rgba8).unwrap();
    }
    click(&mut h,point(&out,"记事本"));
    let out=draw(&mut h,vec![]);
    click(&mut h,point(&out,"添加"));
    assert!(!h.app.avoidance.open);
    assert_eq!(h.app.settings.avoid_apps.len(),1);
    h.app.settings.avoid_browsers=false;h.app.settings.save();
    let loaded=crate::model::Settings::load();
    assert!(!loaded.avoid_browsers);assert!(loaded.browser_minimal);
    assert_eq!(loaded.avoid_apps,h.app.settings.avoid_apps);
    let mut out=draw(&mut h,vec![]);raster.update_textures(&out.textures_delta);
    for _ in 0..40{std::thread::sleep(std::time::Duration::from_millis(20));out=draw(&mut h,vec![]);raster.update_textures(&out.textures_delta);}
    assert!(!out.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="记事本")),"图标区不应常驻显示应用名称");
    if let Some(dest)=std::env::var_os("QINGYU_AVOID_PREVIEW_DIR") {
        raster.buf.fill([0.0,0.0,0.0,1.0]);raster.draw(&h.ctx.tessellate(out.shapes.clone(),out.pixels_per_point),out.pixels_per_point);
        let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);image::save_buffer(std::path::Path::new(&dest).join("图标区域.png"),&rgba,600,560,image::ColorType::Rgba8).unwrap();
    }
    click(&mut h,point(&out,"×"));assert!(h.app.settings.avoid_apps.is_empty());
    h.app.settings.save();assert!(crate::model::Settings::load().avoid_apps.is_empty());
}

#[test]
fn six_minutes_without_interaction_do_not_restart_static_animation_or_grow_textures(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.settings.pet=false;
    for _ in 0..120{h.frame(vec![]);}
    let textures=h.ctx.tex_manager().read().num_allocated();
    for second in 3..=363 {
        h.t=second as f64;h.system_idle=Some(second as f64);h.frame(vec![]);
        assert!(!h.animating,"空闲到第 {second} 秒不应重新进入连续动画");
        assert_eq!(h.ctx.tex_manager().read().num_allocated(),textures,"空闲纹理数不能持续增长");
    }
}

#[test]
fn avoidance_icon_area_stays_one_row_and_scrolls() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.ctx.set_visuals(egui::Visuals::dark());h.app.settings.browser_minimal=true;
    fn draw(h:&mut H,events:Vec<egui::Event>)->egui::FullOutput {
        h.t+=1.0/60.0;
        h.ctx.run(egui::RawInput{time:Some(h.t),screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(420.0,560.0))),events,..Default::default()},|ctx|{
            egui::CentralPanel::default().show(ctx,|ui|{h.app.avoidance.settings(ui,&mut h.app.settings);});
        })
    }
    fn text(out:&egui::FullOutput,label:&str)->Rect {
        out.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t) if t.galley.text()==label=>Some(t.visual_bounding_rect()),_=>None}).unwrap()
    }
    let footer="无需全屏或最大化；窗口移开或切换应用后恢复。";
    h.app.settings.avoid_apps=vec![("D:/sample/A.exe".into(),"A应用".into())];
    draw(&mut h,vec![]);let one=draw(&mut h,vec![]);let baseline=text(&one,footer).top();
    let cross=text(&one,"×").center();
    assert!(one.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Circle(c) if c.radius<=7.1 && c.center.distance(cross)<3.0)),"移除角标应为小圆点，不能被按钮最小尺寸撑大");
    h.app.settings.avoid_apps=(0..30).map(|i|(format!("D:/sample/{i}.exe"),format!("{}应用",char::from_u32(65+i).unwrap()))).collect();
    draw(&mut h,vec![]);let many=draw(&mut h,vec![]);
    assert!((text(&many,footer).top()-baseline).abs()<1.0,"应用增多时始终保持单行高度");
    let first=text(&many,"A");let pointer=first.center();
    draw(&mut h,vec![egui::Event::PointerMoved(pointer),egui::Event::MouseWheel{unit:egui::MouseWheelUnit::Point,delta:egui::vec2(0.0,-150.0),modifiers:Default::default()}]);
    let mut scrolled=draw(&mut h,vec![]);for _ in 0..30{scrolled=draw(&mut h,vec![]);}
    assert!((text(&scrolled,footer).top()-text(&many,footer).top()).abs()<1.0,"滚动图标不能移动区域下方内容");
    assert!(scrolled.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="A" && t.visual_bounding_rect().left()<first.left()-10.0)) || !scrolled.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="A")),"应能滚动查看后续应用");
}

#[test]
fn compact_canvas_preserves_panel_and_notice_pixels(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    fn draw(width:f32,page:Option<usize>,notice:bool)->Vec<u8>{
        let mut h=H::new();h.app.settings.island_lyrics=true;
        let media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),playing:true,duration:120.0,..Default::default()};
        h.app.set_lyrics("Track","Artist",120.0,crate::lyrics::parse_lrc("[00:00]晚风吹过了山岗\n[01:00]下一句"));
        if let Some(page)=page{h.app.show_page(page);}
        let mut raster=crate::preview_raster::Raster::new(1280,WIN_H as usize);
        let mut rgba=Vec::new();
        for frame in 0..180 {
            if notice && frame==60{h.app.alert("收到新邮件","会议通知及验证码 834921",crate::app::MAIL_PAGE,true);}
            let click=(notice && (frame==90 || frame==91)).then(||Pos2::new(WIN_W/2.0+(h.app.current_width()-44.0)/2.0+26.0,15.0));
            let t=frame as f64/60.0;
            let env=Env{t,clock:"09:41:00".into(),today:"2026-10-28".into(),date:"10月28日 周三".into(),media:media.clone(),cursor:click.or_else(||page.map(|_|H::local(300.0,40.0))),..Default::default()};
            let output=h.ctx.run(egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(width,WIN_H))),time:Some(t),events:click.map(|pos|vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed:frame==90,modifiers:Default::default()}]).unwrap_or_default(),..Default::default()},|ctx|{
                egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx,|ui|{h.app.frame(ui,Rect::from_min_size(Pos2::ZERO,Vec2::new(WIN_W,WIN_H)),&env);});
            });
            raster.update_textures(&output.textures_delta);
            if [30,65,95,179].contains(&frame){
                raster.buf.fill([0.0;4]);raster.draw(&h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);
                let mut pixels=Vec::new();raster.to_rgba8(&mut pixels);
                if page.is_none() && notice && frame==95 {
                    if let Some(dir)=std::env::var_os("QINGYU_GRAPHICS_PREVIEW") {let dir=std::path::PathBuf::from(dir);std::fs::create_dir_all(&dir).unwrap();image::save_buffer(dir.join(format!("canvas-{width}.png")),&pixels,1280,WIN_H as u32,image::ColorType::Rgba8).unwrap();}
                }
                rgba.extend(pixels);
            }
        }
        rgba
    }
    for page in std::iter::once(None).chain((0..crate::app::PAGE_COUNT).map(Some)) {
        for notice in [false,true] {
            assert_eq!(draw(1280.0,page,notice),draw(960.0,page,notice),"缩减透明余量不能改变页面 {page:?} 和通知 {notice} 的像素");
        }
    }
}


#[test]
fn hidden_mail_masks_every_display_field_and_reveal_is_temporary(){
 let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
 let i=crate::mail::Item{received_unix:None,account:"a@example.com".into(),from:"123456".into(),subject:"验证码 123456".into(),time:"09:41".into(),code:Some("123456".into()),html:"<h1>验证码</h1><p><strong>123456</strong></p>".into(), summary:"验证码 1 2 3-4 5 6".into(),links:vec![],uid:42};
 h.app.mail.privacy.observe(&i,true,0.0,100);h.app.t=61.0;
 let shown=h.app.display_mail(&i);assert!(shown.code.is_none());for text in [shown.from.as_str(),shown.subject.as_str(),shown.summary.as_ref()]{assert!(!text.contains('1'));}
 assert!(shown.html.is_empty(),"隐藏验证码时 HTML 原文也必须隐藏");
 assert!(!h.app.mail_body_lines(&shown).iter().any(|line|line.text.contains("123456")));
 h.app.reveal_mail_code(&i);let revealed=h.app.display_mail(&i);assert!(revealed.code.is_some());
 assert_eq!(revealed.html,i.html);assert!(h.app.mail_body_lines(&revealed).iter().any(|line|line.text.contains("123456")));
 h.app.t+=15.0;let hidden=h.app.display_mail(&i);assert!(hidden.code.is_none()&&hidden.html.is_empty());
}

#[test]
fn compact_font_atlas_preserves_panel_and_notice_pixels(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    fn draw(texture_side:usize,page:Option<usize>,notice:bool)->Vec<u8>{
        let mut h=H::new();h.app.settings.island_lyrics=true;
        let media=crate::smtc::NowPlaying{title:"Track".into(),artist:"Artist".into(),playing:true,duration:120.0,..Default::default()};
        h.app.set_lyrics("Track","Artist",120.0,crate::lyrics::parse_lrc("[00:00]晚风吹过了山岗\n[01:00]下一句"));
        if let Some(page)=page{h.app.show_page(page);}
        let mut raster=crate::preview_raster::Raster::new(1280,WIN_H as usize);
        let mut rgba=Vec::new();
        for frame in 0..180 {
            if notice && frame==60{h.app.alert("收到新邮件","会议通知及验证码 834921",crate::app::MAIL_PAGE,true);}
            let click=(notice && (frame==90 || frame==91)).then(||Pos2::new(WIN_W/2.0+(h.app.current_width()-44.0)/2.0+26.0,15.0));
            let t=frame as f64/60.0;
            let env=Env{t,clock:"09:41:00".into(),today:"2026-10-28".into(),date:"10月28日 周三".into(),media:media.clone(),cursor:click.or_else(||page.map(|_|H::local(300.0,40.0))),..Default::default()};
            let output=h.ctx.run(egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(960.0,WIN_H))),max_texture_side:Some(texture_side),time:Some(t),events:click.map(|pos|vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed:frame==90,modifiers:Default::default()}]).unwrap_or_default(),..Default::default()},|ctx|{
                egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx,|ui|{h.app.frame(ui,Rect::from_min_size(Pos2::ZERO,Vec2::new(WIN_W,WIN_H)),&env);});
            });
            raster.update_textures(&output.textures_delta);
            if [30,65,95,179].contains(&frame){
                raster.buf.fill([0.0;4]);raster.draw(&h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);
                let mut pixels=Vec::new();raster.to_rgba8(&mut pixels);
                if page.is_none() && notice && frame==95 {
                    if let Some(dir)=std::env::var_os("QINGYU_GRAPHICS_PREVIEW") {let dir=std::path::PathBuf::from(dir);std::fs::create_dir_all(&dir).unwrap();image::save_buffer(dir.join(format!("atlas-{texture_side}.png")),&pixels,1280,WIN_H as u32,image::ColorType::Rgba8).unwrap();}
                }
                rgba.extend(pixels);
            }
        }
        rgba
    }
    for page in std::iter::once(None).chain((0..crate::app::PAGE_COUNT).map(Some)) {
        for notice in [false,true] {
            let reference=draw(16384,page,notice);
            for limit in [4096,2048] {let other=draw(limit,page,notice);
                let max=reference.iter().zip(&other).map(|(a,b)|a.abs_diff(*b)).max().unwrap_or(0);let count=reference.iter().zip(&other).filter(|(a,b)|a!=b).count();
                // 图集坐标改变只允许少量通道有 1/255 的浮点采样舍入差异（实测最多约 0.005% 的通道），肉眼不可见。
                assert!(max<=1 && count<reference.len()/1000,"图集上限 {limit} 改变了页面 {page:?}、通知 {notice} 的像素：{max}/{count}");}
        }
    }
}
#[test]
fn compact_font_atlas_preserves_settings_pixels_at_multiple_scales(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    fn draw(limit:usize,scale:f32)->Vec<Vec<u8>>{
        let mut h=H::new();h.app.open_settings();h.ctx.set_embed_viewports(false);
        let output=std::sync::Arc::new(std::sync::Mutex::new(None));let capture=output.clone();
        egui::Context::set_immediate_viewport_renderer(move|ctx,mut viewport|{
            let id=viewport.ids.this;
            let raw=egui::RawInput{viewport_id:id,max_texture_side:Some(limit),screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(690.0,560.0))),
                viewports:std::iter::once((id,egui::ViewportInfo{native_pixels_per_point:Some(scale),..Default::default()})).collect(),..Default::default()};
            *capture.lock().unwrap()=Some(ctx.run(raw,|ctx|(viewport.viewport_ui_cb)(ctx)));
        });
        let mut raster=crate::preview_raster::Raster::new((690.0*scale)as usize,(560.0*scale)as usize);let mut result=Vec::new();
        for tab in SETTINGS_TABS {h.app.settings_tab=tab;
            for frame in 0..3 {
                let root=h.ctx.run(egui::RawInput{max_texture_side:Some(limit),viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(scale),..Default::default()})).collect(),..Default::default()},|ctx|h.app.settings_ui(ctx,false));
                let mut child=output.lock().unwrap().take().unwrap();
                raster.update_textures(&root.textures_delta);raster.update_textures(&child.textures_delta);
                if frame==2 {
                    // 常驻预览读取真实时钟，排除跨分钟变化；固定时钟已由主界面对照覆盖。
                    if tab==10 || cfg!(target_os="macos") && tab==0{child.shapes.retain(|s|!matches!(&s.shape,egui::Shape::Text(t) if t.galley.text().len()==5 && t.galley.text().as_bytes()[2]==b':'));}
                    raster.buf.fill([0.0,0.0,0.0,1.0]);raster.draw(&h.ctx.tessellate(child.shapes,child.pixels_per_point),child.pixels_per_point);
                    let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);result.push(rgba);
                }
                raster.free_textures(&root.textures_delta);raster.free_textures(&child.textures_delta);
            }
        }result
    }
    for scale in [1.0,1.5,2.0] {
        let before=draw(16384,scale);
        for limit in [4096,2048] {let after=draw(limit,scale);
        for (tab,(before,after)) in before.iter().zip(&after).enumerate(){
            // 图集坐标改变只允许少量通道有 1/255 的浮点采样舍入差异。
            let max=before.iter().zip(after).map(|(a,b)|a.abs_diff(*b)).max().unwrap_or(0);
            let count=before.iter().zip(after).filter(|(a,b)|a!=b).count();assert!(max<=1 && count<before.len()/1000,"设置页 {tab} 在图集上限 {limit}、缩放 {scale} 下出现可见差异：{max}/{count}");
        }}
    }
}

#[test]
fn mailbox_wheel_stays_in_list_with_hidden_cards_and_short_lists() {
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for mode in 0..4 {for count in [0,1,3,8] {
        let mut h=H::new();
        h.app.mail.accounts=vec![crate::mail::Account{custom:None,email:"test@qq.com".into(),provider:crate::mail::Provider::Qq,enabled:true}];
        h.app.mail.recent=(1..=count).map(|uid|crate::mail::Item{received_unix:None,account:"test@qq.com".into(),from:format!("来源 {uid}"),subject:format!("邮件 {uid}"),time:"11-04 09:41".into(),code:if mode==0{None}else{Some(format!("{uid:06}"))},html: String::new().into(), summary:String::new().into(),links:vec![],uid}).collect();
        if mode>=2 {for item in h.app.mail.recent.clone(){h.app.mail.privacy.observe(&item,true,h.app.t,chrono::Utc::now().timestamp());}}
        h.app.settings.mail_code=mode!=2;
        let cursor=H::local(if mode==3 && count>0{220.0}else{65.0},90.0);
        h.app.show_page(crate::app::MAIL_PAGE);h.hover(cursor,1.0);
        for delta in [300.0,-900.0,-300.0,900.0,300.0] {
            h.wheel(delta);h.hover(cursor,0.3);
            assert_eq!(h.app.page(),crate::app::MAIL_PAGE,"模式 {mode}、{count} 封邮件：列表范围内和滚动边界不能触发翻页");
        }
    }}
}

#[test]
fn mailbox_compact_third_row_stays_visible_below_the_toolbar(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.mail.accounts=vec![crate::mail::Account {custom:None,email:"test@qq.com".into(),provider:crate::mail::Provider::Qq,enabled:true}];
    h.app.mail.recent=(1..=6).map(|uid|crate::mail::Item {received_unix:None,account:"test@qq.com".into(),from:format!("来源 {uid}"),subject:format!("邮件 {uid}"),time:"09:41".into(),code:None,html: String::new().into(), summary:String::new().into(),links:vec![],uid}).collect();
    h.app.show_page(crate::app::MAIL_PAGE);h.hover(H::local(200.0,90.0),1.0);
    let third=h.ctx.read_response(egui::Id::new(("card",true,("mail-row",3u32,"test@qq.com")))).expect("第三封邮件应完整可见");
    assert!(third.rect.bottom()<=H::local(380.0,122.0).y+0.5);
    h.click(third.rect.center());
    assert_eq!(h.app.mail.open_uid,Some(("test@qq.com".into(),3)));
}


#[test]
#[ignore = "按需导出五套皮肤的预览图：QINGYU_SKIN_PREVIEW_DIR=目录 cargo test skin_gallery -- --ignored"]
fn skin_gallery(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let dest=std::path::PathBuf::from(std::env::var_os("QINGYU_SKIN_PREVIEW_DIR").expect("需指定预览目录"));
    std::fs::create_dir_all(&dest).unwrap();
    let (w,hgt)=((crate::app::CANVAS_W*2.0) as usize,(WIN_H*2.0) as usize);
    // 日落渐变 + 几条城市剪影，用来观察半透明皮肤的通透感。
    let backdrop=|raster:&mut crate::preview_raster::Raster|{
        for y in 0..hgt {for x in 0..w {
            let t=y as f32/hgt as f32;let s=x as f32/w as f32;
            let mut c=[0.98-0.25*t+0.05*s,0.62+0.12*t-0.1*s,0.45+0.35*t,1.0];
            if (y>hgt*7/10) && ((x/37)%3!=0) && y>hgt*7/10+((x*7919)%97) {c=[0.30,0.33,0.45,1.0];}
            let cloud=((x as f32/90.0).sin()*(y as f32/40.0).cos()).max(0.0);if y<hgt/2 {for k in 0..3{c[k]=c[k]+(1.0-c[k])*cloud*0.6;}}
            if x%160<12 && y<hgt*6/10 {c=[0.18,0.20,0.30,1.0];}
            raster.buf[y*w+x]=c;
        }}
    };
    for (skin,shape) in crate::model::Skin::ALL.into_iter().flat_map(|s|crate::model::IslandShape::ALL.map(|p|(s,p))) {
        for open in [false,true] {
            let mut h=H::new();h.app.settings.skin=skin;h.app.settings.island_shape=shape;
            h.media=crate::smtc::NowPlaying{title:"Paper Boats".into(),artist:"The Lanterns".into(),position:64.0,duration:201.0,playing:true,..Default::default()};
            let mut raster=crate::preview_raster::Raster::new(w,hgt);
            for frame in 0..120 {
                let t=frame as f64/60.0;
                let cursor=open.then_some(Pos2::new(WIN_W/2.0,14.0));
                let env=Env{t,clock:"11:13:00".into(),today:"2026-10-26".into(),date:"10月26日 周一".into(),media:h.media.clone(),cursor,backdrop:Some([0.85,0.62,0.6]),glass_blur:true,..Default::default()};
                let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(crate::app::CANVAS_W,WIN_H))),time:Some(t),
                    viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(2.0),..Default::default()})).collect(),..Default::default()};
                let mut glass=None;
                let output=h.ctx.run(raw,|ctx|{
                    egui::CentralPanel::default().frame(egui::Frame::new()).show(ctx,|ui|{let win=Rect::from_min_size(ui.max_rect().min,Vec2::new(WIN_W,WIN_H));glass=h.app.frame(ui,win,&env).glass;});
                });
                raster.update_textures(&output.textures_delta);
                if open && frame==100 {h.app.show_page(1);}
                if open && frame==104 && shape==crate::model::IslandShape::Notch {
                    backdrop(&mut raster);
                    raster.draw(&h.ctx.tessellate(output.shapes.clone(),output.pixels_per_point),output.pixels_per_point);
                    let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
                    let image=image::RgbaImage::from_raw(w as u32,hgt as u32,rgba).unwrap();
                    image::imageops::crop_imm(&image,120,0,1040,400).to_image().save(dest.join(format!("{}-switch.png",skin.key()))).unwrap();
                }
                if frame==119 {
                    backdrop(&mut raster);
                    // 模拟系统实时模糊：只有岛体轮廓内的背景被模糊（与实机的合成层一致）。
                    if let Some(shapes)=glass {
                        let pieces=crate::app::blur_pieces(&shapes,2.0);
                        let mut blurred=raster.buf.clone();
                        for _ in 0..3 {
                            let src=blurred.clone();let r=18i32;
                            for y in 0..hgt as i32 {for x in 0..w as i32 {let mut s=[0.0f32;4];let mut n=0.0;for dx in (-r..=r).step_by(3){let xx=(x+dx).clamp(0,w as i32-1);let c=src[(y as usize)*w+xx as usize];for k in 0..4{s[k]+=c[k];}n+=1.0;}blurred[(y as usize)*w+x as usize]=s.map(|v|v/n);}}
                            let src=blurred.clone();
                            for y in 0..hgt as i32 {for x in 0..w as i32 {let mut s=[0.0f32;4];let mut n=0.0;for dy in (-r..=r).step_by(3){let yy=(y+dy).clamp(0,hgt as i32-1);let c=src[(yy as usize)*w+x as usize];for k in 0..4{s[k]+=c[k];}n+=1.0;}blurred[(y as usize)*w+x as usize]=s.map(|v|v/n);}}
                        }
                        for y in 0..hgt {for x in 0..w {
                            let (px,py)=(x as f32+0.5,y as f32+0.5);
                            let inside=pieces.iter().any(|p|{px>=p.x&&px<p.x+p.w&&py>=p.y&&py<p.y+p.h&&{let cx=px.clamp(p.x+p.radius,p.x+p.w-p.radius);let cy=py.clamp(p.y+p.radius,p.y+p.h-p.radius);(px-cx).powi(2)+(py-cy).powi(2)<=p.radius*p.radius}});
                            if inside {raster.buf[y*w+x]=blurred[y*w+x];}
                        }}
                    }
                    raster.draw(&h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);
                    let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
                    let image=image::RgbaImage::from_raw(w as u32,hgt as u32,rgba).unwrap();
                    let crop=if open{image::imageops::crop_imm(&image,120,0,1040,400)}else{image::imageops::crop_imm(&image,300,0,680,110)};
                    crop.to_image().save(dest.join(format!("{}-{}-{}.png",skin.key(),shape.key(),if open{"open"}else{"closed"}))).unwrap();
                }
                raster.free_textures(&output.textures_delta);
            }
        }
    }
}


#[test]
#[ignore = "按需导出：浅色皮肤下每个页面的展开态，用于排查配色"]
fn skin_gallery_pages(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let dest=std::path::PathBuf::from(std::env::var_os("QINGYU_SKIN_PREVIEW_DIR").expect("需指定预览目录"));
    std::fs::create_dir_all(&dest).unwrap();
    let skins=std::env::var("QINGYU_SKINS").unwrap_or_else(|_|"glass_light,cream,classic".into());
    for skin in skins.split(',').map(crate::model::Skin::parse) {
        for page in 0..crate::app::PAGE_COUNT {
            let mut h=H::new();h.app.settings.skin=skin;h.app.settings.island_shape=crate::model::IslandShape::Pill;

            h.app.todos.items.push(crate::model::TodoItem{text:"整理轻屿皮肤反馈".into(),done_on:None,due:Some("2026-11-05 10:00".into()),reminded:false,..Default::default()});
            h.app.todos.items.push(crate::model::TodoItem{text:"已完成的事项".into(),done_on:Some("2026-11-04".into()),due:None,reminded:false,..Default::default()});
            h.media=crate::smtc::NowPlaying{title:"Paper Boats".into(),artist:"The Lanterns".into(),position:64.0,duration:201.0,playing:true,..Default::default()};
            let (w,hgt)=((crate::app::CANVAS_W*2.0) as usize,(WIN_H*2.0) as usize);
            let mut raster=crate::preview_raster::Raster::new(w,hgt);
            for frame in 0..140 {
                let t=frame as f64/60.0;
                if frame==40 { h.app.show_page(page); }
                let env=Env{t,clock:"22:46:00".into(),today:"2026-11-04".into(),date:"11月4日 周三".into(),media:h.media.clone(),cursor:Some(Pos2::new(WIN_W/2.0,20.0)),backdrop:Some([0.8,0.84,0.88]),glass_blur:true,..Default::default()};
                let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(crate::app::CANVAS_W,WIN_H))),time:Some(t),
                    viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(2.0),..Default::default()})).collect(),..Default::default()};
                let output=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::new()).show(ctx,|ui|{let win=Rect::from_min_size(ui.max_rect().min,Vec2::new(WIN_W,WIN_H));h.app.frame(ui,win,&env);});});
                raster.update_textures(&output.textures_delta);
                if frame==139 {
                    raster.buf.fill([0.80,0.84,0.88,1.0]);
                    raster.draw(&h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);
                    let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
                    let image=image::RgbaImage::from_raw(w as u32,hgt as u32,rgba).unwrap();
                    image::imageops::crop_imm(&image,160,0,960,500).to_image().save(dest.join(format!("page-{}-{page}.png",skin.key()))).unwrap();
                }
                raster.free_textures(&output.textures_delta);
            }
        }
    }
}


/// 新功能画面：按需导出，检查排版。
fn feature_raster()->crate::preview_raster::Raster{crate::preview_raster::Raster::new((crate::app::CANVAS_W*2.0) as usize,(WIN_H*2.0) as usize)}
fn shoot_feature(h:&mut H,raster:&mut crate::preview_raster::Raster,dest:&std::path::Path,name:&str,frames:usize,cursor:Option<Pos2>,today:&str){
    let ppp=2.0;let (w,hgt)=(crate::app::CANVAS_W,WIN_H);
    for f in 0..frames {
        h.t+=1.0/60.0;
        let env=Env{t:h.t,clock:"09:41:00".into(),today:today.into(),date:"11月4日 周三".into(),cursor,..Default::default()};
        let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(w,hgt))),time:Some(h.t),focused:true,
            events:cursor.map(|p|vec![egui::Event::PointerMoved(p)]).unwrap_or_default(),
            viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(ppp),..Default::default()})).collect(),..Default::default()};
        let app=&mut h.app;
        let output=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::new()).show(ctx,|ui|{let win=Rect::from_min_size(ui.max_rect().min,Vec2::new(WIN_W,WIN_H));app.frame(ui,win,&env);});});
        raster.update_textures(&output.textures_delta);
        let primitives=h.ctx.tessellate(output.shapes,output.pixels_per_point);
        if f+1==frames {
            raster.buf.fill([0.20,0.24,0.32,1.0]);
            raster.draw(&primitives,output.pixels_per_point);
            let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
            let image=image::RgbaImage::from_raw((w*ppp) as u32,(hgt*ppp) as u32,rgba).unwrap();
            let crop_h=(h.app.expanded_height()*ppp+24.0).max(330.0).min(hgt*ppp) as u32;
            image::imageops::crop_imm(&image,200,0,880,crop_h).to_image().save(dest.join(format!("{name}.png"))).unwrap();
        }
        raster.free_textures(&output.textures_delta);
    }
}


static SIDE_MUSIC:std::sync::atomic::AtomicBool=std::sync::atomic::AtomicBool::new(false);
/// 侧边停靠的画面：画布为 SIDE_W × SIDE_H。
fn shoot_side(h:&mut H,raster:&mut crate::preview_raster::Raster,dest:&std::path::Path,name:&str,frames:usize,cursor:Option<Pos2>){
    let ppp=2.0;let (w,hgt)=(crate::side_dock::SIDE_W,crate::side_dock::SIDE_H);
    for f in 0..frames {
        h.t+=1.0/60.0;
        let mut env=Env{t:h.t,clock:"09:41:00".into(),today:"2026-11-04".into(),date:"11月4日 周三".into(),cursor,battery:Some(76),..Default::default()};
        if SIDE_MUSIC.load(std::sync::atomic::Ordering::Relaxed) {env.media.title="晴天".into();env.media.artist="周杰伦".into();env.media.playing=true;env.media.source="cloudmusic.exe".into();env.media.duration=269.0;}
        let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(w,hgt))),time:Some(h.t),focused:true,
            events:cursor.map(|p|vec![egui::Event::PointerMoved(p)]).unwrap_or_default(),
            viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(ppp),..Default::default()})).collect(),..Default::default()};
        let app=&mut h.app;let mut out=None;
        let output=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::new()).show(ctx,|ui|{let win=Rect::from_min_size(ui.max_rect().min,Vec2::new(w,hgt));out=Some(app.frame(ui,win,&env));});});
        raster.update_textures(&output.textures_delta);
        if f+1==frames {
            raster.buf.fill([0.20,0.24,0.32,1.0]);
            raster.draw(&h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);
            let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
            let mut image=image::RgbaImage::from_raw((w*ppp) as u32,(hgt*ppp) as u32,rgba).unwrap();
            // 画出窗口区域（红）与命中区域（绿）的边框，方便检查。
            if let Some(out)=out {
                for (r,color) in [(out.region,image::Rgba([255,80,80,255])),(out.hit,image::Rgba([80,255,120,255]))] {
                    let (x0,y0,x1,y1)=((r.left()*ppp).max(0.0) as u32,(r.top()*ppp).max(0.0) as u32,((r.right()*ppp) as u32).min(image.width()-1),((r.bottom()*ppp) as u32).min(image.height()-1));
                    for x in x0..=x1 {image.put_pixel(x,y0,color);image.put_pixel(x,y1,color);}
                    for y in y0..=y1 {image.put_pixel(x0,y,color);image.put_pixel(x1,y,color);}
                }
            }
            image.save(dest.join(format!("{name}.png"))).unwrap();
        }
        raster.free_textures(&output.textures_delta);
    }
}


fn feature_raster_sized(w:f32,h:f32)->crate::preview_raster::Raster{crate::preview_raster::Raster::new((w*2.0) as usize,(h*2.0) as usize)}


#[test]
fn side_dock_auto_minimal_follows_avoidance(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let (w,hgt)=(crate::side_dock::SIDE_W,crate::side_dock::SIDE_H);
    let mut h=H::new();h.app.settings.dock=crate::model::Dock::Right;h.app.settings.browser_minimal=true;
    let away=Some(Pos2::new(w/2.0,hgt-5.0));
    for _ in 0..60 {side_frame(&mut h,away);}
    let normal=side_frame(&mut h,away).hit;
    h.browser_at_top=Some(true);
    let mut out=side_frame_with(&mut h,away);
    for _ in 0..60 {out=side_frame_with(&mut h,away);}
    assert!(out.hit.width()<normal.width() && out.hit.height()<normal.height(),"避让时收成贴边细线：{:?} → {:?}",normal,out.hit);
    h.browser_at_top=Some(false);
    for _ in 0..60 {out=side_frame_with(&mut h,away);}
    assert!((out.hit.height()-normal.height()).abs()<1.0,"离开后恢复：{:?}",out.hit);
}

fn side_frame_with(h:&mut H,cursor:Option<Pos2>)->crate::app::FrameOut{
    let at_top=h.browser_at_top;
    let (w,hgt)=(crate::side_dock::SIDE_W,crate::side_dock::SIDE_H);
    h.t+=1.0/60.0;
    let env=Env{t:h.t,clock:"09:41:00".into(),cursor,battery:Some(76),browser_at_top:at_top,..Default::default()};
    let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(w,hgt))),time:Some(h.t),focused:true,..Default::default()};
    let app=&mut h.app;let mut out=None;
    let _=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::new()).show(ctx,|ui|{out=Some(app.frame(ui,Rect::from_min_size(ui.max_rect().min,Vec2::new(w,hgt)),&env));});});
    out.unwrap()
}

fn side_frame(h:&mut H,cursor:Option<Pos2>)->crate::app::FrameOut{
    let (w,hgt)=(crate::side_dock::SIDE_W,crate::side_dock::SIDE_H);
    h.t+=1.0/60.0;
    let env=Env{t:h.t,clock:"09:41:00".into(),today:"2026-11-04".into(),date:"11月4日 周三".into(),cursor,battery:Some(76),..Default::default()};
    let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(w,hgt))),time:Some(h.t),focused:true,
        events:cursor.map(|p|vec![egui::Event::PointerMoved(p)]).unwrap_or_default(),..Default::default()};
    let app=&mut h.app;let mut out=None;
    let output=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::new()).show(ctx,|ui|{out=Some(app.frame(ui,Rect::from_min_size(ui.max_rect().min,Vec2::new(w,hgt)),&env));});});
    h.shapes=output.shapes;
    out.unwrap()
}

#[test]
fn side_dock_opens_from_the_edge_and_collapses_back(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let (w,hgt)=(crate::side_dock::SIDE_W,crate::side_dock::SIDE_H);
    for (dock,shape) in [(crate::model::Dock::Left,crate::model::IslandShape::Notch),(crate::model::Dock::Right,crate::model::IslandShape::Pill)] {
        let mut h=H::new();h.app.settings.dock=dock;h.app.settings.island_shape=shape;
        let away=Some(Pos2::new(w/2.0,hgt-5.0));
        let mut out=side_frame(&mut h,away);
        for _ in 0..60 {out=side_frame(&mut h,away);}
        assert!(!h.app.open);
        // 收起：竖向胶囊贴边、竖直居中。
        assert!(out.hit.height()>out.hit.width(),"{dock:?} 收起时是竖向：{:?}",out.hit);
        assert!((out.hit.center().y-hgt/2.0).abs()<1.0);
        let edge=if dock==crate::model::Dock::Left{0.5}else{w-0.5};
        assert!(out.hit.contains(Pos2::new(edge,hgt/2.0)),"{dock:?} 光标推到屏幕边缘也能命中：{:?}",out.hit);
        assert!(out.region.contains_rect(out.hit.shrink(0.5)));
        // 推到边缘停留 → 展开，面板从边缘滑出且完整留在画布内。
        for _ in 0..60 {out=side_frame(&mut h,Some(Pos2::new(edge,hgt/2.0)));}
        assert!(h.app.open && h.app.progress()>0.95,"{dock:?} 应展开");
        assert!((out.hit.width()-crate::side_dock::PANEL_W).abs()<20.0 && out.hit.height()>crate::side_dock::PANEL_H-1.0,"{:?}",out.hit);
        assert!(out.hit.height()>out.hit.width(),"侧边展开必须是窄高的竖向面板");
        assert!(Rect::from_min_size(Pos2::ZERO,Vec2::new(w,hgt)).contains_rect(out.region),"区域不能超出画布：{:?}",out.region);
        if dock==crate::model::Dock::Left {assert!(out.hit.left()<=0.5)} else {assert!(out.hit.right()>=w-0.5)}
        // 离开 → 收起。
        for _ in 0..90 {out=side_frame(&mut h,away);}
        assert!(!h.app.open && h.app.progress()<0.05);
        assert!(out.hit.width()<60.0);
    }
}


#[test]
fn new_mail_notification_masks_codes_in_sender_and_history(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.mail.accounts=vec![crate::mail::Account{email:"a@example.com".into(),provider:crate::mail::Provider::Qq,custom:None,enabled:true}];
    let item=crate::mail::Item{received_unix:None,account:"a@example.com".into(),from:"登录验证码 123456".into(),subject:"验证码 123456".into(),time:"09:41".into(),code:Some("123456".into()),html: String::new().into(), summary:"验证码 123456".into(),links:vec![],uid:42};
    h.app.mail_event(crate::mail::Event::Mail{item,new:true});
    assert!(!h.app.history.is_empty());
    for alert in h.app.alerts.iter().chain(h.app.history.iter()) {
        assert!(!alert.body.contains("123456"),"通知和历史记录不能留下验证码明文");
    }
}


/// 内存实测：模拟长时间听歌（不断出现新汉字）并浏览各页面，输出进程内存、字形图集和贴图占用。
/// 运行：QINGYU_ATLAS=4096 QINGYU_PPP=1.5 cargo test --lib memory_profile -- --ignored --nocapture --test-threads=1


#[test]
#[ignore = "按需导出设置页预览：QINGYU_SETTINGS_PREVIEW_DIR=目录 cargo test settings_gallery -- --ignored"]
fn settings_gallery(){
    use crate::settings_theme::SettingsTheme;
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let dest=std::path::PathBuf::from(std::env::var_os("QINGYU_SETTINGS_PREVIEW_DIR").expect("需指定预览目录"));
    std::fs::create_dir_all(&dest).unwrap();
    let width:f32=std::env::var("QINGYU_SETTINGS_WIDTH").ok().and_then(|v|v.parse().ok()).unwrap_or(690.0);
    let (w,hgt)=(width,2400.0f32);
    let scale=1.5f32;
    let selected_tabs:Option<Vec<usize>>=std::env::var("QINGYU_SETTINGS_TABS").ok().map(|value|value.split(',').filter_map(|tab|tab.trim().parse().ok()).collect());
    for (mode,dark) in [(SettingsTheme::Dark,true),(SettingsTheme::Light,false)] {
        for (_,tab,_) in crate::extras::SETTINGS_GROUPS {
            if selected_tabs.as_ref().is_some_and(|tabs|!tabs.contains(&tab)){continue;}
            let mut h=H::new();h.app.open_settings();h.ctx.set_embed_viewports(false);
            if let Ok(dock)=std::env::var("QINGYU_SETTINGS_DOCK"){
                if let Some(dock)=match dock.as_str(){"right"=>Some(crate::model::Dock::Right),"left"=>Some(crate::model::Dock::Left),"top"=>Some(crate::model::Dock::Top),_=>None}{h.app.settings.dock=dock;}
            }

            h.app.settings.settings_theme=mode;h.app.settings_tab=tab;
            let output=std::sync::Arc::new(std::sync::Mutex::new(None));let capture=output.clone();
            egui::Context::set_immediate_viewport_renderer(move|ctx,mut viewport|{
                let id=viewport.ids.this;
                let raw=egui::RawInput{viewport_id:id,screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(w,hgt))),
                    viewports:std::iter::once((id,egui::ViewportInfo{native_pixels_per_point:Some(scale),..Default::default()})).collect(),..Default::default()};
                *capture.lock().unwrap()=Some(ctx.run(raw,|ctx|(viewport.viewport_ui_cb)(ctx)));
            });
            let mut raster=crate::preview_raster::Raster::new((w*scale) as usize,(hgt*scale) as usize);
            for frame in 0..4 {
                h.ctx.data_mut(|data|data.insert_temp(egui::Id::new("settings-system-theme"),(std::time::Instant::now(),dark)));
                let root=h.ctx.run(Default::default(),|ctx|h.app.settings_ui(ctx,false));
                raster.update_textures(&root.textures_delta);
                let child=output.lock().unwrap().take().unwrap();
                raster.update_textures(&child.textures_delta);
                if frame==3 {
                    raster.buf.fill(if dark{[0.08,0.08,0.09,1.0]}else{[0.96,0.96,0.97,1.0]});
                    raster.draw(&h.ctx.tessellate(child.shapes,child.pixels_per_point),child.pixels_per_point);
                    let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
                    let image=image::RgbaImage::from_raw((w*scale) as u32,(hgt*scale) as u32,rgba).unwrap();
                    // 去掉页面下方的空白
                    let bg=image.get_pixel(((w-20.0)*scale) as u32,((hgt-5.0)*scale) as u32).0;
                    let mut bottom=image.height()-1;
                    'scan: while bottom>200 { for x in (270*scale as u32)..image.width()-10 { if image.get_pixel(x,bottom).0!=bg {break 'scan;} } bottom-=1; }
                    image::imageops::crop_imm(&image,0,0,image.width(),(bottom+20).min(image.height())).to_image()
                        .save(dest.join(format!("settings-{}-{tab:02}.png",if dark{"dark"}else{"light"}))).unwrap();
                }
                raster.free_textures(&child.textures_delta);
            }
        }
    }
}


#[test]
fn side_dock_file_drag_opens_the_shelf_and_drop_saves_it(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let (w,hgt)=(crate::side_dock::SIDE_W,crate::side_dock::SIDE_H);
    let path=crate::model::config_dir().join("侧边拖入.txt");std::fs::create_dir_all(path.parent().unwrap()).unwrap();std::fs::write(&path,"x").unwrap();
    for dock in [crate::model::Dock::Left,crate::model::Dock::Right] {
        let mut h=H::new();h.app.settings.dock=dock;
        let edge=if dock==crate::model::Dock::Left{10.0}else{w-10.0};
        let at=Pos2::new(edge,hgt/2.0);
        let mut frame=|h:&mut H,hovered:bool,dropped:bool|{
            h.t+=1.0/60.0;
            let env=Env{t:h.t,clock:"09:41:00".into(),cursor:Some(at),..Default::default()};
            let mut raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(w,hgt))),time:Some(h.t),focused:true,..Default::default()};
            if hovered {raw.hovered_files.push(egui::HoveredFile{path:Some(path.clone()),..Default::default()});}
            if dropped {raw.dropped_files.push(egui::DroppedFile{path:Some(path.clone()),..Default::default()});}
            let app=&mut h.app;let mut out=None;
            let _=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::new()).show(ctx,|ui|{out=Some(app.frame(ui,Rect::from_min_size(ui.max_rect().min,Vec2::new(w,hgt)),&env));});});
            out.unwrap()
        };
        let out=frame(&mut h,false,false);
        let zone=out.file_drop_zone.expect("置物架开启时侧边也有拖放感应区");
        assert!(zone.contains(at),"{dock:?} 感应区应覆盖贴边的胶囊：{zone:?}");
        frame(&mut h,true,false);
        assert!(h.app.drag_hot && h.app.page==crate::app::SHELF_PAGE,"{dock:?} 文件拖到胶囊上应切到置物架");
        for _ in 0..40 {frame(&mut h,true,false);}
        assert!(h.app.open);
        frame(&mut h,false,true);
        assert!(h.app.shelf.items.contains(&path),"{dock:?} 松手后应放进置物架");
        h.app.shelf.items.clear();h.app.shelf.save();
    }
}


#[test]
fn shelf_menu_last_action_fits_inside_window_and_works(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    h.app.shelf.items=vec![r"folder\menu-test.zip".into()];
    h.app.show_page(crate::app::SHELF_PAGE);
    h.hover(H::local(209.5,101.0),1.2);
    let path=h.app.shelf.items[0].clone();let point=h.drag_response(("shelf",&path)).rect.center();h.hover(point,0.2);
    for pressed in [true,false]{
        h.frame(vec![egui::Event::PointerButton{pos:point,button:egui::PointerButton::Secondary,pressed,modifiers:Default::default()}]);
    }
    h.hover(point,1.5);
    assert!(!h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()==r"folder\menu-test.zip")));
    let label=h.shapes.iter().find_map(|s|match &s.shape{
        egui::Shape::Text(t) if t.galley.text()=="从置物架移除"=>Some(Rect::from_min_size(t.pos,t.galley.size())),
        _=>None,
    }).expect("右键菜单应显示移除操作");
    assert!(h.region.contains_rect(h.app.shelf_menu_rect.unwrap() + h.ctx.style().visuals.popup_shadow.margin()));
    assert!(h.app.shelf_menu_rect.unwrap().contains_rect(label));
    h.hover(label.center(),1.0);
    assert!(h.app.open);
    h.click(label.center());
    assert!(h.app.shelf.items.is_empty());
}


#[test]
fn wheel_short_lists_turn_pages(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    for count in [0,1,5]{
        h.app.shelf.items=(0..count).map(|i|std::path::PathBuf::from(format!("scroll-test-{i}.txt"))).collect();
        for (delta,page) in [(-40.0,crate::app::CLIPBOARD_PAGE),(40.0,crate::app::MUSIC_PAGE)]{
            h.app.show_page(crate::app::SHELF_PAGE);h.hover(H::local(300.0,60.0),0.5);h.wheel(delta);
            assert_eq!(h.app.page,page,"置物架未溢出应翻页，条数 {count}");
        }
    }
    for count in [0,1,3]{
        h.app.todos.items=(0..count).map(|i|crate::model::TodoItem{text:format!("任务 {i}"),..Default::default()}).collect();
        for (delta,page) in [(-40.0,crate::app::MAIL_PAGE),(40.0,crate::app::CLIPBOARD_PAGE)]{
            h.app.show_page(crate::app::TODO_PAGE);h.hover(H::local(300.0,60.0),0.5);h.wheel(delta);
            assert_eq!(h.app.page,page,"待办未溢出应翻页，条数 {count}");
        }
    }
    for count in [0,1,crate::pages::CLIP_ROWS]{
        h.app.clipboard.lock().items.clear();
        for i in 0..count{h.app.clipboard.lock().push(&i.to_string(),"09:00");}
        for (delta,page) in [(-40.0,crate::app::TODO_PAGE),(40.0,crate::app::SHELF_PAGE)]{
            h.app.show_page(crate::app::CLIPBOARD_PAGE);h.hover(H::local(300.0,60.0),0.5);h.wheel(delta);
            assert_eq!(h.app.page,page,"剪贴板未溢出应翻页，条数 {count}");
        }
    }
}

#[test]
fn mail_reading_wheel_over_headers_never_turns_pages(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.mail.accounts=vec![crate::mail::Account{custom:None,email:"work@qq.com".into(),provider:crate::mail::Provider::Qq,enabled:true}];
    h.app.mail.recent=vec![crate::mail::Item{html:String::new().into(),received_unix:None,account:"work@qq.com".into(),from:"产品团队".into(),subject:"阅读中的邮件".into(),time:"09:41".into(),code:None,summary:"正文内容。".repeat(200).into(),links:vec![],uid:42}];
    h.app.show_page(crate::app::MAIL_PAGE);h.app.mail.open_uid=Some(("work@qq.com".into(),42));
    for (x,y) in [(210.0,18.0),(200.0,48.0),(200.0,73.0),(200.0,92.0),(270.0,214.0)]{
        for delta in [-120.0,120.0]{
            h.hover(H::local(x,y),0.5);h.wheel(delta);
            assert_eq!(h.app.page(),crate::app::MAIL_PAGE,"阅读邮件时在 ({x},{y}) 滚轮不能翻页");
            assert_eq!(h.app.mail.open_uid,Some(("work@qq.com".into(),42)));
        }
    }
    h.click(H::local(350.0,48.0));assert!(h.app.mail.open_uid.is_none());
    h.hover(H::local(210.0,18.0),0.5);h.wheel(-120.0);
    assert_eq!(h.app.page(),crate::app::WEATHER_PAGE,"返回列表后顶栏滚轮应恢复翻页");
}

#[test]
fn reader_actions_remain_bound_to_the_opened_mailbox() {
    use crate::mail_manage::{Action,Folder,Request,Completed,Snapshot};
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=managed_mail_fixture();
    let old=h.app.mail.recent[0].clone();
    h.app.mail_open_reader(&old);assert!(h.app.mail_reader_ready(&old,Folder::Inbox));
    let mut fresh=old.clone();fresh.subject="重建文件夹后的邮件".into();
    h.app.mail_event(crate::mail::Event::Status{account:old.account.clone(),status:crate::mail::Status::Ready});
    assert!(!h.app.mail_reader_ready(&old,Folder::Inbox));
    h.app.mail_managed(Completed{request:Request{account:h.app.mail.accounts[0].clone(),folder:Folder::Inbox,validity:None,known:vec![],extra:vec![],action:Action::Load},result:Ok(Snapshot{validity:8,entries:vec![(old.uid,false)],items:vec![fresh]})});
    assert!(h.app.mail.reading.is_none());assert!(!h.app.mail_reader_ready(&old,Folder::Inbox));
}

#[test]
fn local_reader_moves_and_restores_after_switching_the_main_list() {
    use crate::mail_manage::{Action,Folder};
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=managed_mail_fixture();h.app.settings.mail_sync_operations=false;
    let item=h.app.mail.recent[0].clone();
    for folder in [Folder::Inbox,Folder::Trash]{
        h.app.mail_open_reader(&item);h.app.mail_toggle_trash();
        assert!(h.app.mail_reader_ready(&item,folder));
        h.app.mail_manage_request(Some(&item.account),folder,Action::Move{uid:item.uid});
        assert!(h.app.mail_rows().iter().any(|m|m.uid==item.uid));
    }
    h.app.mail_manage_request(Some(&item.account),Folder::Trash,Action::Move{uid:item.uid});
    assert!(h.app.mail.manage.message.contains("刷新"));
}

#[test]
fn new_mail_preserves_an_explicit_reveal_in_the_trash(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=managed_mail_fixture();
    let mut item=h.app.mail.recent[0].clone();item.uid=77;item.code=Some("123456".into());
    h.app.mail.manage.trash.push(item.clone());h.app.reveal_mail_code(&item);
    let incoming=h.app.mail.recent[1].clone();h.app.mail_event(crate::mail::Event::Mail{item:incoming,new:true});
    assert!(!h.app.mail_code_hidden(&item));
    h.app.t+=16.0;assert!(h.app.mail_code_hidden(&item));
}

fn managed_mail_fixture()->H{
    use crate::mail_manage::{Folder,Snapshot};
    let mut h=H::new();h.app.settings.mail_sync_operations=true;let account=crate::mail::Account{custom:None,email:"manage@example.com".into(),provider:crate::mail::Provider::Qq,enabled:true};
    h.app.mail.accounts=vec![account.clone()];
    h.app.mail.recent=(1..=3).map(|uid|crate::mail::Item{html:String::new().into(),received_unix:None,account:account.email.clone(),from:format!("发件人 {uid}"),subject:format!("测试邮件 {uid}"),time:format!("09:4{uid}"),code:None,summary:"完整的测试邮件正文。".repeat(10).into(),links:vec![],uid}).collect();
    h.app.mail.manage.identities.insert(account.email.clone(),account.clone());
    h.app.mail.manage.data.insert((account.email,Folder::Inbox),Snapshot{validity:7,entries:vec![(1,false),(2,true),(3,true)],items:vec![]});
    h.app.show_page(crate::app::MAIL_PAGE);h.hover(H::local(220.0,80.0),1.0);h
}
#[test]
fn managed_mail_unread_filter_open_and_mark_read(){
    use crate::mail_manage::{Action,Folder,Completed};
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=managed_mail_fixture();
    h.click(H::local(118.0,42.0));assert!(h.app.mail.manage.unread_only);
    assert!(!h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()=="测试邮件 2")));
    h.cmds.clear();h.click(H::local(180.0,64.0));h.frame(vec![]);
    assert_eq!(h.app.mail.open_uid,Some(("manage@example.com".into(),1)));
    let request=h.cmds.iter().find_map(|c|match c{crate::app::Cmd::MailManage(r)if r.action==(Action::Read{uid:1,seen:true})=>Some(r.clone()),_=>None}).expect("打开未读邮件应提交服务器已读操作");
    let mut snapshot=h.app.mail.manage.data[&("manage@example.com".into(),Folder::Inbox)].clone();snapshot.entries[0].1=true;
    h.app.mail_managed(Completed{request,result:Ok(snapshot)});h.hover(H::local(270.0,150.0),0.5);
    h.click(H::local(350.0,48.0));assert!(h.app.mail.open_uid.is_none());h.hover(H::local(200.0,80.0),0.4);
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()=="没有未读邮件")));
}
#[test]
fn managed_mail_trash_icon_and_restore_keep_inbox_uid_separate(){
    use crate::mail_manage::{Action,Folder,Snapshot,Completed};
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=managed_mail_fixture();
    assert!(!h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text().starts_with("回收站"))),"回收站入口只显示图标");
    let item=h.app.mail.recent[0].clone();h.app.mail.manage.trash=vec![item];
    h.app.mail.manage.data.insert(("manage@example.com".into(),Folder::Trash),Snapshot{validity:9,entries:vec![(1,true)],items:vec![]});
    h.click_control("mail-trash");assert!(h.app.mail.manage.trash_open);
    h.frame(vec![]);h.cmds.clear();h.click_control(("mail-restore",1u32,"manage@example.com"));
    let request=h.cmds.iter().find_map(|c|match c{crate::app::Cmd::MailManage(r)if r.action==(Action::Move{uid:1})=>Some(r.clone()),_=>None}).expect("回收站恢复按钮应提交移动操作");
    assert_eq!(request.folder,Folder::Trash);assert_eq!(request.validity,Some(9));
    h.app.mail_managed(Completed{request,result:Ok(Snapshot{validity:9,entries:vec![],items:vec![]})});
    assert!(h.app.mail.manage.trash.is_empty());assert_eq!(h.app.mail.recent.len(),3,"同 UID 的收件箱邮件不能被回收站恢复操作移除");
    assert!(h.app.mail.manage.data.contains_key(&("manage@example.com".into(),Folder::Inbox)),"恢复时应保留收件箱正文缓存的有效性，切回后只补读新增邮件");
    h.frame(vec![]);h.click_control("mail-trash-back");assert!(!h.app.mail.manage.trash_open);
}
#[test]
fn managed_mail_actions_wait_for_the_server_and_batch_read_is_explicit(){
    use crate::mail_manage::{Action,Completed};
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=managed_mail_fixture();h.cmds.clear();
    let row=h.response(("mail-row",1u32,"manage@example.com")).rect;h.hover(row.center(),0.2);h.click_control(("mail-delete",1u32,"manage@example.com"));
    let request=h.cmds.iter().find_map(|c|match c{crate::app::Cmd::MailManage(r)if r.action==(Action::Move{uid:1})=>Some(r.clone()),_=>None}).expect("删除应提交移到回收站");
    assert_eq!(h.app.mail.recent.len(),3,"服务器确认前不能丢掉邮件");
    h.app.mail_managed(Completed{request,result:Err("服务器拒绝操作".into())});assert_eq!(h.app.mail.recent.len(),3);
    for snapshot in h.app.mail.manage.data.values_mut(){for (_,seen) in &mut snapshot.entries{*seen=true;}}
    h.frame(vec![]);h.cmds.clear();h.click_control("mail-read-all");
    assert!(h.cmds.iter().any(|c|matches!(c,crate::app::Cmd::MailManage(r)if r.action==Action::ReadAll)),"即使最近邮件已读，也应能处理邮箱中更早的未读邮件");
}

#[test]
fn mail_local_mode_persists_without_writing_to_the_server(){
    use crate::mail_manage::{Action,Folder,Request,Completed};
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=managed_mail_fixture();h.app.settings.mail_sync_operations=false;h.cmds.clear();
    let original=h.app.mail.recent.clone();let account=h.app.mail.accounts[0].clone();
    let snapshot=h.app.mail.manage.data[&("manage@example.com".into(),Folder::Inbox)].clone();
    h.app.mail_manage_request(Some("manage@example.com"),Folder::Inbox,Action::Read{uid:2,seen:false});
    assert_eq!(h.app.mail_seen(&original[1],Folder::Inbox),Some(false));
    assert!(h.app.mail.manage.data[&("manage@example.com".into(),Folder::Inbox)].entries[1].1,"服务器状态不能被本机已读操作改动");
    h.app.mail_manage_request(Some("manage@example.com"),Folder::Inbox,Action::Move{uid:1});
    assert_eq!(h.app.mail.recent.len(),2);assert_eq!(h.app.mail.manage.trash.len(),1);
    h.frame(vec![]);assert!(!h.cmds.iter().any(|c|matches!(c,crate::app::Cmd::MailManage(r)if r.action!=Action::Load)));
    let path=crate::model::config_dir().join("mail-local-state.json");let saved=std::fs::read_to_string(&path).unwrap();
    assert!(!saved.contains("manage@example.com")&&!saved.contains("测试邮件"),"本机状态仅保存标识和操作标记");
    h.app.mail.manage=Default::default();h.app.mail.recent=original.clone();h.app.mail_reload_local();
    h.app.mail_managed(Completed{request:Request{account,folder:Folder::Inbox,validity:Some(7),known:vec![],extra:vec![],action:Action::Load},result:Ok(snapshot)});
    assert_eq!(h.app.mail.recent.len(),2,"重新读取服务器后，本机移除的邮件不能重新出现");assert_eq!(h.app.mail.manage.trash.len(),1);
    assert_eq!(h.app.mail_seen(&original[1],Folder::Inbox),Some(false),"本机已读状态必须跨重启保留");
    h.app.mail.manage.trash_open=true;h.app.mail_manage_request(Some("manage@example.com"),Folder::Trash,Action::Move{uid:1});
    assert!(h.app.mail.manage.trash.is_empty());assert_eq!(h.app.mail.recent.len(),3);
    h.cmds.clear();h.app.settings.mail_sync_operations=true;h.app.mail_manage_mode_changed();h.frame(vec![]);
    assert!(!h.cmds.iter().any(|c|matches!(c,crate::app::Cmd::MailManage(r)if r.action!=Action::Load)),"开启同步不能补交本机操作");
    assert_eq!(h.app.mail_seen(&original[1],Folder::Inbox),Some(true),"开启同步后应显示服务器真实状态");
}
#[test]
fn mail_sync_switch_defaults_off_and_persists(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    assert!(!crate::model::Settings::default().mail_sync_operations);
    h.app.settings.mail_sync_operations=true;h.app.settings.save();assert!(crate::model::Settings::load().mail_sync_operations);
    h.app.settings.mail_sync_operations=false;h.app.settings.save();assert!(!crate::model::Settings::load().mail_sync_operations);
}


#[test]
fn weather_page_shows_the_local_snapshot_and_forecast(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    h.app.weather.snapshot=Some(crate::weather::Snapshot{place:"杭州".into(),position:None,auto:false,temp:9.0,feels:Some(5.6),humidity:Some(58),wind:Some(13.9),code:51,day:true,high:Some(15.2),low:Some(6.8),
        sunrise:Some("06:52".into()),sunset:Some("17:21".into()),sun_times:None,
        days:vec![
            crate::weather::Day{label:"今天".into(),date:"2026-11-09".into(),high:15.2,low:6.8,code:51,rain:60},
            crate::weather::Day{label:"明天".into(),date:"2026-11-10".into(),high:14.5,low:4.7,code:2,rain:0},
            crate::weather::Day{label:"周三".into(),date:"2026-11-11".into(),high:17.4,low:5.0,code:2,rain:0},
            crate::weather::Day{label:"周四".into(),date:"2026-11-12".into(),high:19.2,low:8.2,code:61,rain:25},
        ],at:chrono::Utc::now().timestamp()});
    h.app.show_page(crate::app::WEATHER_PAGE);h.hover(H::local(300.0,60.0),1.0);
    assert_eq!(h.app.page,crate::app::WEATHER_PAGE,"天气应是第九个页面");
    for phrase in ["9°","毛毛雨","杭州","今天","明天","周三","周四","60%","体感","6°","湿度","58%","日出 06:52","日落 17:21"]{
        assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text().contains(phrase))),"天气页缺少：{phrase}");
    }
    for s in &h.shapes{if let egui::Shape::Text(t)=&s.shape{assert!(s.clip_rect.contains_rect(t.visual_bounding_rect()),"天气页文字被裁切：{}",t.galley.text());}}
    if let Some(dest)=std::env::var_os("QINGYU_WEATHER_PREVIEW_DIR"){
        let dest=std::path::PathBuf::from(dest);std::fs::create_dir_all(&dest).unwrap();
        let mut raster=feature_raster();shoot_feature(&mut h,&mut raster,&dest,"weather",30,Some(H::local(300.0,60.0)),"2026-11-09");
    }
}

#[test]
fn weather_page_explains_when_data_is_missing(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    h.app.weather.error=Some("网络不可用".into());
    h.app.show_page(crate::app::WEATHER_PAGE);h.hover(H::local(300.0,60.0),1.0);
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()=="天气暂时取不到")));
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text().contains("网络不可用"))));
}

#[test]
fn weather_chip_sits_after_the_clock_on_the_resident_bar(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    h.app.weather.snapshot=Some(crate::weather::Snapshot{place:"杭州".into(),position:None,auto:false,temp:9.0,feels:Some(5.6),humidity:Some(58),wind:Some(13.9),code:51,day:true,high:Some(15.2),low:Some(6.8),sunrise:Some("06:52".into()),sunset:Some("17:21".into()),sun_times:None,days:vec![],at:chrono::Utc::now().timestamp()});
    for _ in 0..60{h.frame(vec![]);}
    let layout=h.app.resident.current();
    let clock=layout.x[crate::bar_layout::CLOCK].expect("时钟应常驻");
    let chip=layout.x[crate::bar_layout::WEATHER].expect("有快照时天气徽标应紧跟时钟占位");
    assert!(chip>clock+22.0+6.9,"天气应在时钟右侧：时钟 {clock}，天气 {chip}");
    let clock_text=h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t)if t.galley.text()=="09:41"=>Some(t.visual_bounding_rect()),_=>None}).expect("常驻栏应有时钟");
    let temp=h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t)if t.galley.text()=="9°"=>Some(t.visual_bounding_rect()),_=>None}).expect("常驻栏应在时间后显示当前温度");
    assert!(temp.left()>clock_text.right()+4.0,"温度应与时钟留出间距：时钟右边 {:?}，温度左边 {:?}",clock_text.right(),temp.left());
    assert!(temp.center().y<30.0,"温度应与图标同一行");
    h.click(temp.center());
    assert_eq!(h.app.page(),crate::app::WEATHER_PAGE,"点温度也能打开天气页");
}

#[test]
fn weather_chip_hides_without_data_or_when_the_page_is_hidden(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();
    for _ in 0..60{h.frame(vec![]);}
    assert!(h.app.resident.current().x[crate::bar_layout::WEATHER].is_none(),"没有快照时不占位");
    h.app.weather.snapshot=Some(crate::weather::Snapshot{place:"杭州".into(),position:None,auto:false,temp:9.0,feels:None,humidity:None,wind:None,code:0,day:true,high:None,low:None,sunrise:None,sunset:None,sun_times:None,days:vec![],at:chrono::Utc::now().timestamp()});
    for _ in 0..60{h.frame(vec![]);}
    assert!(h.app.resident.current().x[crate::bar_layout::WEATHER].is_some(),"有快照时占位");
    h.app.settings.visible_pages[crate::app::WEATHER_PAGE]=false;
    for _ in 0..60{h.frame(vec![]);}
    assert!(h.app.resident.current().x[crate::bar_layout::WEATHER].is_none(),"天气页被隐藏后不占位");
    assert!(!h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()=="9°")),"隐藏后不再画温度");
}

#[test]
#[ignore="按需导出常驻天气徽标预览"]
fn weather_resident_bar_preview(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let dest=std::path::PathBuf::from(std::env::var_os("QINGYU_WEATHER_PREVIEW_DIR").expect("需指定预览目录"));std::fs::create_dir_all(&dest).unwrap();
    let mut h=H::new();
    h.app.weather.snapshot=Some(crate::weather::Snapshot{place:"杭州".into(),position:None,auto:false,temp:9.0,feels:Some(5.6),humidity:Some(58),wind:Some(13.9),code:51,day:true,high:Some(15.2),low:Some(6.8),sunrise:Some("06:52".into()),sunset:Some("17:21".into()),sun_times:None,days:vec![],at:chrono::Utc::now().timestamp()});
    let mut raster=crate::preview_raster::Raster::new(1280,380);
    for frame in 0..90{
        let t=frame as f64/60.0;let env=Env{t,clock:"18:07:00".into(),battery:Some(80),charging:false,..Default::default()};
        let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(640.0,190.0))),time:Some(t),viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(2.0),..Default::default()})).collect(),..Default::default()};
        let output=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx,|ui|{h.app.frame(ui,ui.max_rect(),&env);});});
        raster.update_textures(&output.textures_delta);
        if frame==89{
            raster.buf.fill([0.09,0.11,0.18,1.0]);raster.draw(&h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
            image::save_buffer(dest.join("weather-bar.png"),&rgba,1280,380,image::ColorType::Rgba8).unwrap();
        }
    }
    // 设置页「常驻排列」预览：天气徽标应跟随时钟一起出现。换一个 H，避免复用纹理缓存。
    let mut h=H::new();
    h.app.weather.snapshot=Some(crate::weather::Snapshot{place:"杭州".into(),position:None,auto:false,temp:9.0,feels:Some(5.6),humidity:Some(58),wind:Some(13.9),code:51,day:true,high:Some(15.2),low:Some(6.8),sunrise:Some("06:52".into()),sunset:Some("17:21".into()),sun_times:None,days:vec![],at:chrono::Utc::now().timestamp()});
    let mut raster=crate::preview_raster::Raster::new(1200,300);
    for frame in 0..3{
        let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(600.0,150.0))),time:Some(frame as f64),
            viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(2.0),..Default::default()})).collect(),..Default::default()};
        let output=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().show(ctx,|ui|{h.app.resident_order_settings(ui);});});
        raster.update_textures(&output.textures_delta);
        if frame==2{
            raster.buf.fill([0.13,0.13,0.15,1.0]);raster.draw(&h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
            image::save_buffer(dest.join("weather-bar-order.png"),&rgba,1200,300,image::ColorType::Rgba8).unwrap();
        }
        raster.free_textures(&output.textures_delta);
    }
}

#[test]
fn weather_icons_stay_bright_on_light_skins(){
    use crate::skin_paint::{map_color,CREAM,GLASS_LIGHT,GLASS_DARK};
    let luma=|c:egui::Color32|0.299*c.r() as f32+0.587*c.g() as f32+0.114*c.b() as f32;
    let cloud=egui::Color32::from_rgb(238,242,250);let sun=egui::Color32::from_rgb(255,196,72);
    for palette in [CREAM,GLASS_LIGHT]{
        // 通用换色会把白云反成深色、把太阳压成暗金；天气图标改用自己的浅色版配色。
        assert!(luma(map_color(cloud,&palette))<90.0);
        assert!(luma(crate::weather_ui::light_tone(cloud,&palette))>190.0,"浅色皮肤下的云应是淡蓝灰");
        let bright=crate::weather_ui::light_tone(sun,&palette);
        assert!(bright.r()>=250&&bright.g()>=160&&bright.b()<80,"太阳保持明亮暖黄：{bright:?}");
        let half=crate::weather_ui::light_tone(sun.gamma_multiply(0.5),&palette);assert_eq!(half.a(),sun.gamma_multiply(0.5).a(),"保留透明度");
    }
    assert_eq!(crate::weather_ui::light_tone(cloud,&GLASS_DARK),map_color(cloud,&GLASS_DARK),"深色皮肤不受影响");
}

#[test]
#[ignore="按需导出常驻天气的全部图标样式"]
fn weather_icon_atlas_preview(){
    use crate::{paint::{Card,H as Align},theme::{self,W},weather::{self,Snapshot}};
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let dest=std::path::PathBuf::from(std::env::var_os("QINGYU_WEATHER_PREVIEW_DIR").expect("需指定预览目录"));std::fs::create_dir_all(&dest).unwrap();
    let cases=[("晴 · 白天",0,true,"0"),("晴 · 夜间",0,false,"0"),("晴间多云 · 白天",2,true,"1 / 2"),
        ("晴间多云 · 夜间",2,false,"1 / 2"),("阴天",3,true,"3"),("雾 / 雾凇",45,true,"45 / 48"),
        ("细雨 / 冻毛毛雨",51,true,"51 / 53 / 55 / 56 / 57"),("雨 / 冻雨",63,true,"61 / 63 / 65 / 66 / 67"),("阵雨",80,true,"80 / 81 / 82"),
        ("雪 / 阵雪",73,true,"71 / 73 / 75 / 77 / 85 / 86"),("雷雨 / 冰雹",95,true,"95 / 96 / 99"),("未知代码 · 回退云图标",200,true,"其他代码")];
    for (name,palette,ppp) in [("weather-icons-dark",None,2.0f32),("weather-icons-cream",Some(crate::skin_paint::CREAM),2.0),("weather-icons-glass-light",Some(crate::skin_paint::GLASS_LIGHT),2.0),("weather-icons-dark-1x",None,1.0),("weather-icons-dark-125",None,1.25),("weather-icons-dark-150",None,1.5)] {
        let (pw,ph)=((924.0*ppp) as usize,(700.0*ppp) as usize);
        let mut h=H::new();
        let mut layout=crate::bar_layout::Layout::new(false,false,true);layout.widths[crate::bar_layout::WEATHER]=50.0;
        let mut raster=crate::preview_raster::Raster::new(pw,ph);
        for frame in 0..4 {
            let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(924.0,700.0))),time:Some(frame as f64),
                viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(ppp),..Default::default()})).collect(),..Default::default()};
            let mut output=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx,|ui|{
                let c=Card::new(ui,Pos2::ZERO,ui.max_rect());c.fill(0.0,0.0,924.0,700.0,0.0,egui::Color32::from_rgb(14,15,19));
                c.text(20.0,38.0,"常驻天气图标",22.0,W::Bold,theme::WHITE,Align::Left);
                c.text(20.0,61.0,"11类图标 · 顶部横排与侧边竖排 · 当前实际绘制样式",11.5,W::Regular,theme::GRAY,Align::Left);
                for (index,&(label,code,day,codes)) in cases.iter().enumerate(){
                    let tile=c.shifted(20.0+(index%3)as f32*298.0,84.0+(index/3)as f32*150.0);
                    tile.fill(0.0,0.0,286.0,138.0,11.0,egui::Color32::from_rgb(25,25,29));
                    tile.text(14.0,23.0,label,12.5,W::Semibold,theme::WHITE,Align::Left);
                    crate::weather_ui::icon(&tile,32.0,63.0,33.0,weather::kind(code,day));
                    let snapshot=Snapshot{place:"杭州".into(),temp:if code==73{-6.0}else{19.0},code,day,at:chrono::Utc::now().timestamp(),..Default::default()};
                    tile.fill(65.0,46.0,148.0,30.0,15.0,egui::Color32::BLACK);
                    let bar=tile.shifted(65.0,46.0);bar.text(32.0,21.0,"10:18",15.5,W::Bold,theme::WHITE,Align::Center);
                    h.app.paint_pill_weather(&bar,105.0,&layout,&snapshot);
                    tile.fill(234.0,40.0,36.0,47.0,18.0,egui::Color32::BLACK);
                    h.app.paint_side_weather(&tile.shifted(234.0,40.0),18.0,23.0,&snapshot);
                    for (x,label) in [(32.0,"放大"),(139.0,"顶部 15px"),(252.0,"侧边")]{tile.text(x,104.0,label,9.5,W::Regular,theme::GRAY,Align::Center);}
                    tile.text(14.0,125.0,&format!("WMO  {codes}"),8.5,W::Regular,theme::DIM,Align::Left);
                }
                c.text(20.0,690.0,"无数据、关闭天气或极简模式时不显示；超过半小时未更新时，温度转灰。",10.5,W::Regular,theme::GRAY,Align::Left);
                // 和岛上一样按图层换色，天气图标登记的浅色配色才会生效。
                if let Some(palette)=palette {let layer=ui.layer_id();crate::skin_paint::recolor_layer(ctx,layer,&[(0,crate::skin_paint::shape_count(ctx,layer))],&palette);}
            });});
            raster.update_textures(&output.textures_delta);
            if frame==3 {raster.buf.fill([0.0,0.0,0.0,1.0]);raster.draw(&h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);image::save_buffer(dest.join(format!("{name}.png")),&rgba,pw as u32,ph as u32,image::ColorType::Rgba8).unwrap();}
            raster.free_textures(&output.textures_delta);
        }
    }
}

#[test]
#[ignore="按需导出天气设置页，检查排版"]
fn weather_settings_preview(){
    use crate::settings_theme::SettingsTheme;
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let dest=std::path::PathBuf::from(std::env::var_os("QINGYU_WEATHER_SETTINGS_PREVIEW").expect("需指定预览目录"));std::fs::create_dir_all(&dest).unwrap();
    for (tag,theme,dropdown) in [("dark",SettingsTheme::Dark,false),("light",SettingsTheme::Light,false),("dropdown-dark",SettingsTheme::Dark,true),("dropdown-light",SettingsTheme::Light,true)] {
        let mut h=H::new();h.app.settings.weather_city="杭州".into();h.app.weather.city_input="杭州".into();h.app.weather.manual_city=true;h.app.settings.settings_theme=theme;
        h.ctx.style_mut(|style|style.animation_time=0.0);
        let current=crate::weather::Position{lat:30.27,lon:120.15,label:"杭州".into(),region:"浙江 · 中国".into()};
        h.app.weather.selected_city=Some(current.clone());h.app.weather.active_position=Some(current.clone());
        h.app.settings.weather_position=Some(current.clone());h.app.settings.weather_saved=vec![current.clone(),crate::weather::Position{lat:31.23,lon:121.47,label:"上海".into(),region:"上海 · 中国".into()}];
        h.app.weather.snapshot=Some(crate::weather::Snapshot{place:"杭州".into(),temp:19.0,code:2,day:true,feels:Some(18.0),humidity:Some(58),wind:Some(12.0),high:Some(23.0),low:Some(15.0),
            sunrise:Some("06:52".into()),sunset:Some("17:21".into()),at:chrono::Utc::now().timestamp(),..Default::default()});
        h.app.weather.snapshot.as_mut().unwrap().position=Some(current);
        if dropdown {
            h.app.weather.city_input="佛山".into();
            h.app.weather.cities.seed("佛山",vec![
                crate::weather::Position{lat:23.02677,lon:113.13148,label:"佛山市".into(),region:"广东 · 中国".into()},
                crate::weather::Position{lat:28.73401,lon:98.80612,label:"佛山".into(),region:"迪庆藏族自治州 · 云南 · 中国".into()},
                crate::weather::Position{lat:30.04572,lon:106.41138,label:"佛山".into(),region:"重庆市 · 中国".into()},
            ]);
        }
        h.app.settings_tab=12;h.app.open_settings();h.ctx.set_embed_viewports(false);
        let children=std::sync::Arc::new(Mutex::new(Vec::new()));let capture=children.clone();
        egui::Context::set_immediate_viewport_renderer(move|ctx,mut viewport|{
            let id=viewport.ids.this;
            let raw=egui::RawInput{viewport_id:id,screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(800.0,620.0))),
                viewports:std::iter::once((id,egui::ViewportInfo{native_pixels_per_point:Some(2.0),..Default::default()})).collect(),..Default::default()};
            capture.lock().unwrap().push(ctx.run(raw,|ctx|(viewport.viewport_ui_cb)(ctx)));
        });
        let mut raster=crate::preview_raster::Raster::new(1600,1240);
        for frame in 0..4 {
            if dropdown && frame==1 {h.ctx.memory_mut(|m|m.open_popup(egui::Id::new("weather-city-options")));}
            let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(crate::app::CANVAS_W,WIN_H))),time:Some(frame as f64),..Default::default()};
            let root=h.ctx.run(raw,|ctx|h.app.settings_ui(ctx,false));
            let child_frames:Vec<_>=children.lock().unwrap().drain(..).collect();
            for child in &child_frames {raster.update_textures(&child.textures_delta);}
            raster.update_textures(&root.textures_delta);
            for child in child_frames {
                if frame==3 {raster.buf.fill([0.1,0.1,0.1,1.0]);raster.draw(&h.ctx.tessellate(child.shapes,child.pixels_per_point),child.pixels_per_point);let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);image::save_buffer(dest.join(format!("weather-settings-{tag}.png")),&rgba,1600,1240,image::ColorType::Rgba8).unwrap();}
                raster.free_textures(&child.textures_delta);
            }
            raster.free_textures(&root.textures_delta);
        }
        assert!(dest.join(format!("weather-settings-{tag}.png")).exists(),"必须导出真实设置窗口");
    }
}

#[test]
#[cfg(windows)]
fn settings_window_memory_restores_only_the_child_viewport_and_keeps_live_resizing(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.ctx.set_embed_viewports(false);h.app.open_settings();
    let original=(h.app.settings.island_size,h.app.settings.dock,h.app.settings.display_id.clone());
    let size=std::sync::Arc::new(std::sync::Mutex::new(egui::vec2(900.0,600.0)));
    let resized=size.clone();
    egui::Context::set_immediate_viewport_renderer(move|ctx,mut viewport|{
        let id=viewport.ids.this;let size=*resized.lock().unwrap();
        let raw=egui::RawInput{viewport_id:id,screen_rect:Some(Rect::from_min_size(Pos2::ZERO,size)),
            viewports:std::iter::once((id,egui::ViewportInfo{native_pixels_per_point:Some(1.0),
                inner_rect:Some(Rect::from_min_size(egui::pos2(100.0,140.0),size)),
                outer_rect:Some(Rect::from_min_size(egui::pos2(92.0,100.0),size+egui::vec2(16.0,48.0))),..Default::default()})).collect(),..Default::default()};
        let _=ctx.run(raw,|ctx|(viewport.viewport_ui_cb)(ctx));
    });
    let frame=|h:&mut H|h.ctx.run(egui::RawInput{viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(1.75),..Default::default()})).collect(),..Default::default()},|ctx|h.app.settings_ui(ctx,false));
    let _=frame(&mut h);h.app.settings_open=false;let _=frame(&mut h);
    *size.lock().unwrap()=egui::vec2(690.0,560.0);h.app.open_settings();let root=frame(&mut h);
    assert!(root.viewport_output.iter().filter(|(id,_)|**id!=egui::ViewportId::ROOT).flat_map(|(_,v)|&v.commands).any(|c|matches!(c,egui::ViewportCommand::OuterPosition(_))),"重开时应恢复子窗口");
    assert!(!root.viewport_output.get(&egui::ViewportId::ROOT).unwrap().commands.iter().any(|c|matches!(c,egui::ViewportCommand::OuterPosition(_)|egui::ViewportCommand::InnerSize(_))));
    *size.lock().unwrap()=egui::vec2(1100.0,700.0);let live=frame(&mut h);
    assert!(!live.viewport_output.values().flat_map(|v|&v.commands).any(|c|matches!(c,egui::ViewportCommand::InnerSize(_))),"正常拖动不能每帧强制恢复旧尺寸");
    assert_eq!((h.app.settings.island_size,h.app.settings.dock,h.app.settings.display_id.clone()),original);
}


#[test]
fn media_sampling_is_fast_only_while_the_music_page_is_watched(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.media=crate::smtc::NowPlaying{source:"QQMusic.exe".into(),title:"歌曲".into(),artist:"歌手".into(),duration:120.0,playing:true,..Default::default()};
    h.frame(vec![]);
    h.app.open=false;
    assert_eq!(h.app.media_poll_interval(),crate::smtc::POLL_BACKGROUND,"收起且不显示歌词时只需跟上歌名");
    h.app.settings.island_lyrics=true;h.frame(vec![]);h.app.open=false;
    assert_eq!(h.app.media_poll_interval(),crate::smtc::POLL_LYRICS,"收起显示歌词时次快");
    h.app.open=true;h.app.page=0;
    assert_eq!(h.app.media_poll_interval(),crate::smtc::POLL_FAST,"展开音乐页保持原来的 250 毫秒");
    h.app.page=1;
    assert_eq!(h.app.media_poll_interval(),crate::smtc::POLL_BACKGROUND,"展开其他页面时不必按最快频率采样");
}


fn raised(h:&H,title:&str)->bool{
    h.app.alerts.iter().chain(h.app.notice.as_ref()).any(|alert|alert.title==title)
}

#[test]
fn closed_modules_preserve_due_tasks_until_reenabled(){
    use crate::app::TODO_PAGE;
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.frame(vec![]);
    h.app.todos.items.push(crate::model::TodoItem{text:"交周报".into(),due:Some("2026-10-28 09:40".into()),..Default::default()});
    h.app.set_page_visible(TODO_PAGE,false);h.frame(vec![]);
    assert!(!raised(&h,"待办到时间了"));assert!(!h.app.todos.items[0].reminded);
    h.app.set_page_visible(TODO_PAGE,true);h.frame(vec![]);
    assert!(raised(&h,"待办到时间了"));assert!(h.app.todos.items[0].reminded);
}

#[test]
fn background_services_follow_the_module_switch(){
    use crate::app::{MAIL_PAGE,SHELF_PAGE,CLIPBOARD_PAGE};
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();
    h.app.settings.mail_enabled=true;h.app.settings.clipboard_history=true;
    assert!(h.app.mail_active() && h.app.clipboard_active());
    h.app.set_page_visible(SHELF_PAGE,false);
    assert!(h.app.clipboard_active(),"关闭置物架不应暂停独立粘贴板");
    for page in [MAIL_PAGE,CLIPBOARD_PAGE] {h.app.set_page_visible(page,false);}
    assert!(!h.app.mail_active() && !h.app.clipboard_active(),"页面开关就是模块总开关");
    h.frame(vec![]);
    assert!(!h.app.clipboard.is_enabled(),"粘贴板关闭后不再记录");
    for page in [MAIL_PAGE,CLIPBOARD_PAGE] {h.app.set_page_visible(page,true);}
    h.app.settings.mail_enabled=false;h.app.settings.clipboard_history=false;
    assert!(!h.app.mail_active() && !h.app.clipboard_active(),"模块内的开关仍然有效");
}

#[test]
fn every_module_settings_tab_maps_to_its_page(){
    let mut pages=Vec::new();
    for (page,tab) in crate::app::PAGE_SETTINGS_TABS.into_iter().enumerate(){
        assert_eq!(crate::extras::settings_tab_module(tab),Some(page));
        assert!(!crate::extras::settings_section_name(tab).is_empty());pages.push(page);
    }
    pages.sort();assert_eq!(pages,(0..crate::app::PAGE_COUNT).collect::<Vec<_>>());
}

#[path="portrait_tests.rs"]
mod portrait_tests;

#[path="notification_tests.rs"]
mod notification_tests;
