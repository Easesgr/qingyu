//! A compact Mac settings pane: a meaningful placement preview, followed by the controls it explains.
use crate::{app::{App, Cmd}, extras::{setting_row, settings_card, settings_control_row, settings_control_row_right, settings_hint, settings_label, settings_segmented},
    mac_design, model::{Dock, IslandShape, MacosPlacement, Skin}};
use eframe::egui::{self, Align2, Color32, FontId, Rect, Stroke};

impl App {
    pub(crate) fn mac_general_settings(&mut self,ui:&mut egui::Ui,autostart:bool) {
        self.mac_placement_preview(ui);
        settings_label(ui,"位置");
        settings_card(ui,|ui|{
            settings_control_row(ui,"显示器",80.0,|ui|{
                let selected=self.displays.iter().find(|d|d.id==self.settings.display_id);
                let label=if self.settings.display_id.is_empty(){"主显示器"}else{selected.map_or("显示器已断开",|d|d.label.as_str())};
                egui::ComboBox::from_id_salt("mac-display").selected_text(label).width(ui.available_width().min(340.0)).truncate().show_ui(ui,|ui|{
                    ui.selectable_value(&mut self.settings.display_id,String::new(),"主显示器");
                    for display in &self.displays{ui.selectable_value(&mut self.settings.display_id,display.id.clone(),&display.label);}
                }).response
            });
            ui.separator();
            settings_control_row_right(ui,"放置",|ui|{
                let mut placement=if self.settings.island_position.is_some()&&!self.side(){3}else{match self.settings.dock{Dock::Top=>0,Dock::Left=>1,Dock::Right=>2}};
                if settings_segmented(ui,&mut placement,&[(0,"顶部"),(1,"左侧"),(2,"右侧"),(3,"自由")],58.0){
                    self.settings.dock=match placement{1=>Dock::Left,2=>Dock::Right,_=>Dock::Top};
                    self.settings.island_offset=0;
                    self.settings.island_position=if placement==3{Some(self.settings.island_position.unwrap_or([0.5,0.35]))}else{None};
                }
            });
            if self.settings.dock.is_side(){
                ui.separator();
                settings_control_row_right(ui,"收起显示",|ui|{
                    settings_segmented(ui,&mut self.settings.macos_side_handle,&[(false,"灵动岛"),(true,"细把手")],82.0);
                });
                ui.separator();
                settings_control_row(ui,"面板宽度",80.0,|ui|ui.add(egui::Slider::new(&mut self.settings.side_panel_width,292..=520).suffix(" 点")));
                settings_control_row(ui,"面板高度",80.0,|ui|ui.add(egui::Slider::new(&mut self.settings.side_panel_height,532..=900).suffix(" 点")));
                if ui.button("恢复默认面板大小").clicked(){self.settings.side_panel_width=292;self.settings.side_panel_height=532;}
            }
            if self.settings.dock==Dock::Top&&self.settings.island_position.is_none() {
                ui.separator();
                settings_control_row_right(ui,"顶部位置",|ui|{
                    let modes=[(MacosPlacement::Auto,"智能定位"),(MacosPlacement::BelowMenuBar,"菜单栏下方"),(MacosPlacement::Manual,"手动")];
                    settings_segmented(ui,&mut self.settings.macos_placement,&modes,76.0);
                });
                settings_hint(ui,match self.settings.macos_placement{
                    MacosPlacement::Auto=>"自动贴合刘海或保留顶部间距。",
                    MacosPlacement::BelowMenuBar=>"保留菜单栏的操作空间。",
                    MacosPlacement::Manual=>"自行选择形状与位置。",
                });
                if self.settings.macos_placement==MacosPlacement::Manual{
                    setting_row(ui,"避开菜单栏","",&mut self.settings.avoid_top_bar);
                }
            }
            ui.separator();
            if let Some(mut position)=self.settings.island_position{
                if !self.side(){settings_control_row(ui,"水平位置",80.0,|ui|ui.add(egui::Slider::new(&mut position[0],0.0..=1.0).show_value(false)));}
                settings_control_row(ui,"垂直位置",80.0,|ui|ui.add(egui::Slider::new(&mut position[1],0.0..=1.0).show_value(false)));
                if ui.button("移到屏幕中央").clicked(){position=[0.5,0.5];self.settings.dock=Dock::Top;}
                self.settings.island_position=Some(position);
            }else{
            settings_control_row(ui,if self.settings.dock.is_side(){"垂直偏移"}else{"水平偏移"},80.0,|ui|{
                ui.horizontal(|ui|{
                    let width=(ui.available_width()-if self.settings.dock.is_side(){140.0}else{76.0}).clamp(80.0,250.0);ui.spacing_mut().slider_width=width;
                    ui.add(egui::Slider::new(&mut self.settings.island_offset,-100..=100).show_value(false));
                    if self.settings.dock.is_side(){
                        if ui.add_enabled(self.settings.island_offset>-100,egui::Button::new("↑")).on_hover_text("上移").clicked(){self.settings.island_offset=(self.settings.island_offset-5).max(-100);}
                        if ui.add_enabled(self.settings.island_offset<100,egui::Button::new("↓")).on_hover_text("下移").clicked(){self.settings.island_offset=(self.settings.island_offset+5).min(100);}
                    }
                    if ui.add_enabled(self.settings.island_offset!=0,egui::Button::new("居中")).clicked(){self.settings.island_offset=0;}
                }).response
            });
            }
            settings_hint(ui,"按住岛体或顶部时间拖动，靠近左右边缘吸附。");
        });
        settings_label(ui,"外观");
        settings_card(ui,|ui|{
            settings_control_row(ui,"皮肤",80.0,|ui|{
                egui::ComboBox::from_id_salt("mac-skin").selected_text(self.settings.skin.label()).width(180.0).show_ui(ui,|ui|{
                    for skin in Skin::ALL{ui.selectable_value(&mut self.settings.skin,skin,skin.label());}
                }).response
            });
            if self.settings.dock==Dock::Top{
                if self.settings.island_position.is_none()&&self.settings.macos_placement==MacosPlacement::Manual{
                    ui.separator();settings_control_row_right(ui,"形状",|ui|{
                        settings_segmented(ui,&mut self.settings.island_shape,&[(IslandShape::Notch,"贴边"),(IslandShape::Pill,"胶囊")],72.0);
                    });
                }
            }
            if self.settings.skin.is_glass(){ui.separator();setting_row(ui,"背景模糊","",&mut self.settings.glass_blur);}
            ui.separator();
            settings_control_row(ui,"岛体大小",80.0,|ui|{
                ui.spacing_mut().slider_width=(ui.available_width()-48.0).clamp(100.0,240.0);
                ui.add(egui::Slider::new(&mut self.settings.island_size,80..=140).step_by(5.0).suffix("%"))
            });
        });
        settings_label(ui,"常驻内容");
        settings_card(ui,|ui|{
            setting_row(ui,"灵动小宠物","",&mut self.settings.pet);
            if self.settings.pet{self.resident_pet_picker(ui);}
            ui.separator();
            settings_control_row_right(ui,"收起条长度",|ui|{
                let mut automatic=self.settings.resident_length==0;
                if settings_segmented(ui,&mut automatic,&[(true,"自动"),(false,"自定")],68.0){self.settings.resident_length=if automatic{0}else{220};}
            });
            if self.settings.resident_length>0{
                settings_control_row(ui,"长度",80.0,|ui|ui.add(egui::Slider::new(&mut self.settings.resident_length,crate::island_position::LENGTH_MIN..=crate::island_position::LENGTH_MAX).suffix(" 点")));
                if ui.button("恢复自动长度").clicked(){self.settings.resident_length=0;}
            }
            settings_hint(ui,if self.settings.island_lyrics&&!self.side(){"长条可显示更多歌词；保留宠物与 Agent，歌词至少 300 点。"}else{"短条保留状态，长条增加歌名、歌手与任务摘要。"});
            ui.separator();
            if !self.settings.dock.is_side(){
                let mut minimal=self.settings.minimal;
                setting_row(ui,"极简模式","收起时只保留细线",&mut minimal);if minimal!=self.settings.minimal{self.set_minimal(minimal);}
                ui.separator();
            }
            settings_control_row_right(ui,"时间格式",|ui|{settings_segmented(ui,&mut self.settings.clock_12h,&[(false,"24 小时"),(true,"12 小时")],80.0);});

        });
        settings_label(ui,"启动与显示");
        settings_card(ui,|ui|{
            let mut enabled=autostart;
            setting_row(ui,"登录时启动","",&mut enabled);if enabled!=autostart{self.pending.push(Cmd::ToggleAutostart);}
            ui.separator();
            setting_row(ui,"全屏时隐藏","",&mut self.settings.hide_fullscreen);
        });
    }

