use super::*;
use crate::{agent_monitor::{AgentKind,Session,Status},model::{Dock,Skin}};

fn seed(h:&mut H){
    h.app.settings.pet=true;
    h.app.settings.notification_preview=true;
    h.app.settings.skin=Skin::GlassDark;
    h.app.agent_snapshot.sessions=(0..3).map(|index|Session{
        id:format!("companion-{index}"),agent:[AgentKind::Codex,AgentKind::Claude,AgentKind::Gemini][index],status:Status::Working,
        title:format!("演示任务 {}",index+1),turn_key:(index+1)as u64,..Default::default()
    }).collect();
}

#[test]
fn companions_fit_the_resident_body_and_open_the_real_agent_page(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|error|error.into_inner());
    for dock in Dock::ALL{for length in [0,80,520]{
        let mut h=H::new();seed(&mut h);h.app.settings.dock=dock;h.app.settings.resident_length=length;
        for _ in 0..70{if dock.is_side(){h.hit=side_frame(&mut h,None).hit;}else{h.frame(vec![]);}}
        for id in ["island-pet","island-codex-team"]{
            let response=h.ctx.read_response(egui::Id::new(("card",true,id))).expect("resident companion");
            assert!(h.hit.expand(0.6).contains_rect(response.rect),"{dock:?} {length}: {id} must fit the native island hit region");
        }
        let pet=h.ctx.read_response(egui::Id::new(("card",true,"island-pet"))).unwrap().rect;
        let team=h.ctx.read_response(egui::Id::new(("card",true,"island-codex-team"))).unwrap().rect;
        assert!(!pet.intersects(team));
        if !dock.is_side(){
            h.click(team.center());
            assert_eq!(h.app.page(),crate::app::AGENT_PAGE);assert!(h.app.open);
        }
    }}
}

#[test]
fn pet_preference_survives_restart_and_the_old_retired_setting(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|error|error.into_inner());
    let mut h=H::new();
    std::fs::write(crate::model::config_dir().join("settings.txt"),"pet=0\n").unwrap();
    assert!(crate::model::Settings::load().pet,"the new companion must be available when upgrading an old config");
    h.app.settings.pet=false;h.app.settings.save();
    assert!(!crate::model::Settings::load().pet);
    h.app.settings.pet=true;h.app.settings.save();assert!(crate::model::Settings::load().pet);
}

#[test]
fn pet_choices_and_agent_bindings_survive_reload_and_remain_independent(){
    use crate::{island_companion::{AGENTS,binding_key},model::{Settings,PetKind}};
    let _guard=ENV_LOCK.lock().unwrap_or_else(|error|error.into_inner());
    let mut h=H::new();
    let defaults=AGENTS.map(|agent|h.app.settings.agent_pet(agent));
    for i in 0..defaults.len(){for j in i+1..defaults.len(){assert_ne!(defaults[i],defaults[j]);}}
    h.app.settings.resident_pet=PetKind::Panda;
    h.app.settings.agent_pets.insert(binding_key(AgentKind::Codex).into(),PetKind::Frog);
    h.app.settings.agent_pets.insert(binding_key(AgentKind::Claude).into(),PetKind::Puppy);
    assert_eq!(h.app.settings.resident_pet_kind(),PetKind::Panda);
    h.app.settings.resident_pet_agent=Some(binding_key(AgentKind::Claude).into());
    assert_eq!(h.app.settings.resident_pet_kind(),PetKind::Puppy);
    h.app.settings.save();let saved=Settings::load();
    assert_eq!(saved.resident_pet_kind(),PetKind::Puppy);
    assert_eq!(saved.resident_pet,PetKind::Panda);
    assert_eq!(saved.agent_pet(AgentKind::Codex),PetKind::Frog);
    assert_eq!(saved.agent_pet(AgentKind::Claude),PetKind::Puppy);
    assert_eq!(saved.agent_pet(AgentKind::Gemini),PetKind::Bunny);
    h.app.settings=saved;h.app.settings.agent_pets.insert(binding_key(AgentKind::Claude).into(),PetKind::Kitten);
    assert_eq!(h.app.settings.resident_pet_kind(),PetKind::Kitten,"follow mode reflects later binding edits");
    h.app.settings.resident_pet_agent=None;h.app.settings.save();
    assert_eq!(Settings::load().resident_pet_kind(),PetKind::Panda,"manual choice is retained while following an Agent");
    std::fs::write(crate::model::config_dir().join("settings.txt"),"island_pet=1\nresident_pet=unknown\nresident_pet_agent=missing-agent\nagent_pets={}\n").unwrap();
    let invalid=Settings::load();assert_eq!(invalid.resident_pet_kind(),PetKind::Tan);
    assert_eq!(invalid.agent_pet(AgentKind::Claude),PetKind::Fox);
}

