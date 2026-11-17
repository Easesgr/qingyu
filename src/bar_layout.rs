//! 常驻组件按实际占位排布，并在开关变化时连续补位。
use crate::anim::Spring;

// 保留持久化槽位编号，旧排序可以直接过滤为时间/天气与音乐。
pub(crate) const CLOCK: usize = 0;
pub(crate) const WEATHER: usize = 5;
pub(crate) const MUSIC: usize = 6;
pub(crate) const BELL: usize = 8;
pub(crate) const PET: usize = 9;
pub(crate) const TEAM: usize = 10;
pub(crate) const WEATHER_WIDTH: f32 = 38.0;
const WIDTHS: [f32;11] = [44.0,0.0,0.0,0.0,0.0,WEATHER_WIDTH,14.0,0.0,20.0,28.0,70.0];
pub(crate) const DEFAULT_ORDER: [usize;2]=[CLOCK,MUSIC];
pub(crate) fn valid_order(order:&[usize])->bool {
    order.len()==DEFAULT_ORDER.len() && DEFAULT_ORDER.iter().all(|slot|order.iter().filter(|value|*value==slot).count()==1)
}
pub(crate) fn migrate_order(order:&[usize])->Vec<usize>{
    if order.is_empty(){return Vec::new();}
    let mut result=Vec::new();
    for &slot in order{if DEFAULT_ORDER.contains(&slot)&&!result.contains(&slot){result.push(slot);}}
    for slot in DEFAULT_ORDER{if !result.contains(&slot){result.push(slot);}}
    result
}

pub(crate) fn animations_enabled()->bool {
    #[cfg(windows)] {
        static ENABLED:std::sync::OnceLock<bool>=std::sync::OnceLock::new();
        *ENABLED.get_or_init(||{
            use windows::Win32::{Foundation::BOOL,UI::WindowsAndMessaging::*};
            let mut enabled=BOOL(1);
            unsafe {let _=SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION,0,Some((&mut enabled as *mut BOOL).cast()),SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0));}
            enabled.as_bool()
        })
    }
    #[cfg(not(windows))] {true}
}

