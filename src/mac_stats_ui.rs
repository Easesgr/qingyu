//! A quiet, local system monitor. Percent charts share a fixed 0–100% scale.
use crate::{app::App, mac_stats::{NetworkStatus, Snapshot}, paint::{self, Card, H}, theme::{self, W}};
use eframe::egui::{self, Color32, Pos2, Rect};
use std::{collections::VecDeque, time::{Duration, SystemTime}};

const WINDOW: Duration = Duration::from_secs(60);

#[derive(Clone, Copy)]
enum Metric { Cpu, Memory, Gpu, Download, Upload }

#[derive(Clone)]
struct Sample {
    at: SystemTime,
    cpu: Option<f64>, memory: Option<f64>, gpu: Option<f64>,
    download: Option<f64>, upload: Option<f64>,
}
impl Sample {
    fn value(&self, metric: Metric) -> Option<f64> {
        match metric { Metric::Cpu=>self.cpu, Metric::Memory=>self.memory, Metric::Gpu=>self.gpu,
            Metric::Download=>self.download, Metric::Upload=>self.upload }
    }
}

#[derive(Default)]
pub(crate) struct History { samples: VecDeque<Sample> }
impl History {
    pub(crate) fn observe(&mut self, snapshot: &Snapshot) {
        let Some(at)=snapshot.sampled_at else{return;};
        if self.samples.back().is_some_and(|sample|sample.at==at){return;}
        if self.samples.back().is_some_and(|sample|at.duration_since(sample.at).map_or(true,|gap|gap>Duration::from_secs(8))){self.samples.clear();}
        self.samples.push_back(Sample{at,cpu:snapshot.cpu_usage,memory:memory_percent(snapshot),gpu:gpu_usage(snapshot),
            download:snapshot.network.as_ref().and_then(|n|n.received_bytes_per_sec),
            upload:snapshot.network.as_ref().and_then(|n|n.sent_bytes_per_sec)});
        while self.samples.front().is_some_and(|sample|at.duration_since(sample.at).unwrap_or_default()>WINDOW)||self.samples.len()>64{self.samples.pop_front();}
    }

    fn chart(&self,c:&Card,rect:Rect,metric:Metric,max:f64,color:Color32){
        let end=self.samples.back().map(|s|s.at);
        let mut previous:Option<(SystemTime,Pos2)>=None;
        c.seg((rect.left(),rect.bottom()),(rect.right(),rect.bottom()),0.6,theme::TRACK);
        for sample in &self.samples {
            let Some(value)=sample.value(metric).filter(|v|v.is_finite()&&*v>=0.0) else{previous=None;continue;};
            let age=end.and_then(|at|at.duration_since(sample.at).ok()).unwrap_or_default().as_secs_f32();
            let point=Pos2::new(rect.right()-rect.width()*(age/60.0).clamp(0.0,1.0),rect.bottom()-rect.height()*(value/max.max(1.0)).clamp(0.0,1.0) as f32);
            if let Some((_,p))=previous.filter(|(at,_)|sample.at.duration_since(*at).unwrap_or_default()<=Duration::from_secs(6)){
                if matches!(metric,Metric::Upload){
                    let length=p.distance(point);let mut offset=0.0;
                    while offset<length{let a=p.lerp(point,offset/length);let b=p.lerp(point,((offset+3.0)/length).min(1.0));c.seg((a.x,a.y),(b.x,b.y),1.2,color);offset+=5.0;}
                }else{c.seg((p.x,p.y),(point.x,point.y),1.5,color);}
            }
            previous=Some((sample.at,point));
        }
        if let Some((_,point))=previous {c.circle(point.x,point.y,1.8,color);}
    }

    fn network_max(&self)->f64{
        self.samples.iter().flat_map(|s|[s.download,s.upload]).flatten().filter(|v|v.is_finite()).fold(1024.0,f64::max)
    }
}

fn memory_percent(s:&Snapshot)->Option<f64>{(s.memory_valid&&s.memory_total>0).then(||s.memory_used as f64/s.memory_total as f64*100.0)}
fn gpu_usage(s:&Snapshot)->Option<f64>{s.gpus.iter().filter_map(|gpu|gpu.usage).filter(|v|v.is_finite()).max_by(f64::total_cmp)}
fn percent(value:Option<f64>)->String{value.filter(|v|v.is_finite()).map_or_else(||"—".into(),|v|format!("{:.0}%",v.clamp(0.0,100.0)))}
fn cpu_detail(s:&Snapshot)->String{
    if s.cpu_name.is_empty(){"正在读取处理器信息…".into()}else if s.logical_cores>0{format!("{} · {} 个逻辑核心",s.cpu_name,s.logical_cores)}else{s.cpu_name.clone()}
}

