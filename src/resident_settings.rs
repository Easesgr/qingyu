use crate::{bar_layout::*,paint::{self,Card},theme};
use eframe::egui::{self,Color32,RichText,Rect,Pos2,Vec2};

pub(crate) fn preview_layout(order:&[usize],weather:bool)->Layout {
    let mut layout=Layout::new(true,false,weather);
    layout.reorder(order);layout
}

impl crate::app::App {
    pub(crate) fn resident_order_settings(&mut self,ui:&mut egui::Ui){
        let default=DEFAULT_ORDER.to_vec();
        let mut order=if valid_order(&self.settings.resident_order){self.settings.resident_order.clone()}else{default.clone()};
        ui.horizontal(|ui|{
            ui.label(RichText::new("拖动调整顺序").color(ui.visuals().text_color()));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{
                if ui.small_button("恢复默认").clicked(){self.settings.resident_order.clear();order=default;}
            });
        });
        ui.add_space(12.0);
        let weather_snapshot=self.weather_pill().cloned();
        let mut layout=preview_layout(&order,weather_snapshot.is_some());
        if self.settings.clock_period!=0 { layout.widen_clock(40.0); }
        if let Some(extra)=self.weather_pill_extra(ui.ctx()){layout.widen(WEATHER,extra);}
        layout.companions(self.settings.pet,self.module_on(crate::app::AGENT_PAGE));
        layout.enrich(self.settings.resident_length);layout.set_length(self.settings.resident_length);
        let (stage,_)=ui.allocate_exact_size(Vec2::new(ui.available_width(),100.0),egui::Sense::hover());
        ui.painter().rect_filled(stage,10.0,Color32::from_rgb(19,25,36));
        let scale=((stage.width()-28.0)/layout.width).min(1.0);
        let origin=Pos2::new(stage.center().x-layout.width*scale/2.0,stage.top());
        // 预览跟随当前的皮肤与形状。
        let pill=self.settings.island_shape==crate::model::IslandShape::Pill;
        let origin=if pill{origin+Vec2::new(0.0,4.0)}else{origin};
        let body=Rect::from_min_size(origin,Vec2::new(layout.width*scale,30.0*scale));
        let fill=self.preview_fill();
        if pill {ui.painter().rect_filled(body,15.0*scale,fill);}
        else {crate::shape::fill(ui.painter(),body,8.0*scale,10.0*scale,12.0*scale,fill,1.0/ui.ctx().pixels_per_point());}
        if self.settings.skin.is_glass() {
            let rim=egui::Stroke::new(1.0_f32,Color32::from_white_alpha(60));
            if pill {ui.painter().rect_stroke(body,15.0*scale,rim,egui::StrokeKind::Inside);}
            else {
                let edge=Rect::from_min_max(body.min+Vec2::new(0.5,0.0),body.max-Vec2::new(0.5,0.5));
                ui.painter().add(egui::Shape::line(crate::shape::outline(edge,8.0*scale,10.0*scale,12.0*scale-0.5),rim));
            }
        }
        let content_from=crate::skin_paint::shape_count(ui.ctx(),ui.layer_id());
        let mut card=Card::new(ui,origin,stage);card.s=scale;
        let cx=layout.width/2.0;
        let pointer=ui.ctx().pointer_interact_pos();
        let dragged=egui::DragAndDrop::payload::<usize>(ui.ctx());
        let dragged_index=dragged.as_deref().and_then(|slot|order.iter().position(|value|value==slot));
        let mut movement=None;
        let label=|slot|match slot{CLOCK=>"时间与天气",_=>"音乐"};
        for (index,&slot) in order.iter().enumerate(){
            let x=cx+layout.x[slot].unwrap();
            let width=layout.widths[slot];
            let left=x-width/2.0;
            // 天气与时钟成组，拖拽热区一并盖住，拖时钟会带着天气走。
            let (left,width)=if slot==CLOCK&&layout.x[WEATHER].is_some(){(left,width+12.0+layout.widths[WEATHER])}else{(left,width)};
            let rect=card.rect(left-5.0,1.0,width+10.0,28.0);
            let response=ui.interact(rect,ui.id().with(("resident-icon",slot)),egui::Sense::click_and_drag()).on_hover_cursor(egui::CursorIcon::Grab);
            response.dnd_set_drag_payload(slot);
            if let (Some(from),Some(from_index),Some(point))=(dragged.as_deref(),dragged_index,pointer){
                // 越过目标中心后才换位，避免宽窄不同的组件在静止指针下反复交换。
                let crossed=if from_index>index{point.x<=rect.center().x}else{point.x>=rect.center().x};
                if *from!=slot && crossed && rect.contains(point) && ui.input(|i|i.pointer.primary_down()){movement=Some((*from,index));}
            }
            let is_dragged=dragged.as_deref()==Some(&slot);
            let dx=if is_dragged{pointer.map(|p|((p.x-origin.x)/scale-(left+width/2.0)).clamp(-left,layout.width-left-width)).unwrap_or(0.0)}else{0.0};
            if is_dragged {ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);}
            else if response.hovered(){ui.painter().rect_filled(rect,6.0,Color32::from_white_alpha(18));}
            let active=match slot{MUSIC=>self.media.is_music(),_=>true};
            let c=card.shifted(dx,0.0).faded(if active||is_dragged{1.0}else{0.38}).ghost();
            match slot{
                CLOCK=>{
                    let text=self.settings.clock_text(&chrono::Local::now().format("%H:%M:%S").to_string());
                    paint::clock(&c,x-paint::clock_width(&c,&text,theme::size::PILL_CLOCK)/2.0,21.0,&text,theme::size::PILL_CLOCK);
                    if let Some(snapshot)=weather_snapshot.as_ref() {
                        self.paint_pill_weather(&c,cx+layout.x[WEATHER].unwrap(),&layout,snapshot);
                    }
                },
                MUSIC=>if layout.widths[MUSIC]>=60.0{self.draw_resident_music(&c,x,15.0,layout.widths[MUSIC]);}else{paint::playback_bars(&c,x,15.0,self.t,self.media.playing,theme::WHITE)},
                _=>{},
            }
            if is_dragged {ui.painter().line_segment([Pos2::new(rect.center().x,body.bottom()+6.0),Pos2::new(rect.center().x,body.bottom()+12.0)],egui::Stroke::new(2.0,theme::BLUE));}
            response.on_hover_text(label(slot));
        }
        if let Some(x)=layout.x[PET]{self.draw_resident_pet(&card.ghost(),cx+x,15.0,28.0);}
        if let Some(x)=layout.x[TEAM]{self.draw_resident_team(&card.ghost(),cx+x,15.0,false,layout.widths[TEAM]);}
        if let Some(palette)=self.skin_palette() {
            let end=crate::skin_paint::shape_count(ui.ctx(),ui.layer_id());
            crate::skin_paint::recolor_layer(ui.ctx(),ui.layer_id(),&[(content_from,end)],&palette);
        }
        if let Some((slot,index))=movement{
            if let Some(from)=order.iter().position(|value|*value==slot){order.remove(from);order.insert(index,slot);self.settings.resident_order=order;}
        }
    }
}