#[test]
fn clicking_a_pet_choice_replaces_the_resident_and_ends_follow_mode(){
    use crate::model::PetKind;
    let _guard=ENV_LOCK.lock().unwrap_or_else(|error|error.into_inner());
    let mut h=H::new();h.app.settings.resident_pet_agent=Some("codex".into());
    let draw_picker=|h:&mut H,events:Vec<egui::Event>|{
        h.t+=1.0/60.0;let mut ids=Vec::new();
        let _=h.ctx.run(egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(520.0,420.0))),time:Some(h.t),events,focused:true,..Default::default()},|ctx|{
            egui::CentralPanel::default().show(ctx,|ui|{
                ids=PetKind::ALL.map(|kind|ui.id().with("resident-pet-picker").with(kind.key())).to_vec();
                h.app.resident_pet_picker(ui);
            });
        });ids
    };
    let ids=draw_picker(&mut h,vec![]);
    let target=h.ctx.read_response(ids[4]).unwrap().rect.center();
    draw_picker(&mut h,vec![egui::Event::PointerMoved(target)]);
    for pressed in [true,false]{draw_picker(&mut h,vec![egui::Event::PointerButton{pos:target,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()}]);}
    assert_eq!(h.app.settings.resident_pet,PetKind::Panda);
    assert_eq!(h.app.settings.resident_pet_agent,None);
    assert_eq!(h.app.settings.agent_pet(AgentKind::Codex),PetKind::Tan,"manual selection never rewrites Agent bindings");
}

#[test]
#[ignore="Export the actual settings components for visual review"]
fn pet_selection_gallery(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|error|error.into_inner());
    let path=std::path::PathBuf::from(std::env::var_os("QINGYU_PET_CREW_REVIEW").expect("review output folder"));std::fs::create_dir_all(&path).unwrap();
    let mut h=H::new();h.ctx.set_pixels_per_point(2.0);
    let mut raster=crate::preview_raster::Raster::new(1160,1120);
    for frame in 0..4{
        let output=h.ctx.run(egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(580.0,560.0))),time:Some(frame as f64/60.0),..Default::default()},|ctx|{
            egui::CentralPanel::default().show(ctx,|ui|{
                ui.add_space(12.0);ui.heading("轻屿的小伙伴");ui.add_space(10.0);
                h.app.resident_pet_picker(ui);ui.add_space(18.0);
                ui.label("宠物搭档");ui.separator();h.app.pet_binding_settings(ui);
            });
        });
        raster.update_textures(&output.textures_delta);
        if frame==3{
            raster.buf.fill([0.08,0.12,0.14,1.0]);raster.draw(&h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);
            let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);image::RgbaImage::from_raw(1160,1120,rgba).unwrap().save(path.join("pet-selection.png")).unwrap();
        }
        raster.free_textures(&output.textures_delta);
    }
}

#[test]
fn companion_motion_has_a_static_reduced_motion_alternative(){
    let ctx=egui::Context::default();
    let render=|time,reduced|ctx.run(egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(100.0,50.0))),time:Some(time),..Default::default()},|ctx|{
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx,|ui|{
            crate::island_companion::pet(&crate::paint::Card::new(ui,Pos2::ZERO,ui.clip_rect()),crate::model::PetKind::Tan,25.0,25.0,30.0,time,reduced);
        });
    }).shapes;
    assert_eq!(format!("{:?}",render(1.0,true)),format!("{:?}",render(12.7,true)));
    assert_ne!(format!("{:?}",render(1.0,false)),format!("{:?}",render(12.7,false)));
}

