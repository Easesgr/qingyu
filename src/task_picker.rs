//! 岛内右侧的日期、小时和分钟滚轮。
use chrono::{Datelike,Duration,Local,NaiveDate,NaiveDateTime,Timelike};
use eframe::egui::{self,Color32};
use crate::paint::{capsule,Card,H};
use crate::theme::{self,W};

pub(crate) enum PickerAction { Save, Skip, Cancel }
pub(crate) struct ReminderPicker {
    pub open:bool,
    pub focus:bool,
    /// 每周重复的待办：只看时间，允许选今天已过去的时刻；左侧说明文字。
    pub repeat_note:Option<String>,
    origin:NaiveDate,
    positions:[i64;3],
    wheel:[f32;3],
    drag:[f32;3],
    wheel_at:[f64;3],
    error:String,
}
impl ReminderPicker {
    pub fn new(due:Option<&str>)->Self {
        let selected=due.and_then(|text|NaiveDateTime::parse_from_str(text,"%Y-%m-%d %H:%M").ok())
            .unwrap_or_else(||(Local::now()+Duration::minutes(15)).naive_local());
        Self{open:false,focus:false,repeat_note:None,origin:selected.date(),positions:[0,selected.hour() as i64,selected.minute() as i64],wheel:[0.0;3],drag:[0.0;3],wheel_at:[0.0;3],error:String::new()}
    }
    pub fn show(&mut self){self.open=true;self.focus=true;}
    fn selected(&self)->NaiveDateTime {
        (self.origin+Duration::days(self.positions[0])).and_hms_opt(self.positions[1].rem_euclid(24) as u32,self.positions[2].rem_euclid(60) as u32,0).unwrap()
    }
    pub fn due(&self)->String{self.selected().format("%Y-%m-%d %H:%M").to_string()}
    fn adjust(&mut self,column:usize,steps:i64){
        if steps==0{return;}
        let next=self.positions[column].saturating_add(steps);
        if column==0 {
            let Some(date)=Duration::try_days(next).and_then(|delta|self.origin.checked_add_signed(delta)) else{return};
            self.positions[0]=(date.max(Local::now().date_naive())-self.origin).num_days();
        }else{self.positions[column]=next;}
        self.error.clear();
    }
    fn label(&self,column:usize,position:i64)->String {
        if column!=0{return format!("{:02}",position.rem_euclid(if column==1{24}else{60}));}
        let Some(date)=Duration::try_days(position).and_then(|delta|self.origin.checked_add_signed(delta)) else{return String::new()};
        let today=Local::now().date_naive();
        if date<today{return String::new();}
        match (date-today).num_days(){0=>"今天".into(),1=>"明天".into(),2=>"后天".into(),_=>date.format(if date.year()==today.year(){"%m/%d"}else{"%y/%m/%d"}).to_string()}
    }
    pub fn draw(&mut self,c:&Card,title:&str,has_reminder:bool)->Option<PickerAction> {
        let ctx=c.ui.ctx();
        c.fill(196.0,31.0,181.0,99.0,11.0,Color32::from_rgb(23,23,25));
        c.text(204.0,45.0,&c.fit(title,9.5,W::Regular,165.0),9.5,W::Regular,theme::GRAY,H::Left);
        c.fill(202.0,65.0,169.0,24.0,6.0,Color32::from_rgb(42,42,46));
        c.seg((269.0,69.0),(269.0,85.0),0.5,Color32::from_gray(70));
        for y in [74.5,80.0]{c.circle(317.0,y,0.9,theme::WHITE);}
        for (column,(x,width)) in [(204.0,62.0),(271.0,38.0),(325.0,42.0)].into_iter().enumerate(){
            let response=c.hit_drag(x,51.0,width,52.0,("reminder-wheel",column));
            if response.hovered(){
                let (delta,now)=ctx.input(|i|{
                    let delta=i.events.iter().filter_map(|event|match event{
                        egui::Event::MouseWheel{unit,delta,modifiers} if !modifiers.ctrl && !modifiers.command=>Some(-delta.y*if *unit==egui::MouseWheelUnit::Point{1.0}else{40.0}),_=>None,
                    }).sum::<f32>();(delta,i.time)
                });
                if delta!=0.0 {
                    if now-self.wheel_at[column]>0.25 || self.wheel[column]*delta<0.0{self.wheel[column]=0.0;}
                    self.wheel_at[column]=now;
                    self.wheel[column]+=delta;
                    let steps=(self.wheel[column]/40.0).trunc() as i64;
                    self.wheel[column]-=steps as f32*40.0;
                    self.adjust(column,steps);
                    ctx.input_mut(|i|{i.raw_scroll_delta=egui::Vec2::ZERO;i.smooth_scroll_delta=egui::Vec2::ZERO;});
                }
            }
            if response.drag_started(){self.drag[column]=0.0;}
            if response.dragged(){
                self.drag[column]-=response.drag_delta().y/c.s;
                let steps=(self.drag[column]/18.0).trunc() as i64;
                self.drag[column]-=steps as f32*18.0;self.adjust(column,steps);
            }
            if response.clicked(){if let Some(pos)=response.interact_pointer_pos(){let y=c.to_local(pos).y;if y<68.0{self.adjust(column,-1);}else if y>88.0{self.adjust(column,1);}}}
            response.on_hover_text(format!("{} · 滚轮、上下拖动或点选调整",["日期","小时","分钟"][column]));
            let position=ctx.animate_value_with_time(egui::Id::new(("reminder-wheel-position",column)),self.positions[column] as f32,if self.focus{0.0}else{0.13});
            let clipped=c.clipped(x,51.0,width,52.0);
            for value in position.floor() as i64-1..=position.floor() as i64+2 {
                let distance=value as f32-position;
                let strength=(1.0-distance.abs()*0.55).clamp(0.0,1.0);
                let selected_size=if column==0{14.0}else{18.0};
                let size=9.5+(selected_size-9.5)*strength;
                let color=Color32::from_gray((85.0+170.0*strength) as u8);
                clipped.centered_text(x+width/2.0,77.0+distance*18.0,&self.label(column,value),size,if strength>0.7{W::Semibold}else{W::Regular},color,H::Center);
            }
        }
        let note=self.repeat_note.as_deref();
        let (label,color)=if !self.error.is_empty(){(self.error.as_str(),theme::RED)}else if let Some(note)=note.filter(|n|!n.is_empty()){(note,theme::BLUE)}else{("已完成",theme::GRAY)};
        let w=c.text(42.0,79.0,label,9.0,W::Regular,color,H::Left);
        if self.error.is_empty() && note==Some(""){c.text(42.0+w+4.0,79.0,"· 下方可设每周重复",9.0,W::Regular,theme::DIM,H::Left);}
        let (skip,b)=capsule(c,41.0,105.0,100.0,22.0,"task-reminder-skip",theme::BTN,theme::BTN_HOVER);
        b.centered_text(91.0,116.0,if has_reminder{"取消提醒"}else{"暂不提醒"},10.0,W::Regular,theme::GRAY,H::Center);
        if skip.clicked(){return Some(PickerAction::Skip);}
        if c.hit(248.0,108.0,43.0,22.0,"task-reminder-cancel").clicked(){return Some(PickerAction::Cancel);}
        c.centered_text(269.5,119.0,"取消",10.0,W::Regular,theme::GRAY,H::Center);
        let (save,b)=capsule(c,307.0,108.0,63.0,20.0,"task-reminder-save",Color32::from_rgb(10,132,255),Color32::from_rgb(35,150,255));
        b.centered_text(338.5,118.0,"完成",10.0,W::Semibold,theme::WHITE,H::Center);
        if save.clicked(){
            if self.repeat_note.as_deref().is_none_or(str::is_empty) && self.selected()<=Local::now().naive_local(){self.error="请选择晚于当前的时间".into();}
            else{return Some(PickerAction::Save);}
        }
        None
    }