fn bytes(value:u64)->String{
    const UNITS:[&str;5]=["B","KiB","MiB","GiB","TiB"];
    let mut n=value as f64;let mut unit=0;
    while n>=1024.0&&unit<UNITS.len()-1{n/=1024.0;unit+=1;}
    if unit==0||n>=100.0 {format!("{n:.0} {}",UNITS[unit])}else{format!("{n:.1} {}",UNITS[unit])}
}
fn rate(value:Option<f64>)->String{value.filter(|v|v.is_finite()&&*v>=0.0).map_or_else(||"—".into(),|v|format!("{}/s",bytes(v.min(u64::MAX as f64)as u64)))}
fn memory_value(s:&Snapshot)->String{
    if !s.memory_valid||s.memory_total==0{return "不可用".into();}
    let unit=if s.memory_total>=1024u64.pow(4){1024f64.powi(4)}else{1024f64.powi(3)};
    format!("{:.1} / {:.0} {}",s.memory_used as f64/unit,s.memory_total as f64/unit,if unit==1024f64.powi(4){"TiB"}else{"GiB"})
}
fn gpu_value(s:&Snapshot)->String{
    gpu_usage(s).map_or_else(||if s.gpus.is_empty(){"不可用".into()}else{"未提供占用率".into()},|value|percent(Some(value)))
}
fn network_name(s:&Snapshot)->&str{
    match s.network_status {
        NetworkStatus::Online=>s.network.as_ref().map_or("网络已连接",|n|n.interface.as_str()),
        NetworkStatus::Offline=>"未连接",NetworkStatus::Unavailable=>"不可用",NetworkStatus::Unknown=>"正在读取…",
    }
}
fn subtitle(s:&Snapshot)->&str{
    if !s.supported{"仅支持 macOS"}else if s.paused{"采集已暂停"}else if s.error.is_some(){"部分指标暂不可用"}
    else if s.sampled_at.is_none(){"正在读取本机状态…"}else{"每 2 秒更新 · 最近 1 分钟"}
}

fn metric_label(c:&Card,rect:Rect,id:&str,text:&str){
    let response=c.ui.interact(c.rect(rect.left(),rect.top(),rect.width(),rect.height()),egui::Id::new(("mac-stats-metric",c.live,id)),egui::Sense::hover()).on_hover_text(text);
    response.widget_info(||egui::WidgetInfo::labeled(egui::WidgetType::Label,c.live,text));
}
fn meter(c:&Card,x:f32,y:f32,width:f32,value:Option<f64>){
    c.fill(x,y,width,3.0,1.5,theme::TRACK);
    if let Some(value)=value.filter(|v|v.is_finite()){let width=width*(value/100.0).clamp(0.0,1.0)as f32;if width>0.0{c.fill(x,y,width.max(1.5),3.0,1.5,theme::CYAN);}}
}
fn memory_detail(s:&Snapshot)->String{
    if !s.memory_valid{return "系统未提供内存指标".into();}
    format!("压缩 {} · 交换 {}",bytes(s.memory_compressed),if s.swap_valid{bytes(s.swap_used)}else{"不可用".into()})
}
fn disk_value(s:&Snapshot)->String{if s.disk_valid{format!("{} 可用",bytes(s.disk_available))}else{"不可用".into()}}
fn disk_detail(s:&Snapshot)->String{format!("读取 {} · 写入 {}",rate(s.disk_read_bytes_per_sec),rate(s.disk_write_bytes_per_sec))}
fn gpu_detail(s:&Snapshot)->String{
    if s.gpus.is_empty(){"系统未提供 GPU 指标".into()}else{s.gpus.iter().map(|gpu|format!("{} · {}",gpu.name,percent(gpu.usage))).collect::<Vec<_>>().join("\n")}
}

