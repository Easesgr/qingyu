use super::*;
use crate::{app::MAC_STATS_PAGE,mac_stats::{Gpu,Network,NetworkStatus,Snapshot},model::{Dock,Skin}};
use std::time::{Duration,SystemTime};

fn seed(h:&mut H){
    let end=SystemTime::now();
    for i in 0..31{
        let wave=(i as f64*0.65).sin()*8.0;
        h.app.mac_snapshot=Snapshot{supported:true,paused:false,sampled_at:Some(end-Duration::from_secs((30-i)*2)),
            cpu_usage:Some(24.0+wave),cpu_name:"Apple M1 Pro".into(),logical_cores:10,
            memory_total:32*1024u64.pow(3),memory_used:12*1024u64.pow(3),memory_compressed:1024u64.pow(3),memory_valid:true,swap_valid:true,
            disk_total:1024u64.pow(4),disk_available:640*1024u64.pow(3),disk_valid:true,
            disk_read_bytes_per_sec:Some(2048.0),disk_write_bytes_per_sec:Some(4096.0),
            gpus:vec![Gpu{name:"Apple M1 Pro".into(),usage:Some(10.0+wave)}],
            network:Some(Network{interface:"en0".into(),received_bytes_per_sec:Some(5.0*1024f64.powi(2)+wave*65536.0),sent_bytes_per_sec:Some(80.0*1024.0),..Default::default()}),network_status:NetworkStatus::Online,..Default::default()};
        h.app.stats_history.observe(&h.app.mac_snapshot);
    }
}

fn stats_frame(h:&mut H,events:Vec<egui::Event>)->egui::FullOutput{
    h.t+=1.0/60.0;
    let size=h.app.canvas_size();let rect=Rect::from_min_size(Pos2::ZERO,size);
    let env=Env{t:h.t,clock:"09:41:00".into(),today:"2026-11-15".into(),cursor:h.cursor,..Default::default()};
    let output=h.ctx.run(egui::RawInput{screen_rect:Some(rect),time:Some(h.t),events,focused:true,..Default::default()},|ctx|{
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx,|ui|{h.app.frame(ui,rect,&env);});
    });
    h.shapes=output.shapes.clone();output
}

#[test]
fn mac_stats_panels_show_all_five_metrics_and_accessible_refresh(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let preview=std::env::var_os("QINGYU_MAC_STATS_PREVIEW_DIR").map(std::path::PathBuf::from);
    if let Some(path)=&preview{std::fs::create_dir_all(path).unwrap();}
    for (dock,width,height,skin) in [(Dock::Top,292,532,Skin::Classic),(Dock::Left,292,532,Skin::Classic),(Dock::Right,292,532,Skin::Classic),(Dock::Right,520,760,Skin::Cream),(Dock::Right,292,532,Skin::GlassDark)]{
        let mut h=H::new();h.app.settings.dock=dock;h.app.settings.skin=skin;h.app.settings.side_panel_width=width;h.app.settings.side_panel_height=height;
        h.app.show_page(MAC_STATS_PAGE);seed(&mut h);h.ctx.set_pixels_per_point(2.0);
        let size=h.app.canvas_size();
        let mut raster=preview.as_ref().map(|_|crate::preview_raster::Raster::new((size.x*2.0)as usize,(size.y*2.0)as usize));
        for frame in 0..65{
            let output=stats_frame(&mut h,vec![]);
            if let Some(raster)=&mut raster{
                raster.update_textures(&output.textures_delta);
                if frame==64{
                    raster.buf.fill([0.13,0.18,0.20,1.0]);raster.draw(&h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);
                    let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
                    image::save_buffer(preview.as_ref().unwrap().join(format!("mac-stats-{}-{width}-{skin:?}.png",dock.key())),&rgba,(size.x*2.0)as u32,(size.y*2.0)as u32,image::ColorType::Rgba8).unwrap();
                }
                raster.free_textures(&output.textures_delta);
            }
        }
        for title in ["CPU","内存",if dock==Dock::Top{"磁盘"}else{"启动磁盘"},"GPU","网络"]{
            assert!(h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(t)if t.galley.text()==title)),"{dock:?} 缺少 {title}");
        }
        let refresh=h.ctx.read_response(egui::Id::new(("card",true,"mac-stats-refresh"))).unwrap();
        assert!(refresh.rect.width()>=28.0&&refresh.rect.height()>=28.0);
        let panel=h.app.island_rect(Rect::from_min_size(Pos2::ZERO,size),h.app.expanded_width(),h.app.expanded_height());
        assert!(panel.expand(1.0).contains_rect(refresh.rect));
        for shape in &h.shapes{if let egui::Shape::Text(text)=&shape.shape{
            if text.galley.text().trim().is_empty(){continue;}
            let origin=h.app.content_origin(Rect::from_min_size(Pos2::ZERO,size));
            let navigation=Rect::from_min_max(origin+egui::vec2(62.0,48.0),origin+egui::vec2(h.app.expanded_width()-12.0,102.0));
            if dock.is_side()&&navigation.expand(0.75).contains_rect(shape.clip_rect){continue;}
            assert!(shape.clip_rect.expand(0.75).contains_rect(text.visual_bounding_rect()),"{dock:?} {skin:?} 文字裁切：{}",text.galley.text());
        }}
        let settings=if dock==Dock::Top{"page-settings"}else{"portrait-settings"};
        let point=h.ctx.read_response(egui::Id::new(("card",true,settings))).unwrap().rect.center();h.cursor=Some(point);
        stats_frame(&mut h,vec![egui::Event::PointerMoved(point)]);
        for pressed in [true,false]{stats_frame(&mut h,vec![egui::Event::PointerButton{pos:point,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()}]);}
        assert!(h.app.settings_open);assert_eq!(h.app.settings_tab,15);
    }
}

#[test]
fn mac_stats_unavailable_and_offline_values_do_not_look_like_idle_hardware(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.app.show_page(MAC_STATS_PAGE);h.app.mac_snapshot=Snapshot{supported:true,paused:false,network_status:NetworkStatus::Offline,..Default::default()};
    for _ in 0..65{stats_frame(&mut h,vec![]);}
    let texts:Vec<_>=h.shapes.iter().filter_map(|shape|match &shape.shape{egui::Shape::Text(t)=>Some(t.galley.text()),_=>None}).collect();
    assert!(texts.contains(&"不可用")&&texts.contains(&"未连接")&&texts.contains(&"—"));
    assert!(!texts.iter().any(|text|text.contains("0%")||text.contains("0 B/s")));
    h.app.set_page_visible(MAC_STATS_PAGE,false);h.app.show_page(MAC_STATS_PAGE);
    assert_ne!(h.app.page(),MAC_STATS_PAGE);assert!(!h.app.settings_open);assert_eq!(h.app.settings_tab,0);
    assert_eq!(crate::extras::settings_tab_module(15),Some(MAC_STATS_PAGE));
}