#[derive(Clone,Debug)]
pub(crate) struct Layout {
    pub width:f32,
    pub x:Vec<Option<f32>>,
    pub widths:Vec<f32>,
}
impl Layout {
    pub fn new(music:bool,bell:bool,weather:bool)->Self {
        let mut x=vec![None;WIDTHS.len()];let mut cursor=16.0;
        for (slot,on) in [(CLOCK,true),(WEATHER,weather),(MUSIC,music&&!bell),(BELL,bell)] {
            if on {x[slot]=Some(cursor+WIDTHS[slot]/2.0);cursor+=WIDTHS[slot]+12.0;}
        }
        let content_width=cursor-12.0+16.0;
        for value in x.iter_mut().flatten(){*value-=content_width/2.0;}
        Self{width:content_width.max(96.0),x,widths:WIDTHS.to_vec()}
    }
    pub fn widen(&mut self,slot:usize,extra:f32){
        if extra<=0.0{return;}
        let Some(center)=self.x.get(slot).copied().flatten() else{return;};
        for (i,x) in self.x.iter_mut().enumerate(){if i!=slot{if let Some(x)=x{*x+=if *x<center{-extra/2.0}else{extra/2.0};}}}
        self.widths[slot]+=extra;self.width+=extra;
    }
    pub fn widen_clock(&mut self,extra:f32){self.widen(CLOCK,extra);}
    /// Companions frame the user's retained clock/music order. They do not
    /// become persisted reorderable modules or stretch with a custom length.
    pub fn companions(&mut self,pet:bool,team:bool){
        let left=if pet{WIDTHS[PET]+12.0}else{0.0};
        let right=if team{WIDTHS[TEAM]+12.0}else{0.0};
        for x in self.x.iter_mut().flatten(){*x+=(left-right)/2.0;}
        self.width+=left+right;
        if pet{self.x[PET]=Some(-self.width/2.0+16.0+WIDTHS[PET]/2.0);}
        if team{self.x[TEAM]=Some(self.width/2.0-16.0-WIDTHS[TEAM]/2.0);}
    }
    /// Spend a custom length on readable content before distributing spare
    /// spacing. Automatic and short bars keep their compact identities.
    pub fn enrich(&mut self,requested:u32){
        if requested==0{return;}
        let spare=(requested as f32-self.width).max(0.0);
        if self.x[MUSIC].is_some()&&spare>=52.0{self.widen(MUSIC,spare.min(220.0));}
        else if self.x[MUSIC].is_none()&&self.x[TEAM].is_some()&&spare>=60.0{self.widen(TEAM,60.0);}
    }
    /// Additional length is spacing. Component widths and fonts stay fixed;
    /// a short request yields to the real content's minimum width.
    pub fn set_length(&mut self,requested:u32){
        if requested==0{return;}
        let width=(crate::island_position::length(requested) as f32).max(self.width);
        let extra=width-self.width;
        let mut slots=self.x.iter().enumerate().filter_map(|(i,x)|x.map(|x|(i,x))).collect::<Vec<_>>();
        slots.sort_by(|a,b|a.1.total_cmp(&b.1));
        let count=slots.len() as f32;
        for (index,(slot,x)) in slots.into_iter().enumerate(){self.x[slot]=Some(x+extra*((index+1) as f32/(count+1.0)-0.5));}
        self.width=width;
    }
    pub fn reorder(&mut self,order:&[usize]){
        if !valid_order(order){return;}
        let original=self.x.clone();
        let mut cursor=original.iter().enumerate().filter_map(|(i,x)|x.map(|x|x-self.widths[i]/2.0)).fold(f32::INFINITY,f32::min);
        for slot in order.iter().copied().chain(std::iter::once(BELL)){
            let slots=if slot==CLOCK&&original[WEATHER].is_some(){vec![CLOCK,WEATHER]}else{vec![slot]};
            let left=slots.iter().filter_map(|&i|original[i].map(|x|x-self.widths[i]/2.0)).fold(f32::INFINITY,f32::min);
            let right=slots.iter().filter_map(|&i|original[i].map(|x|x+self.widths[i]/2.0)).fold(f32::NEG_INFINITY,f32::max);
            if !left.is_finite(){continue;}
            for i in slots{if let Some(x)=original[i]{self.x[i]=Some(cursor+x-left);}}
            cursor+=right-left+12.0;
        }
    }
}

