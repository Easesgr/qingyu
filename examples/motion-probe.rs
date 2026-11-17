//! Actual debug/Metal frame timing with the production AppKit input and blur
//! shell. Runs a temporary island for eight seconds; no accounts or services.
#[cfg(target_os="macos")]
#[path="../src/graphics.rs"]
mod graphics;

#[cfg(target_os="macos")]
fn main()->eframe::Result<()>{
    use eframe::egui::{self,Pos2,Rect};
    use qingyu::{app::{App,Env,Options},model::{Dock,Settings,Skin},macos};
    use std::{io::Write,time::{Duration,Instant}};
    struct Probe{app:App,window:macos::Window,started:Instant,last:Option<Instant>,samples:Vec<String>,destination:std::path::PathBuf,timer:bool,placed:bool}
    impl eframe::App for Probe{
        fn clear_color(&self,_:&egui::Visuals)->[f32;4]{[0.0;4]}
        fn update(&mut self,ctx:&egui::Context,_:&mut eframe::Frame){
            let start=Instant::now();let t=self.started.elapsed().as_secs_f64();let delta=self.last.replace(start).map_or(0.0,|last|start.duration_since(last).as_secs_f64());
            if !self.placed{
                if let Some(display)=qingyu::display_layout::connected().first(){self.window.place(display,&self.app.settings,1.0,0.0,self.app.placement_position(),self.app.resident_size(),(0,0));self.app.set_side_panel_size(self.window.side_panel_size());self.placed=true;}
            }
            eframe::set_root_viewport_scale(self.window.scale()/ctx.zoom_factor());
            let size=self.app.canvas_size();
            let hover=(1.0..2.7).contains(&t)||(4.0..5.7).contains(&t);
            let env=Env{t,cursor:hover.then_some(egui::pos2(0.5,size.y/2.0)),clock:"09:41:00".into(),today:"2026-11-15".into(),glass_blur:self.window.blur_active(),..Default::default()};
            let mut output=None;
            egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx,|ui|{output=Some(self.app.frame(ui,Rect::from_min_size(Pos2::ZERO,size),&env));});
            let output=output.unwrap();
            self.window.update(output.hit,&output.hit_shapes,output.notice_hit,&output.notice_shapes,output.file_drop_zone,output.glass.as_deref(),false,false);
            let progress=self.app.progress();
            self.samples.push(format!("{t:.6},{:.6},{progress:.6},{:.6},{:.6},{}\n",delta*1000.0,output.hit.width(),output.hit.height(),start.elapsed().as_micros()));
            if t>=8.0{
                let mut file=std::fs::File::create(&self.destination).unwrap();file.write_all(b"time_s,interval_ms,progress,hit_width,hit_height,update_us\n").unwrap();for sample in &self.samples{file.write_all(sample.as_bytes()).unwrap();}
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);return;
            }
            let interval=if self.timer&&output.repaint_after.is_zero(){Duration::from_secs_f64(1.0/60.0)}else{output.repaint_after};
            qingyu::frame_pacing::schedule(ctx,interval);
        }
    }
    let args:Vec<_>=std::env::args().collect();let destination=std::path::PathBuf::from(args.get(1).expect("CSV output path"));
    std::fs::create_dir_all(destination.parent().unwrap()).unwrap();let data=std::env::temp_dir().join(format!("qingyu-motion-probe-{}",std::process::id()));std::fs::create_dir_all(&data).unwrap();std::env::set_var("QINGYU_DATA_DIR",&data);
    let mut settings=Settings{dock:Dock::Left,skin:Skin::GlassDark,macos_side_handle:false,clipboard_history:false,mail_enabled:false,notifications:false,..Default::default()};settings.visible_pages[qingyu::app::WEATHER_PAGE]=false;settings.save();
    let viewport=egui::ViewportBuilder::default().with_title("轻屿动画检查").with_inner_size([500.0,600.0]).with_decorations(false).with_transparent(true).with_active(false).with_resizable(false).with_visible(false);
    let options=eframe::NativeOptions{viewport,vsync:true,..Default::default()};let timer=args.iter().any(|s|s=="--timer");
    let result=graphics::run(options,|options|eframe::run_native("轻屿动画检查",options,Box::new(|cc|Ok(Box::new(Probe{app:App::new(&cc.egui_ctx,Options{fetch_lyrics:false,monitor_agents:false}),window:macos::Window::new(cc).expect("AppKit shell"),started:Instant::now(),last:None,samples:Vec::new(),destination:destination.clone(),timer,placed:false})))));
    let _=std::fs::remove_dir_all(data);result
}

#[cfg(not(target_os="macos"))]
fn main(){eprintln!("This native frame probe requires macOS.");}