    fn mac_placement_preview(&self,ui:&mut egui::Ui){
        let p=mac_design::palette(ui.ctx());
        let (rect,response)=ui.allocate_exact_size(egui::vec2(ui.available_width(),154.0),egui::Sense::hover());
        response.widget_info(||egui::WidgetInfo::labeled(egui::WidgetType::Image,true,"所选显示器上的岛体位置预览"));
        let screen=Rect::from_min_max(rect.min+egui::vec2(2.0,2.0),rect.max-egui::vec2(2.0,28.0));
        egui::Image::new(egui::include_image!("../assets/qingyu/preview-wallpaper.png")).corner_radius(9).paint_at(ui,screen);
        ui.painter().rect_stroke(screen,9.0,Stroke::new(0.7,p.separator),egui::StrokeKind::Inside);
        let menu=Rect::from_min_max(screen.min,egui::pos2(screen.right(),screen.top()+16.0));
        ui.painter().rect_filled(menu,egui::CornerRadius{nw:9,ne:9,sw:0,se:0},p.group);
        for i in 0..3{let left=screen.left()+12.0+i as f32*16.0;ui.painter().rect_filled(Rect::from_min_size(egui::pos2(left,screen.top()+7.0),egui::vec2(10.0,2.0)),1.0,p.secondary.gamma_multiply(0.35));}
        let display=crate::display_layout::selected(&self.displays,&self.settings.display_id);
        let width=if self.settings.resident_length==0{108.0}else{(self.settings.resident_length as f32*0.32).clamp(64.0,166.0)};let offset=self.settings.island_offset as f32/100.0;
        let center=screen.center().x+offset*(screen.width()/2.0-width/2.0-8.0);
        let camera=display.and_then(crate::macos::notch_size);
        if let Some((camera_width,_))=camera {
            let ratio=display.map_or(0.2,|d|screen.width()/(d.bounds.right-d.bounds.left).max(1) as f32);
            let notch=Rect::from_center_size(egui::pos2(screen.center().x,screen.top()+7.0),egui::vec2((camera_width*ratio).clamp(36.0,96.0),14.0));
            ui.painter().rect_filled(notch,egui::CornerRadius{nw:0,ne:0,sw:4,se:4},Color32::BLACK);
        }
        let attached=self.settings.island_position.is_none()&&self.settings.dock==Dock::Top && self.settings.macos_placement==MacosPlacement::Auto && camera.is_some() && self.settings.island_offset==0;
        let top=screen.top()+if attached{14.0}else{23.0};
        let island=if let Some(position)=self.settings.island_position{
            let room=Rect::from_min_max(egui::pos2(screen.left(),screen.top()+19.0),screen.max);
            let dimensions=if self.side(){egui::vec2(32.0,if self.settings.resident_length==0{70.0}else{(self.settings.resident_length as f32*0.2).clamp(36.0,94.0)})}else{egui::vec2(width,24.0)};
            // Preview uses the same normalized anchor and containment rule.
            crate::island_position::body(room,crate::island_position::anchor(position,room),self.dock(),dimensions)
        }else{match self.settings.dock{
            Dock::Top=>Rect::from_center_size(egui::pos2(center,top+12.0),egui::vec2(width,24.0)),
            Dock::Left=>Rect::from_min_size(egui::pos2(screen.left(),screen.center().y+offset*15.0-43.0),egui::vec2(48.0,86.0)),
            Dock::Right=>Rect::from_min_size(egui::pos2(screen.right()-48.0,screen.center().y+offset*15.0-43.0),egui::vec2(48.0,86.0)),
        }};
        if self.settings.dock.is_side(){
            crate::shape::shadow_docked(ui.painter(),island,self.settings.dock,4.0,4.0,6.0,0.35);
            crate::shape::fill_docked(ui.painter(),island,self.settings.dock,4.0,4.0,6.0,Color32::from_rgb(23,57,63),1.0/ui.ctx().pixels_per_point());
        }else{
            ui.painter().rect_filled(island.translate(egui::vec2(0.0,2.0)).expand(1.0),12.0,Color32::from_black_alpha(18));
            ui.painter().rect_filled(island,if attached{egui::CornerRadius{nw:0,ne:0,sw:12,se:12}}else{egui::CornerRadius::same(12)},Color32::from_rgb(23,57,63));
        }
        if self.settings.dock==Dock::Top {
            let time=chrono::Local::now().format(if self.settings.clock_12h{"%I:%M"}else{"%H:%M"}).to_string();
            ui.painter().text(island.left_center()+egui::vec2(13.0,0.0),Align2::LEFT_CENTER,time,FontId::proportional(10.0),Color32::WHITE);

            if let Some(icon)=crate::theme::glyph(ui.ctx(),"music") {
                let rect=Rect::from_center_size(island.right_center()-egui::vec2(20.0,0.0),egui::vec2(12.0,12.0));
                ui.painter().image(icon.id(),rect,Rect::from_min_max(egui::Pos2::ZERO,egui::pos2(1.0,1.0)),Color32::WHITE);
            }
        }
        let label=display.map_or("正在识别显示器",|d|if d.primary{"主显示器"}else{"外接显示器"});
        ui.painter().text(egui::pos2(rect.left()+2.0,rect.bottom()-8.0),Align2::LEFT_CENTER,format!("{label}{}",if self.settings.dock.is_side(){" · 连续贴边"}else if attached{" · 贴合刘海"}else if self.settings.island_position.is_some(){" · 自由放置"}else{" · 悬浮停靠"}),FontId::proportional(11.0),p.secondary);
    }
}