#[test]
fn music_lyrics_expand_collapse_and_scale_without_red_id_errors_or_playback_commands(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|error|error.into_inner());
    let mut h=H::new();seed(&mut h);
    h.app.settings.island_lyrics=true;
    h.app.settings.resident_length=420;
    h.ctx.options_mut(|options|options.warn_on_id_clash=true);
    h.media=crate::smtc::NowPlaying{source:crate::qq_music::SOURCE.into(),title:"过渡检查".into(),artist:"本地夹具".into(),playing:true,duration:180.0,..Default::default()};
    for _ in 0..60{h.frame(vec![]);}
    let mut opening=false;let mut closing=false;let mut warnings=Vec::new();
    for frame in 0..160{
        if frame%40==0{h.ctx.set_pixels_per_point(if frame%80==0{1.4}else{1.0});}
        h.cursor=if frame<80{Some(Pos2::new(WIN_W/2.0,12.0))}else{None};
        let events=h.cursor.map(|point|vec![egui::Event::PointerMoved(point)]).unwrap_or_else(||vec![egui::Event::PointerGone]);
        h.frame(events);
        let p=h.app.progress();
        if p>0.01&&p<0.35{if frame<80{opening=true;}else{closing=true;}}
        for shape in &h.shapes{if let egui::Shape::Text(text)=&shape.shape{
            let message=text.galley.text();
            if message.contains("widget ID"){warnings.push(format!("frame={frame}, p={p:.3}: {message}"));}
        }}
        assert!(!h.cmds.iter().any(|cmd|matches!(cmd,crate::app::Cmd::QqMusicStop|crate::app::Cmd::QqMusicSetPlaying{..}|crate::app::Cmd::Media(_))),"view transitions must not change audio playback");
    }
    assert!(opening&&closing,"exercise both directions of the crossfade");
    assert!(warnings.is_empty(),"red egui errors during music transitions: {warnings:?}");
}

#[cfg(target_os="macos")]
#[test]
fn qq_music_native_lyrics_keep_one_line_and_single_companion_targets(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|error|error.into_inner());
    for length in [0,300,420,520]{for scale in [1.0,1.4,2.0]{
        let mut h=H::new();seed(&mut h);h.app.settings.island_lyrics=true;h.app.settings.resident_length=length;
        h.ctx.set_pixels_per_point(scale);h.ctx.options_mut(|options|options.warn_on_id_clash=true);
        h.media=crate::smtc::NowPlaying{source:crate::qq_music::SOURCE.into(),title:"截图中的歌曲".into(),artist:"歌手".into(),playing:true,duration:180.0,playback_type:crate::smtc::MediaKind::Music,..Default::default()};
        h.app.set_qq_music(crate::qq_music::Library{player_ready:true,..Default::default()},Some(crate::qq_music::NativeLyrics{
            title:h.media.title.clone(),lines:vec!["现在我才听说你当初找过我".into(),"这是一句超过横条可用宽度的歌词，用来验证不会换行挤出岛体或者覆盖两边的小伙伴".into()],current:Some(0),
        }));
        for _ in 0..65{h.frame(vec![]);}
        let lyric=h.shapes.iter().find_map(|shape|match &shape.shape{
            egui::Shape::Text(text)if text.galley.text()=="现在我才听说你当初找过我"=>Some(text),_=>None,
        }).expect("the screenshot's lyric fits the available row without truncation");
        assert_eq!(lyric.galley.rows.len(),1);
        assert!(!lyric.galley.elided,"use the free space beside the cover for the complete lyric at {length}/{scale}");
        let mut opening=false;let mut closing=false;
        for frame in 0..170{
            if frame==80{
                h.app.qq_lyrics.as_mut().unwrap().current=Some(1);
            }
            h.cursor=if frame<85{Some(Pos2::new(WIN_W/2.0,12.0))}else{None};
            h.frame(h.cursor.map(|point|vec![egui::Event::PointerMoved(point)]).unwrap_or_else(||vec![egui::Event::PointerGone]));
            let progress=h.app.progress();
            if progress>0.01&&progress<0.35{if frame<85{opening=true;}else{closing=true;}}
            for shape in &h.shapes{if let egui::Shape::Text(text)=&shape.shape{
                assert!(!text.galley.text().contains("widget ID"),"{length}/{scale}/{frame}: {}",text.galley.text());
                if text.galley.text().starts_with("现在我才听说")||text.galley.text().starts_with("这是一句"){
                    assert_eq!(text.galley.rows.len(),1,"a QQ native lyric must fit the single resident row at {length}/{scale}");
                    if progress<0.001{
                        let bounds=text.visual_bounding_rect();
                        assert!(h.hit.expand(0.8).contains_rect(bounds),"the lyric must stay inside the island");
                        for id in ["island-pet","island-codex-team"]{
                            let hit=h.ctx.read_response(egui::Id::new(("card",true,id))).expect("lyric retains companion action");
                            assert!(!hit.rect.intersects(bounds),"lyrics must not cover {id}");
                        }
                    }
                }
            }}
        }
        assert!(opening&&closing,"exercise both transition directions");
        assert!(!h.cmds.iter().any(|cmd|matches!(cmd,crate::app::Cmd::QqMusicStop|crate::app::Cmd::QqMusicSetPlaying{..}|crate::app::Cmd::QqMusicResume|crate::app::Cmd::Media(_))),"hovering lyrics must not change playback");
    }}
}