impl App {
    pub(crate) fn page_mac_stats(&mut self,c:&Card,side:bool){
        let s=&self.mac_snapshot;let width=self.expanded_width();
        let inset=crate::portrait::INSET;
        let left=if side{inset}else{44.0};let body_width=if side{width-inset*2.0}else{331.0};
        if side{c.centered_text(left,119.0,"Mac 状态",17.0,W::Semibold,theme::WHITE,H::Left);}
        let summary_y=if side{142.0}else{43.0};
        let summary=c.hit(left,summary_y-12.0,body_width-34.0,24.0,"mac-stats-summary").on_hover_text(s.error.as_deref().unwrap_or("CPU、内存、磁盘、GPU 和主网络接口；趋势只保留在本次运行内。"));
        summary.widget_info(||egui::WidgetInfo::labeled(egui::WidgetType::Label,c.live,subtitle(s)));
        c.centered_text(left,summary_y,&c.fit(subtitle(s),10.5,W::Regular,body_width-34.0),10.5,W::Regular,theme::GRAY,H::Left);
        let refresh_y=if side{107.0}else{29.0};
        let refresh=c.hit(left+body_width-28.0,refresh_y,28.0,28.0,"mac-stats-refresh").on_hover_text("刷新 Mac 状态");
        refresh.widget_info(||egui::WidgetInfo::labeled(egui::WidgetType::Button,c.live,"刷新 Mac 状态"));
        if refresh.hovered(){c.fill(left+body_width-28.0,refresh_y,28.0,28.0,7.0,theme::BTN_HOVER);}
        if refresh.has_focus(){c.stroke(left+body_width-28.0,refresh_y,28.0,28.0,7.0,1.4,theme::CYAN);}
        paint::glyph(c,"refresh",left+body_width-14.0,refresh_y+14.0,15.0,theme::GRAY);
        let clicked=refresh.clicked();
        if !s.supported{
            let top=if side{158.0}else{58.0};crate::portrait::empty(c,Rect::from_min_max(egui::pos2(left,top),egui::pos2(left+body_width,self.content_rect().bottom())),"mac-stats","请在 Mac 上查看系统状态","当前平台不提供这些硬件指标");
        }else if side{
            let height=(self.expanded_height()-176.0).max(1.0);
            let mut ui=crate::portrait::scroll_ui(c,left,158.0,body_width,height,"mac-stats");
            let extra=((self.expanded_height()-532.0)/5.0).clamp(0.0,55.0);
            egui::ScrollArea::vertical().id_salt("mac-stats-side").auto_shrink([false,false]).show(&mut ui,|ui|{
                let row_width=(ui.available_width()/c.s).max(1.0);
                for (index,row_h) in [60.0,60.0,74.0,56.0,74.0].into_iter().enumerate(){
                    let row_h=row_h+extra;
                    let (rect,_)=ui.allocate_exact_size(egui::vec2(row_width*c.s,row_h*c.s),egui::Sense::hover());
                    let mut row=Card::new(ui,rect.min,ui.clip_rect());row.s=c.s;row.live=c.live;
                    self.stats_side_row(&row,index,row_width,row_h);
                }
            });
        }else{self.stats_compact_rows(c);}
        if clicked{self.mac_stats.refresh();}
    }

    fn stats_compact_rows(&self,c:&Card){
        let s=&self.mac_snapshot;
        let titles=["CPU","内存","磁盘","GPU","网络"];
        let values=[percent(s.cpu_usage),memory_value(s),disk_value(s),gpu_value(s),network_name(s).into()];
        let details=[format!("{}\nCPU 使用率 {}",cpu_detail(s),values[0]),
            format!("内存 {}\n{}",values[1],memory_detail(s)),
            format!("{}\n所有物理磁盘 I/O：{}",if s.disk_valid{format!("启动磁盘 {} / {}",values[2],bytes(s.disk_total))}else{"启动磁盘容量不可用".into()},disk_detail(s)),gpu_detail(s),
            format!("主网络接口 {}\n下载 {}\n上传 {}",network_name(s),rate(s.network.as_ref().and_then(|n|n.received_bytes_per_sec)),rate(s.network.as_ref().and_then(|n|n.sent_bytes_per_sec)))];
        for index in 0..5{
            let row_height=(self.content_rect().bottom()-58.0)/5.0;let y=58.0+index as f32*row_height;
            c.centered_text(44.0,y+14.0,titles[index],11.5,W::Semibold,theme::GRAY,H::Left);
            if index<4{
                c.centered_text(207.0,y+14.0,&c.fit(&values[index],if index==1{11.0}else{12.0},W::Semibold,110.0),if index==1{11.0}else{12.0},W::Semibold,theme::WHITE,H::Right);
            }else{
                c.centered_text(100.0,y+7.0,&format!("↓ {}",rate(s.network.as_ref().and_then(|n|n.received_bytes_per_sec))),10.5,W::Regular,theme::WHITE,H::Left);
                c.centered_text(100.0,y+22.0,&format!("↑ {}",rate(s.network.as_ref().and_then(|n|n.sent_bytes_per_sec))),10.5,W::Regular,theme::GRAY,H::Left);
                if s.network_status==NetworkStatus::Offline{c.centered_text(290.0,y+14.0,"未连接",11.0,W::Regular,theme::GRAY,H::Center);}
            }
            let graph=Rect::from_min_size(egui::pos2(223.0,y+3.0),egui::vec2(152.0,row_height-9.0));
            match index{
                0=>self.stats_history.chart(c,graph,Metric::Cpu,100.0,theme::CYAN),
                1=>self.stats_history.chart(c,graph,Metric::Memory,100.0,theme::CYAN),
                2=>meter(c,223.0,y+13.0,152.0,(s.disk_valid&&s.disk_total>0).then(||100.0-s.disk_available as f64/s.disk_total as f64*100.0)),
                3=>self.stats_history.chart(c,graph,Metric::Gpu,100.0,theme::CYAN),
                _=>if s.network_status!=NetworkStatus::Offline{
                    self.stats_history.chart(c,graph,Metric::Download,self.stats_history.network_max(),theme::CYAN);
                    self.stats_history.chart(c,graph,Metric::Upload,self.stats_history.network_max(),theme::GRAY);
                },
            }
            if index<4{c.seg((44.0,y+row_height-2.0),(375.0,y+row_height-2.0),0.5,theme::TRACK);}
            metric_label(c,Rect::from_min_size(egui::pos2(44.0,y),egui::vec2(331.0,row_height-2.0)),titles[index],&details[index]);
        }
    }

