//! A shared side-panel spring keeps width, height and content on one timeline.
//! Changing direction preserves velocity; resident-size changes have their own
//! critically damped springs so a weather update does not jump the silhouette.
use crate::anim::{smoothstep,Spring};
#[cfg(test)]
use crate::side_dock::{PANEL_W,PANEL_H};
use eframe::egui::Vec2;

pub(crate) struct Motion{
    expansion:Spring,
    width:Spring,
    height:Spring,
    active:bool,
}

impl Motion{
    pub fn new()->Self{Self{expansion:Spring::new(0.0,310.0,0.9),width:Spring::new(44.0,360.0,1.0),height:Spring::new(72.0,360.0,1.0),active:false}}
    pub fn progress(&self)->f32{self.expansion.pos.max(0.0)}
    pub fn deactivate(&mut self){self.active=false;self.expansion.snap(0.0);}
    pub fn reset(&mut self,open:bool){self.active=false;self.expansion.snap(if open{1.0}else{0.0});}
    pub fn step(&mut self,base:Vec2,panel:Vec2,open:bool,preview:bool,dt:f32,reduced:bool)->(Vec2,Vec2,Vec2){
        if !self.active{self.width.snap(base.x);self.height.snap(base.y);self.active=true;}
        self.width.target=base.x;self.height.target=base.y;
        self.expansion.target=if open{1.0}else if preview{0.008}else{0.0};
        self.expansion.tune(if open{310.0}else{420.0},if open{0.9}else{1.0});
        for spring in [&mut self.width,&mut self.height,&mut self.expansion]{
            if reduced{spring.snap(spring.target);}else{spring.step(dt);}
        }
        if self.expansion.settled(0.15){self.expansion.snap(self.expansion.target);}
        if self.width.settled(base.x){self.width.snap(base.x);}
        if self.height.settled(base.y){self.height.snap(base.y);}
        let p=self.progress();let resident=Vec2::new(self.width.pos,self.height.pos);
        let velocity=Vec2::new(self.width.vel,self.height.vel)*(1.0-p)+(panel-resident)*self.expansion.vel;
        (resident+(panel-resident)*p,velocity,base+(panel-base)*self.expansion.target)
    }
}

/// One curve in both directions avoids an opacity jump when interrupted.
pub(crate) fn content_alpha(progress:f32)->f32{smoothstep(0.04,0.90,progress)}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn side_geometry_has_one_progress_at_60_and_120_hz_and_on_reversal(){
        let base=Vec2::new(44.0,124.0);
        for hz in [60.0,120.0]{
            let mut motion=Motion::new();let mut last=base;let mut largest=0.0f32;
            for frame in 0..(hz as usize*3){
                let open=frame<(hz*0.15) as usize||frame>=(hz*0.28) as usize&&frame<(hz*0.9) as usize;
                let (size,_,_)=motion.step(base,Vec2::new(PANEL_W,PANEL_H),open,false,1.0/hz,false);
                let x=(size.x-base.x)/(PANEL_W-base.x);let y=(size.y-base.y)/(PANEL_H-base.y);
                assert!((x-y).abs()<0.00001,"independent axis timing must not distort the side panel");
                largest=largest.max((size-last).length());last=size;
                assert!(size.x>=base.x&&size.y>=base.y&&size.y<600.0);
            }
            assert!(largest<80.0,"a single frame must not jump: {largest}");
            assert_eq!(motion.progress(),0.0);
        }
    }
    #[test]
    fn side_reduced_motion_snaps_the_shared_progress_and_resident_size(){
        let mut motion=Motion::new();
        assert_eq!(motion.step(Vec2::new(6.0,52.0),Vec2::new(PANEL_W,PANEL_H),true,false,0.016,true).0,Vec2::new(PANEL_W,PANEL_H));
        assert_eq!(motion.step(Vec2::new(44.0,124.0),Vec2::new(PANEL_W,PANEL_H),false,false,0.016,true).0,Vec2::new(44.0,124.0));
        assert_eq!(content_alpha(0.0),0.0);assert_eq!(content_alpha(1.0),1.0);
        assert!(content_alpha(0.5)>0.5,"content should already be present halfway through expansion");
    }
}