#[test]
fn codex_overflow_is_counted_without_expanding_the_bar_or_hiding_the_agent_action(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|error|error.into_inner());
    fn render(h:&mut H)->Vec<egui::epaint::ClippedShape>{
        h.t+=1.0/60.0;let size=h.app.canvas_size();
        h.ctx.run(egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,size)),time:Some(h.t),..Default::default()},|ctx|{
            egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx,|ui|{
                h.hit=h.app.frame(ui,Rect::from_min_size(Pos2::ZERO,size),&Env{t:h.t,clock:"09:41:00".into(),..Default::default()}).hit;
            });
        }).shapes
    }
    for dock in Dock::ALL{
        let mut h=H::new();seed(&mut h);h.app.settings.dock=dock;
        for _ in 0..65{render(&mut h);}
        let initial=h.hit;
        for (count,expected) in [(4,"+3"),(6,"+5"),(120,"99+")]{
            h.app.agent_snapshot.sessions=(0..count).map(|index|Session{
                id:format!("codex:overflow-{index}"),agent:AgentKind::Codex,status:if index==0{Status::Working}else{Status::Waiting},
                turn_key:index+1,..Default::default()
            }).collect();
            let shapes=render(&mut h);
            let label=shapes.iter().find_map(|shape|match &shape.shape{egui::Shape::Text(text)if text.galley.text()==expected=>Some(text.visual_bounding_rect()),_=>None}).expect("overflow count must be visible");
            let action=h.ctx.read_response(egui::Id::new(("card",true,"island-codex-team"))).unwrap();
            assert!(action.rect.contains_rect(label),"count must stay inside its Agent control");
            assert!((h.hit.size()-initial.size()).length()<0.5,"more sessions must not widen or lengthen the island");
        }
    }
}

#[test]
fn longer_music_bars_reveal_metadata_and_keep_both_companions_in_lyrics_and_side_modes(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|error|error.into_inner());
    for dock in Dock::ALL{for length in [0,300,420,520]{for lyrics in [false,true]{
        let mut h=H::new();seed(&mut h);h.app.settings.dock=dock;h.app.settings.resident_length=length;
        h.app.settings.island_lyrics=lyrics;
        let media=crate::smtc::NowPlaying{title:"云间漫步".into(),artist:"轻屿电台".into(),playing:true,duration:180.0,..Default::default()};
        let size=h.app.canvas_size();let mut shapes=Vec::new();
        for _ in 0..70{
            h.t+=1.0/60.0;
            let out=h.ctx.run(egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,size)),time:Some(h.t),..Default::default()},|ctx|{
                egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx,|ui|{
                    h.hit=h.app.frame(ui,Rect::from_min_size(Pos2::ZERO,size),&Env{t:h.t,clock:"09:41:00".into(),media:media.clone(),..Default::default()}).hit;
                });
            });shapes=out.shapes;
        }
        for id in ["island-pet","island-codex-team"]{
            let response=h.ctx.read_response(egui::Id::new(("card",true,id))).expect("music must retain both companions");
            assert!(h.hit.expand(0.6).contains_rect(response.rect),"{dock:?}/{length}/{lyrics}: {id}");
        }
        if !dock.is_side()&&!lyrics{
            let title=shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(text)if text.galley.text()=="云间漫步"));
            let artist=shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(text)if text.galley.text()=="轻屿电台"));
            assert_eq!(title,length>=300,"title richness follows the available length");
            assert_eq!(artist,length>=420,"long bars add the artist");
        }
        for shape in &shapes{if let egui::Shape::Text(text)=&shape.shape{
            assert!(shape.clip_rect.expand(0.8).contains_rect(text.visual_bounding_rect()),"{dock:?}/{length}/{lyrics}: {}",text.galley.text());
        }}
    }}}
}