pub(crate) struct Motion {
    pub target: Layout,
    positions: Vec<Spring>,
    initialized: bool,
}
impl Default for Motion {
    fn default()->Self {Self {target:Layout::new(false,false,false),positions:vec![Spring::new(0.0,360.0,0.76);WIDTHS.len()],initialized:false}}
}
impl Motion {
    pub fn step(&mut self,target:Layout,dt:f32,animate:bool) {
        self.positions.resize(target.x.len(),Spring::new(0.0,360.0,0.76));
        for (i,x) in target.x.iter().enumerate() {
            if let Some(x)=x {
                let spring=&mut self.positions[i];spring.target=*x;
                if !self.initialized || self.target.x.get(i).copied().flatten().is_none() || !animate {spring.snap(*x);}else{spring.step(dt);}
            }
        }
        self.target=target;self.initialized=true;
    }
    pub fn current(&self)->Layout {
        let mut layout=self.target.clone();
        for (i,x) in layout.x.iter_mut().enumerate() {if x.is_some(){*x=Some(self.positions[i].pos);}}
        layout
    }
    pub fn settled(&self)->bool {self.target.x.iter().enumerate().all(|(i,x)|x.is_none()||self.positions[i].settled(100.0))}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_order_filters_removed_slots_without_reordering_survivors(){
        for old in [vec![1,9,0,2,6,7],vec![0,2,6],vec![0,6]]{assert_eq!(migrate_order(&old),DEFAULT_ORDER);}
        assert_eq!(migrate_order(&[7,6,2,0,9,1]),[MUSIC,CLOCK]);
        assert_eq!(migrate_order(&[0,0,99]),DEFAULT_ORDER);
        assert!(migrate_order(&[]).is_empty());assert!(!valid_order(&[0,0]));
    }
    #[test]
    fn retained_components_never_overlap_in_any_order(){
        for order in [DEFAULT_ORDER,[MUSIC,CLOCK]]{for music in [false,true]{for bell in [false,true]{for weather in [false,true]{
            let mut layout=Layout::new(music,bell,weather);layout.reorder(&order);
            layout.widen_clock(40.0);layout.widen(WEATHER,18.0);
            let mut bounds:Vec<_>=layout.x.iter().enumerate().filter_map(|(i,x)|x.map(|x|(x-layout.widths[i]/2.0,x+layout.widths[i]/2.0))).collect();
            bounds.sort_by(|a,b|a.0.total_cmp(&b.0));
            assert!(bounds.windows(2).all(|pair|pair[1].0-pair[0].1>=11.9),"{layout:?}");
            assert!(bounds.first().unwrap().0>=-layout.width/2.0+15.9);
            assert!(bounds.last().unwrap().1<=layout.width/2.0-15.9);
            assert!(layout.x.iter().enumerate().all(|(i,x)|x.is_none()||[CLOCK,WEATHER,MUSIC,BELL].contains(&i)));
            if weather{assert!((layout.x[WEATHER].unwrap()-layout.x[CLOCK].unwrap()-(layout.widths[WEATHER]+layout.widths[CLOCK])/2.0-12.0).abs()<0.01);}
        }}}}
    }
    #[test]
    fn reordered_components_animate_then_settle(){
        let before=Layout::new(true,false,true);let mut after=before.clone();after.reorder(&[MUSIC,CLOCK]);
        let mut motion=Motion::default();motion.step(before.clone(),0.0,true);motion.step(after.clone(),1.0/60.0,true);
        assert_ne!(motion.current().x,after.x);
        for _ in 0..120{motion.step(after.clone(),1.0/60.0,true);}
        assert!((motion.current().x[CLOCK].unwrap()-after.x[CLOCK].unwrap()).abs()<0.01);
        motion.step(before.clone(),0.0,false);assert_eq!(motion.current().x,before.x);
    }
    #[test]
    fn requested_length_adds_spacing_without_scaling_or_hiding_controls(){
        for music in [false,true]{for weather in [false,true]{for period in [false,true]{
            let mut automatic=Layout::new(music,false,weather);if period{automatic.widen_clock(40.0);}automatic.widen(WEATHER,18.0);
            for request in [0,80,220,419,520]{
                let mut layout=automatic.clone();layout.set_length(request);
                assert_eq!(layout.widths,automatic.widths);
                assert!(layout.width>=automatic.width);
                let mut bounds=layout.x.iter().enumerate().filter_map(|(slot,x)|x.map(|x|(x-layout.widths[slot]/2.0,x+layout.widths[slot]/2.0))).collect::<Vec<_>>();
                bounds.sort_by(|a,b|a.0.total_cmp(&b.0));
                assert!(bounds.windows(2).all(|pair|pair[1].0-pair[0].1>=11.99));
                assert!(bounds.first().unwrap().0>=-layout.width/2.0+15.99);
                assert!(bounds.last().unwrap().1<=layout.width/2.0-15.99);
                if request==0{assert_eq!(layout.x,automatic.x);}
            }
        }}}
    }
    #[test]
    fn companions_stay_at_the_ends_without_colliding_with_reordered_content(){
        for order in [DEFAULT_ORDER,[MUSIC,CLOCK]]{for music in [false,true]{for pet in [false,true]{for team in [false,true]{
            let mut layout=Layout::new(music,false,true);layout.reorder(&order);
            layout.widen_clock(40.0);layout.widen(WEATHER,24.0);layout.companions(pet,team);
            for length in [0,80,520]{
                let mut layout=layout.clone();layout.set_length(length);
                let mut slots=layout.x.iter().enumerate().filter_map(|(slot,x)|x.map(|x|(slot,x-layout.widths[slot]/2.0,x+layout.widths[slot]/2.0))).collect::<Vec<_>>();
                slots.sort_by(|a,b|a.1.total_cmp(&b.1));
                assert!(slots.windows(2).all(|pair|pair[1].1-pair[0].2>=11.9),"{layout:?}");
                if pet{assert_eq!(slots.first().unwrap().0,PET);}
                if team{assert_eq!(slots.last().unwrap().0,TEAM);}
                assert!(slots.first().unwrap().1>=-layout.width/2.0+15.9);
                assert!(slots.last().unwrap().2<=layout.width/2.0-15.9);
            }
        }}}}
    }
}