    fn stats_side_row(&self,c:&Card,index:usize,width:f32,height:f32){
        let s=&self.mac_snapshot;let title=["CPU","内存","启动磁盘","GPU","网络"][index];
        let value=match index{0=>percent(s.cpu_usage),1=>memory_value(s),2=>disk_value(s),3=>gpu_value(s),_=>network_name(s).into()};
        c.centered_text(0.0,12.0,title,12.0,W::Semibold,theme::GRAY,H::Left);
        c.centered_text(width,12.0,&c.fit(&value,12.0,W::Semibold,width-66.0),12.0,W::Semibold,theme::WHITE,H::Right);
        let detail=match index{
            0=>{
                let extra=(height-60.0).max(0.0);
                self.stats_history.chart(c,Rect::from_min_size(egui::pos2(0.0,30.0),egui::vec2(width,20.0+extra*0.5)),Metric::Cpu,100.0,theme::CYAN);
                if extra>=24.0{c.centered_text(0.0,height-12.0,&c.fit(&cpu_detail(s),10.5,W::Regular,width),10.5,W::Regular,theme::GRAY,H::Left);}
                format!("{}\nCPU 使用率 {value}",cpu_detail(s))
            },
            1=>{
                meter(c,0.0,30.0,width,memory_percent(s));
                let detail=memory_detail(s);
                c.centered_text(0.0,47.0,&c.fit(&detail,10.5,W::Regular,width),10.5,W::Regular,theme::GRAY,H::Left);
                if height>=84.0{self.stats_history.chart(c,Rect::from_min_size(egui::pos2(0.0,65.0),egui::vec2(width,height-76.0)),Metric::Memory,100.0,theme::CYAN);}
                format!("内存 {value}\n{detail}")
            },
            2=>{
                meter(c,0.0,30.0,width,(s.disk_valid&&s.disk_total>0).then(||100.0-s.disk_available as f64/s.disk_total as f64*100.0));
                c.centered_text(0.0,46.0,&if s.disk_valid{format!("容量 {}",bytes(s.disk_total))}else{"磁盘容量不可用".into()},10.5,W::Regular,theme::GRAY,H::Left);
                let io=disk_detail(s);
                c.centered_text(0.0,63.0,&c.fit(&io,10.5,W::Regular,width),10.5,W::Regular,theme::GRAY,H::Left);
                if height>=98.0&&s.disk_valid{c.centered_text(0.0,height-12.0,&format!("已用 {}",bytes(s.disk_total.saturating_sub(s.disk_available))),10.5,W::Regular,theme::GRAY,H::Left);}
                format!("启动磁盘 {value}\n所有物理磁盘 I/O：{io}")
            },
            3=>{
                self.stats_history.chart(c,Rect::from_min_size(egui::pos2(0.0,29.0),egui::vec2(width,18.0+(height-56.0).max(0.0)*0.5)),Metric::Gpu,100.0,theme::CYAN);
                if gpu_usage(s).is_none(){c.centered_text(0.0,39.0,&c.fit(s.gpus.first().map_or("系统未提供 GPU 指标",|g|g.name.as_str()),10.5,W::Regular,width),10.5,W::Regular,theme::GRAY,H::Left);}
                else if height>=80.0{c.centered_text(0.0,height-12.0,&c.fit(&s.gpus.iter().map(|g|g.name.as_str()).collect::<Vec<_>>().join(" · "),10.5,W::Regular,width),10.5,W::Regular,theme::GRAY,H::Left);}
                gpu_detail(s)
            },
            _=>{
                let down=rate(s.network.as_ref().and_then(|n|n.received_bytes_per_sec));let up=rate(s.network.as_ref().and_then(|n|n.sent_bytes_per_sec));
                c.centered_text(0.0,35.0,&format!("↓ {down}"),11.0,W::Semibold,theme::WHITE,H::Left);
                c.centered_text(width,35.0,&format!("↑ {up}"),11.0,W::Regular,theme::GRAY,H::Right);
                let graph=Rect::from_min_size(egui::pos2(0.0,49.0),egui::vec2(width,(height-55.0).max(19.0)));
                self.stats_history.chart(c,graph,Metric::Download,self.stats_history.network_max(),theme::CYAN);
                self.stats_history.chart(c,graph,Metric::Upload,self.stats_history.network_max(),theme::GRAY);
                format!("主网络接口 {}\n下载 {down}\n上传 {up}",network_name(s))
            }
        };
        if index<4{c.seg((0.0,height-1.0),(width,height-1.0),0.5,theme::TRACK);}
        metric_label(c,Rect::from_min_size(Pos2::ZERO,egui::vec2(width,height)),title,&detail);
    }