/// Export actual egui frames with synthetic sessions for visual review. The
/// same render path and textures are used by the live macOS island.
#[test]
#[ignore="QINGYU_COMPANION_GALLERY=output/companion cargo test companion_gallery -- --ignored --test-threads=1"]
fn companion_gallery(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|error|error.into_inner());
    let folder=std::path::PathBuf::from(std::env::var_os("QINGYU_COMPANION_GALLERY").expect("gallery output directory"));
    std::fs::create_dir_all(&folder).unwrap();
    for (name,dock,skin) in [("top",Dock::Top,Skin::GlassDark),("light",Dock::Top,Skin::GlassLight),("left",Dock::Left,Skin::GlassDark),("right",Dock::Right,Skin::GlassDark),("overflow",Dock::Top,Skin::GlassDark),("music-short",Dock::Top,Skin::GlassDark),("music-medium",Dock::Top,Skin::GlassDark),("music-long",Dock::Top,Skin::GlassDark),("lyrics",Dock::Top,Skin::GlassDark)]{
        if std::env::var("QINGYU_COMPANION_VARIANT").ok().is_some_and(|variant|variant!=name){continue;}
        let mut h=H::new();seed(&mut h);h.app.settings.dock=dock;h.app.settings.skin=skin;
        if name=="overflow"{h.app.agent_snapshot.sessions.extend((3..6).map(|index|Session{id:format!("codex:demo-extra-{index}"),agent:AgentKind::Codex,status:Status::Working,turn_key:index+1,..Default::default()}));}
        h.app.settings.island_shape=crate::model::IslandShape::Pill;
        if name.starts_with("music")||name=="lyrics"{
            h.media=crate::smtc::NowPlaying{title:"云间漫步".into(),artist:"轻屿电台".into(),playing:true,duration:180.0,..Default::default()};
            h.app.settings.resident_length=if name=="music-medium"{300}else if name=="music-short"||name=="lyrics"{0}else{420};
            h.app.settings.island_lyrics=name=="lyrics";
            if name=="lyrics"{
                h.media.source=crate::qq_music::SOURCE.into();
                h.app.set_qq_music(crate::qq_music::Library{player_ready:true,..Default::default()},Some(crate::qq_music::NativeLyrics{
                    title:h.media.title.clone(),lines:vec!["现在我才听说你当初找过我".into()],current:Some(0),
                }));
            }
        }
        h.ctx.set_pixels_per_point(2.0);
        let size=if dock.is_side(){h.app.canvas_size()}else{Vec2::new(WIN_W,220.0)};
        let mut raster=crate::preview_raster::Raster::new((size.x*2.0)as usize,(size.y*2.0)as usize);
        let frame_count=if name=="top"||name=="overflow"{240}else{90};
        for frame in 0..frame_count{
            h.t=2.0+frame as f64/60.0;
            if name=="top"&&frame==110{h.app.enqueue_alert(crate::extras::Alert{
                source:crate::extras::AlertSource::LocalTest,title:"设计评审".into(),body:"资料已准备好".into(),
                page:crate::app::NOTIFICATIONS_PAGE,at:h.app.t,duration:1.1,
            },false);}
            let mut raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,size)),time:Some(h.t),focused:true,..Default::default()};
            let env=Env{t:h.t,clock:"09:41:00".into(),date:"11月15日 周日".into(),media:h.media.clone(),..Default::default()};
            // Exercise the real greeting action halfway through the clip.
            if frame==90||frame==91{if let Some(response)=h.ctx.read_response(egui::Id::new(("card",true,"island-pet"))){
                let pos=response.rect.center();raw.events=vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed:frame==90,modifiers:Default::default()}];
            }}
            let mut bounds=Rect::NOTHING;
            let output=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx,|ui|{
                let out=h.app.frame(ui,Rect::from_min_size(Pos2::ZERO,size),&env);bounds=out.hit;
            });});
            raster.update_textures(&output.textures_delta);
            if frame>=60{
                raster.buf.fill(if skin==Skin::GlassLight{[0.78,0.81,0.85,1.0]}else{[0.10,0.13,0.17,1.0]});
                raster.draw(&h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);
                let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
                let image=image::RgbaImage::from_raw((size.x*2.0)as u32,(size.y*2.0)as u32,rgba).unwrap();
                let crop=if name=="top"{Rect::from_min_size(Pos2::new(100.0,0.0),Vec2::new(440.0,110.0))}
                    else{bounds.expand(18.0).intersect(Rect::from_min_size(Pos2::ZERO,size))};
                let (x,y)=((crop.left()*2.0).floor()as u32,(crop.top()*2.0).floor()as u32);
                let (w,height)=((crop.width()*2.0).floor()as u32,(crop.height()*2.0).floor()as u32);
                image::imageops::crop_imm(&image,x,y,w,height).to_image().save(folder.join(format!("{name}-{:03}.png",frame-60))).unwrap();
            }
            raster.free_textures(&output.textures_delta);
        }
    }
}