    pub fn draw_portrait(&mut self,c:&Card,title:&str,has_reminder:bool)->Option<PickerAction>{
        let ctx=c.ui.ctx();
        c.text(20.0,137.0,"提醒时间",17.0,W::Semibold,theme::WHITE,H::Left);
        c.text(20.0,164.0,&c.fit(title,12.0,W::Regular,252.0),12.0,W::Regular,theme::GRAY,H::Left);
        for (column,(x,width)) in [(20.0,108.0),(140.0,60.0),(212.0,60.0)].into_iter().enumerate(){
            c.centered_text(x+width/2.0,201.0,["日期","小时","分钟"][column],11.0,W::Regular,theme::GRAY,H::Center);
            c.fill(x,257.0,width,36.0,8.0,Color32::from_rgb(37,79,85));
            let response=c.hit_drag(x,220.0,width,110.0,("portrait-reminder-wheel",column));
            response.widget_info(||egui::WidgetInfo::labeled(egui::WidgetType::DragValue,c.live,["日期","小时","分钟"][column]));
            if response.has_focus(){c.stroke(x,220.0,width,110.0,10.0,1.3,theme::CYAN);}
            if response.hovered(){
                let (delta,now)=ctx.input(|i|(i.raw_scroll_delta.y,i.time));
                if delta!=0.0{
                    let delta=-delta;
                    if now-self.wheel_at[column]>0.25||self.wheel[column]*delta<0.0{self.wheel[column]=0.0;}
                    self.wheel_at[column]=now;self.wheel[column]+=delta;
                    let steps=(self.wheel[column]/40.0).trunc() as i64;self.wheel[column]-=steps as f32*40.0;self.adjust(column,steps);
                    ctx.input_mut(|i|{i.raw_scroll_delta=egui::Vec2::ZERO;i.smooth_scroll_delta=egui::Vec2::ZERO;});
                }
            }
            if response.drag_started(){self.drag[column]=0.0;}
            if response.dragged(){
                self.drag[column]-=response.drag_delta().y/c.s;let steps=(self.drag[column]/32.0).trunc() as i64;
                self.drag[column]-=steps as f32*32.0;self.adjust(column,steps);
            }
            if response.clicked(){if let Some(pointer)=response.interact_pointer_pos(){let y=c.to_local(pointer).y;if y<257.0{self.adjust(column,-1);}else if y>293.0{self.adjust(column,1);}}}
            for (key,step) in [(egui::Key::ArrowUp,-1),(egui::Key::ArrowDown,1)]{if response.has_focus()&&ctx.input(|i|i.key_pressed(key)){self.adjust(column,step);}}
            let position=ctx.animate_value_with_time(egui::Id::new(("portrait-reminder-position",column)),self.positions[column] as f32,if self.focus{0.0}else{0.13});
            let clipped=c.clipped(x,220.0,width,110.0);
            for value in position.floor() as i64-2..=position.floor() as i64+2{
                let distance=value as f32-position;let strength=(1.0-distance.abs()*0.45).clamp(0.0,1.0);
                clipped.centered_text(x+width/2.0,275.0+distance*34.0,&self.label(column,value),if column==0{13.0+strength*3.0}else{14.0+strength*7.0},if strength>0.7{W::Semibold}else{W::Regular},paint_color(strength),H::Center);
            }
        }
        let note=if !self.error.is_empty(){self.error.as_str()}else{self.repeat_note.as_deref().filter(|n|!n.is_empty()).unwrap_or("滚轮、上下拖动或点选调整")};
        c.text(20.0,357.0,&c.fit(note,11.0,W::Regular,252.0),11.0,W::Regular,if self.error.is_empty(){theme::GRAY}else{theme::RED},H::Left);
        c.text(20.0,386.0,"每周重复（可选）",11.0,W::Regular,theme::GRAY,H::Left);
        if crate::portrait::button(c,20.0,440.0,252.0,if has_reminder{"取消提醒"}else{"暂不提醒"},"task-reminder-skip"){return Some(PickerAction::Skip);}
        if crate::portrait::button(c,20.0,484.0,112.0,"取消","task-reminder-cancel"){return Some(PickerAction::Cancel);}
        if crate::portrait::button(c,160.0,484.0,112.0,"完成","task-reminder-save"){
            if self.repeat_note.as_deref().is_none_or(str::is_empty)&&self.selected()<=Local::now().naive_local(){self.error="请选择晚于当前的时间".into();}else{return Some(PickerAction::Save);}
        }
        None
    }
}

fn paint_color(strength:f32)->Color32{crate::paint::mix(theme::DIM,theme::WHITE,strength)}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wheels_wrap_time_and_cross_months_without_invalid_dates(){
        let mut picker=ReminderPicker::new(Some("2028-02-28 23:59"));
        picker.adjust(0,1);picker.adjust(1,1);picker.adjust(2,1);
        assert_eq!(picker.due(),"2028-02-29 00:00");
        picker.adjust(0,1);assert_eq!(picker.due(),"2028-03-01 00:00");
        picker.adjust(1,-1);picker.adjust(2,-1);assert_eq!(picker.due(),"2028-03-01 23:59");
    }
    #[test]
    fn date_wheel_stops_at_today_and_ignores_date_overflow(){
        let mut picker=ReminderPicker::new(None);picker.adjust(0,-10000);
        assert_eq!(picker.selected().date(),Local::now().date_naive());
        let due=picker.due();picker.adjust(0,i64::MAX);assert_eq!(picker.due(),due);
        assert_eq!(picker.label(0,picker.positions[0]),"今天");
    }
}