    pub(crate) fn mac_stats_settings(&mut self,ui:&mut egui::Ui){
        use crate::extras::{settings_card,settings_hint,settings_label};
        let s=self.mac_snapshot.clone();
        settings_label(ui,"本机概览");
        settings_card(ui,|ui|{
            ui.horizontal(|ui|{
                ui.label(subtitle(&s));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{if ui.add_enabled(s.supported,egui::Button::new("刷新状态")).clicked(){self.mac_stats.refresh();}});
            });
            if let Some(error)=&s.error{settings_hint(ui,error);}
            ui.separator();
            egui::Grid::new("mac-stats-details").num_columns(2).spacing([20.0,10.0]).show(ui,|ui|{
                for (title,value) in [("CPU",percent(s.cpu_usage)),("处理器",cpu_detail(&s)),
                    ("内存",memory_value(&s)),("压缩与交换",memory_detail(&s)),("启动磁盘",disk_value(&s)),("物理磁盘读写",disk_detail(&s)),("GPU",gpu_detail(&s)),
                    ("主网络接口",network_name(&s).into()),("下载",rate(s.network.as_ref().and_then(|n|n.received_bytes_per_sec))),("上传",rate(s.network.as_ref().and_then(|n|n.sent_bytes_per_sec)))]{
                    ui.label(title);ui.add(egui::Label::new(value).wrap());ui.end_row();
                }
            });
        });
        settings_label(ui,"采集与显示");
        settings_card(ui,|ui|{
            settings_hint(ui,"每 2 秒更新，趋势保留 1 分钟；锁屏时暂停。");
        });
        settings_label(ui,"开源来源");
        settings_card(ui,|ui|{
            ui.hyperlink_to("Stats · exelban/stats","https://github.com/exelban/stats");
        });
    }
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn mac_stats_history_keeps_one_minute_and_breaks_across_pauses(){
        let mut history=History::default();let start=SystemTime::UNIX_EPOCH+Duration::from_secs(1000);
        let mut snapshot=Snapshot{sampled_at:Some(start),cpu_usage:Some(20.0),..Default::default()};
        for i in 0..100{
            snapshot.sampled_at=Some(start+Duration::from_secs(i*2));history.observe(&snapshot);history.observe(&snapshot);
        }
        assert_eq!(history.samples.len(),31,"每帧观察快照不应增加样本，历史只保留一分钟");
        snapshot.sampled_at=Some(start+Duration::from_secs(240));snapshot.cpu_usage=None;history.observe(&snapshot);
        assert_eq!(history.samples.len(),1,"恢复采集后的曲线不能连接暂停前的样本");
        assert_eq!(history.samples[0].cpu,None,"首个差分样本不能画成零使用率");
    }
}
