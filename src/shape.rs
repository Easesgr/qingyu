//! 刘海面板轮廓：顶部向外翻的「肩」+ 竖直侧边 + 底部大圆角。
//!
//! 轮廓左右对称、自上而下单调，所以按行切成梯形条填充（可处理凹肩），
//! 再沿外轮廓补一圈 1 物理像素的羽化带做抗锯齿——边缘不再有锯齿台阶。

use eframe::egui::{self, Color32, Pos2, Rect, Vec2};
use crate::model::Dock;

/// 左右停靠时，轮廓先在「贴顶」坐标系里生成：x 沿屏幕边缘（= 屏幕 y），
/// y 是离开边缘的距离（0 = 贴边）；再映射回屏幕。顶部停靠为恒等映射。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EdgeMap { dock: Dock, edge: f32 }

impl EdgeMap {
    pub const TOP: EdgeMap = EdgeMap { dock: Dock::Top, edge: 0.0 };
    /// `rect` 为屏幕坐标里的岛体外接矩形，返回映射与对应的贴顶矩形。
    pub fn of(rect: Rect, dock: Dock) -> (EdgeMap, Rect) {
        match dock {
            Dock::Top => (Self::TOP, rect),
            Dock::Left => (EdgeMap { dock, edge: rect.left() }, Rect::from_min_max(Pos2::new(rect.top(), 0.0), Pos2::new(rect.bottom(), rect.width()))),
            Dock::Right => (EdgeMap { dock, edge: rect.right() }, Rect::from_min_max(Pos2::new(rect.top(), 0.0), Pos2::new(rect.bottom(), rect.width()))),
        }
    }
    pub fn apply(self, p: Pos2) -> Pos2 {
        match self.dock {
            Dock::Top => p,
            Dock::Left => Pos2::new(self.edge + p.y, p.x),
            Dock::Right => Pos2::new(self.edge - p.y, p.x),
        }
    }
    fn code(self) -> f32 { match self.dock { Dock::Top => 0.0, Dock::Left => 1.0, Dock::Right => 2.0 } }
}

/// 贴着 `dock` 边缘的轮廓（屏幕坐标）。
pub fn outline_docked(rect: Rect, dock: Dock, flare: f32, flare_h: f32, radius: f32) -> Vec<Pos2> {
    let (map, frame) = EdgeMap::of(rect, dock);
    let rows=if dock.is_side(){side_sections(frame,flare,flare_h,radius)}else{sections(frame,flare,flare_h,radius)};
    outline_rows(&rows).into_iter().map(|p| map.apply(p)).collect()
}

/// Quarter-ellipse concave shoulders meet the screen and body with matching
/// tangents. Sampling the angle retains a vertical tangent at the screen;
/// a power curve sampled in depth would leave a visible diagonal seam.
pub fn side_sections(rect:Rect,flare:f32,flare_h:f32,radius:f32)->Vec<(f32,f32,f32)>{
    let r=radius.max(0.0).min(rect.width()*0.5).min(rect.height()*0.5);
    let depth=flare_h.max(0.01).min((rect.height()-r).max(0.01));
    let wing=flare.max(0.0);
    let mut rows=Vec::with_capacity(50);
    for index in 0..=24{
        let angle=index as f32/24.0*std::f32::consts::FRAC_PI_2;
        let (sin,cos)=angle.sin_cos();
        let extra=wing*(1.0-sin);
        rows.push((rect.top()+depth*(1.0-cos),rect.left()-extra,rect.right()+extra));
    }
    if rect.bottom()-r>rect.top()+depth+0.001{rows.push((rect.bottom()-r,rect.left(),rect.right()));}
    for index in 1..=24{
        let angle=index as f32/24.0*std::f32::consts::FRAC_PI_2;
        let (sin,cos)=angle.sin_cos();
        rows.push((rect.bottom()-r+r*sin,rect.left()+r-r*cos,rect.right()-r+r*cos));
    }
    rows
}

fn outline_rows(rows:&[(f32,f32,f32)])->Vec<Pos2>{
    rows.iter().map(|&(y,_,r)|Pos2::new(r,y)).chain(rows.iter().rev().map(|&(y,l,_)|Pos2::new(l,y))).collect()
}

/// The exact polygon is shared by native input and the visual-effect mask.
pub fn contains_outline(points:&[Pos2],point:Pos2)->bool{
    let Some(mut previous)=points.last().copied() else{return false;};
    let mut inside=false;
    for &next in points{
        let segment=next-previous;let offset=point-previous;
        if segment.length_sq()>0.0{
            let t=offset.dot(segment)/segment.length_sq();
            if (0.0..=1.0).contains(&t)&&(offset-segment*t).length_sq()<0.0001{return true;}
        }
        if (next.y>point.y)!=(previous.y>point.y)&&point.x<(previous.x-next.x)*(point.y-next.y)/(previous.y-next.y)+next.x{inside=!inside;}
        previous=next;
    }
    inside
}

/// One fluid surface for the horizontal navigation and disclosure. The smooth
/// minimum of their signed distances creates tangent-continuous shoulders,
/// including when the resident is wider than the panel or centers differ.
/// Every consumer uses the resulting polygon, never the two source shapes.
#[derive(Clone,Copy,Debug,PartialEq)]
pub(crate) struct JoinSpec {
    pub resident:Rect,
    pub panel:Rect,
    pub resident_radius:f32,
    pub panel_radius:f32,
    pub resident_flare:f32,
    pub progress:f32,
    pub room:Rect,
    pub camera:Option<Rect>,
}

impl JoinSpec {
    fn blend(self)->f32 {
        let gap=(self.panel.top()-self.resident.bottom()).max(self.resident.top()-self.panel.bottom()).max(0.0);
        let t=(self.progress/0.2).clamp(0.0,1.0);
        // k > twice the gap keeps a neck throughout both animation directions.
        // k tends to zero inside the resident, so the first/last frame is stable.
        (gap*2.0+8.0)*t*t*(3.0-2.0*t)
    }
    fn distance(self,p:Pos2)->f32 {
        let round=|rect:Rect,r:f32,top_round:bool|{
            let r=if !top_round&&p.y<rect.center().y{0.0}else{r.max(0.0).min(rect.width().min(rect.height())*0.5)};
            let q=(p-rect.center()).abs()-rect.size()*0.5+Vec2::splat(r);
            q.max(Vec2::ZERO).length()+q.x.max(q.y).min(0.0)-r
        };
        let a=round(self.resident,self.resident_radius,self.resident_flare<=0.0);
        let b=round(self.panel,self.panel_radius,true);
        let k=self.blend();
        if k<=0.00001{return a.min(b);}
        let h=(1.0-(a-b).abs()/k).max(0.0);
        a.min(b)-h*h*k*0.25
    }
}

#[derive(Debug)]
pub(crate) struct JoinedSurface {
    pub outline:Vec<Pos2>,
    pub bounds:Rect,
    rows:Vec<(f32,f32,f32)>,
    normals:Vec<Vec2>,
}

impl JoinedSurface {
    pub fn new(spec:JoinSpec)->Self {
        let room=spec.room;
        let allowance=spec.blend()*0.25+0.01;
        let bounds=spec.resident.union(spec.panel).expand(allowance).intersect(room);
        let overlap=Rect::from_min_max(
            Pos2::new(spec.resident.left().max(spec.panel.left()),0.0),
            Pos2::new(spec.resident.right().min(spec.panel.right()),1.0));
        let seeds=[spec.resident.center().x,spec.panel.center().x,overlap.center().x];
        let sample=|y:f32|{
            seeds.into_iter().map(|x|(x,spec.distance(Pos2::new(x,y))))
                .min_by(|a,b|a.1.total_cmp(&b.1)).unwrap()
        };
        let outer_y=|mut outside:f32,mut inside:f32|{
            for _ in 0..20 {
                let mid=(outside+inside)*0.5;
                if sample(mid).1>0.0{outside=mid;}else{inside=mid;}
            }
            inside
        };
        let top=outer_y(bounds.top(),spec.resident.center().y.min(spec.panel.center().y).max(bounds.top()));
        let bottom=outer_y(bounds.bottom(),spec.resident.center().y.max(spec.panel.center().y).min(bounds.bottom()));
        // Sample below a point apart, then simplify both sides together with
        // a 0.045pt chord-error bound. Flat sides stay two points; glass and
        // Fluid use exactly the same rows as the native silhouette.
        let steps=((bottom-top)/0.75).ceil().max(1.0) as usize;
        let mut heights:Vec<_>=(0..=steps).map(|i|top+(bottom-top)*i as f32/steps as f32).collect();
        for rect in [spec.resident,spec.panel] {
            let r=if rect==spec.resident{spec.resident_radius}else{spec.panel_radius}.min(rect.width().min(rect.height())*0.5).max(0.0);
            for i in 0..=24 {
                let y=r*(1.0-(i as f32/24.0*std::f32::consts::FRAC_PI_2).cos());
                heights.extend([rect.top()+y,rect.bottom()-y]);
            }
        }
        if let Some(camera)=spec.camera {
            // Preserve the physical exclusion boundary without a diagonal
            // triangle bridging across its bottom corners.
            heights.extend([camera.top(),camera.bottom()]);
        }
        heights.retain(|&y|y>=top&&y<=bottom);
        heights.sort_by(f32::total_cmp);heights.dedup_by(|a,b|(*a-*b).abs()<0.0001);
        let mut rows=Vec::with_capacity(heights.len());
        let mut protected_rows=Vec::new();
        for y in heights {
            let (seed,d)=sample(y);
            if d>0.0001{continue;}
            let edge=|mut outside:f32,mut inside:f32|{
                for _ in 0..19 {
                    let mid=(outside+inside)*0.5;
                    if spec.distance(Pos2::new(mid,y))>0.0{outside=mid;}else{inside=mid;}
                }
                inside
            };
            let mut left=edge(bounds.left(),seed).max(room.left());
            let mut right=edge(bounds.right(),seed).min(room.right());
            // Keep the incumbent screen shoulder for a camera/notch resident.
            if spec.resident_flare>0.0&&y>=spec.resident.top()&&y<=spec.resident.top()+8.0 {
                let wing=spec.resident_flare*(1.0-(y-spec.resident.top())/8.0).powf(2.4);
                left=left.min(spec.resident.left()-wing).max(room.left());
                right=right.max(spec.resident.right()+wing).min(room.right());
            }
            if let Some(camera)=spec.camera.filter(|camera|y>=camera.top()&&y<camera.bottom()) {
                if spec.resident.right()<=camera.left()+0.001{right=right.min(camera.left());}
                else if spec.resident.left()>=camera.right()-0.001{left=left.max(camera.right());}
            }
            if let Some(camera)=spec.camera.filter(|camera|y==camera.bottom()) {
                let before=if spec.resident.right()<=camera.left()+0.001{Some((y,left,right.min(camera.left())))}
                    else if spec.resident.left()>=camera.right()-0.001{Some((y,left.max(camera.right()),right))}else{None};
                if let Some(before)=before.filter(|row|row.2>=row.1){
                    // Two extents at the exact camera bottom make a horizontal
                    // boundary. A tiny diagonal would paint/hit the housing.
                    protected_rows.extend([rows.len(),rows.len()+1]);rows.push(before);
                }
            }
            if right>=left{rows.push((y,left,right));}
        }
        let rows=simplify_join_rows(&rows,0.045,&protected_rows);
        let mut outline=outline_rows(&rows);
        // Consecutive duplicates at the very top/bottom of a circular cap
        // otherwise give its feather and inset rim an undefined normal.
        outline.dedup_by(|a,b|a.distance_sq(*b)<0.0000001);
        if outline.len()>2&&outline[0].distance_sq(*outline.last().unwrap())<0.0000001{outline.pop();}
        let normals=closed_normals(&outline);
        let bounds=Rect::from_points(&outline);
        Self{outline,bounds,rows,normals}
    }

    /// Fill a concave, y-monotone surface as nonoverlapping trapezoids. Each
    /// interior pixel receives the material once, including the entire neck.
    pub fn mesh(&self,feather:f32,shade:&dyn Fn(f32,f32)->Color32,fluid:bool)->egui::Mesh {
        let rows=if fluid{densify(self.rows.clone())}else{self.rows.clone()};
        let cols=if fluid{((self.bounds.width()/COL_STEP).ceil() as usize).clamp(COLS_MIN,COLS_MAX)}else{1};
        let mut mesh=egui::Mesh::default();
        mesh.vertices.reserve(rows.len()*cols*4+self.outline.len()*4);
        mesh.indices.reserve(rows.len()*cols*6+self.outline.len()*6);
        for rows in rows.windows(2) {
            let (ya,la,ra)=rows[0];let (yb,lb,rb)=rows[1];
            for col in 0..cols {
                let a=col as f32/cols as f32;let b=(col+1) as f32/cols as f32;
                let points=[Pos2::new(la+(ra-la)*a,ya),Pos2::new(la+(ra-la)*b,ya),Pos2::new(lb+(rb-lb)*b,yb),Pos2::new(lb+(rb-lb)*a,yb)];
                let base=mesh.vertices.len() as u32;
                for p in points{mesh.colored_vertex(p,shade(p.x,p.y));}
                mesh.add_triangle(base,base+1,base+2);mesh.add_triangle(base,base+2,base+3);
            }
        }
        if feather>0.0 {
            for i in 0..self.outline.len() {
                let j=(i+1)%self.outline.len();let (a,b)=(self.outline[i],self.outline[j]);
                let base=mesh.vertices.len() as u32;
                mesh.colored_vertex(a,shade(a.x,a.y));mesh.colored_vertex(b,shade(b.x,b.y));
                mesh.colored_vertex(b+self.normals[j]*feather,Color32::TRANSPARENT);
                mesh.colored_vertex(a+self.normals[i]*feather,Color32::TRANSPARENT);
                mesh.add_triangle(base,base+1,base+2);mesh.add_triangle(base,base+2,base+3);
            }
        }
        mesh
    }

    pub fn inset_outline(&self,amount:f32)->Vec<Pos2>{
        self.outline.iter().zip(&self.normals).map(|(&p,&normal)|p-normal*amount).collect()
    }

    /// A single offset shadow follows the entire surface. The transparent
    /// outer ring shades only outside it, so translucent glass has no dark
    /// overlap band beneath the join or a stack of internal drop shadows.
    pub fn shadow(&self,painter:&egui::Painter,strength:f32,feather:f32) {
        if strength<=0.01{return;}
        let mut mesh=egui::Mesh::default();
        let offset=Vec2::new(0.0,3.0);
        for (inner,outer,alpha_a,alpha_b) in [(0.0,2.0,0.20,0.15),(2.0,5.0,0.15,0.065),(5.0,10.0,0.065,0.012),(10.0,14.0,0.012,0.0)] {
            let color=|alpha:f32|Color32::from_black_alpha((255.0*alpha*strength.min(1.5)).round() as u8);
            for i in 0..self.outline.len() {
                let j=(i+1)%self.outline.len();let base=mesh.vertices.len() as u32;
                for (index,grow,c) in [(i,inner,color(alpha_a)),(j,inner,color(alpha_a)),(j,outer,color(alpha_b)),(i,outer,color(alpha_b))] {
                    let normal=self.normals[index];
                    // Downward light, with the upper edge held close to the
                    // body. Avoid showing the shadow through the glass fill.
                    let lift=normal.y.max(0.0);
                    let p=self.outline[index]+normal*(grow+feather)+offset*lift;
                    mesh.colored_vertex(p,c);
                }
                mesh.add_triangle(base,base+1,base+2);mesh.add_triangle(base,base+2,base+3);
            }
        }
        painter.add(egui::Shape::mesh(mesh));
    }
}

fn closed_normals(points:&[Pos2])->Vec<Vec2>{
    let n=points.len();
    (0..n).map(|i|{
        let normal=|a:Pos2,b:Pos2|{let d=(b-a).normalized();Vec2::new(d.y,-d.x)};
        let a=normal(points[(i+n-1)%n],points[i]);let b=normal(points[i],points[(i+1)%n]);
        let joined=(a+b)/(1.0+a.dot(b)).max(0.25);
        joined*(3.0/joined.length().max(3.0))
    }).collect()
}

fn simplify_join_rows(rows:&[(f32,f32,f32)],error:f32,protected:&[usize])->Vec<(f32,f32,f32)>{
    fn mark(rows:&[(f32,f32,f32)],first:usize,last:usize,error:f32,keep:&mut[bool]){
        if last<=first+1{return;}
        let (y0,l0,r0)=rows[first];let (y1,l1,r1)=rows[last];let dy=y1-y0;
        let mut far=0.0;let mut split=first;
        for (index,&(y,l,r)) in rows.iter().enumerate().take(last).skip(first+1) {
            let t=(y-y0)/dy.max(0.000001);
            let dl=(l-l0-(l1-l0)*t).abs()*dy/(dy*dy+(l1-l0).powi(2)).sqrt().max(0.000001);
            let dr=(r-r0-(r1-r0)*t).abs()*dy/(dy*dy+(r1-r0).powi(2)).sqrt().max(0.000001);
            let distance=dl.max(dr);
            if distance>far{far=distance;split=index;}
        }
        if far>error{keep[split]=true;mark(rows,first,split,error,keep);mark(rows,split,last,error,keep);}
    }
    if rows.len()<3{return rows.to_vec();}
    let mut keep=vec![false;rows.len()];keep[0]=true;keep[rows.len()-1]=true;
    let mut anchors=vec![0];anchors.extend(protected.iter().copied().filter(|&index|index<rows.len()));anchors.push(rows.len()-1);
    for pair in anchors.windows(2){keep[pair[0]]=true;keep[pair[1]]=true;mark(rows,pair[0],pair[1],error,&mut keep);}
    rows.iter().zip(keep).filter_map(|(&row,keep)|keep.then_some(row)).collect()
}

#[cfg(test)]
mod joined_tests {
    use super::*;
    use egui::{pos2,vec2};

    fn spec(width:f32,progress:f32,position:[f32;2],scale:f32)->JoinSpec{
        let room=Rect::from_min_size(pos2(-240.0,-90.0),vec2(1512.0,922.0)/scale);
        let resident=crate::island_position::body(room,crate::island_position::anchor(position,room),Dock::Top,vec2(width,30.0));
        let panel_size=vec2(419.0,286.0);
        let width=width.min(panel_size.x)+(panel_size.x-width.min(panel_size.x))*crate::anim::smoothstep(0.0,0.85,progress);
        let panel=crate::island_position::disclosure(room,resident,panel_size,vec2(width,panel_size.y*progress),None);
        JoinSpec{resident,panel,resident_radius:15.0,panel_radius:(panel.width().min(panel.height())*0.5).min(15.0+11.0*progress),resident_flare:0.0,progress,room,camera:None}
    }

    #[test]
    fn fluid_join_keeps_a_connected_neck_in_both_lanes_at_all_lengths_and_scales(){
        for scale in [0.8,1.4,2.0]{for width in [80.0,520.0]{for position in [[0.0,0.0],[0.5,0.0],[1.0,0.98]]{
            for progress in [0.0031,0.02,0.1,0.32,0.65,0.9,1.0]{
                let spec=spec(width,progress,position,scale);let surface=JoinedSurface::new(spec);
                assert!(surface.outline.len()>=4&&surface.outline.iter().all(|p|p.is_finite()&&spec.room.expand(0.001).contains(*p)),"{spec:?}");
                assert!(surface.rows.windows(2).all(|rows|rows[1].0>rows[0].0));
                assert!(surface.rows.iter().all(|&(_,left,right)|right>left));
                assert!(contains_outline(&surface.outline,spec.resident.center()));
                assert!(contains_outline(&surface.outline,spec.panel.center()));
                let gap=(spec.panel.top()-spec.resident.bottom()).max(spec.resident.top()-spec.panel.bottom());
                if gap>0.0{
                    let x=(spec.resident.left().max(spec.panel.left())+spec.resident.right().min(spec.panel.right()))*0.5;
                    let (a,b)=if spec.panel.top()>spec.resident.bottom(){(spec.resident.bottom(),spec.panel.top())}else{(spec.panel.bottom(),spec.resident.top())};
                    for i in 0..=20{assert!(contains_outline(&surface.outline,pos2(x,a+(b-a)*i as f32/20.0)),"neck disconnected: {spec:?}");}
                }
            }
        }}}
    }

    #[test]
    fn fluid_join_mesh_covers_the_concave_outline_once_without_filling_its_corners(){
        for width in [80.0,520.0]{for progress in [0.32,1.0]{
            let spec=spec(width,progress,[0.08,0.0],1.0);let surface=JoinedSurface::new(spec);
            let mesh=surface.mesh(0.0,&|_,_|Color32::from_black_alpha(160),false);
            let cross=|a:Pos2,b:Pos2,p:Pos2|(b.x-a.x)*(p.y-a.y)-(b.y-a.y)*(p.x-a.x);
            for y in (surface.bounds.top().floor() as i32..surface.bounds.bottom().ceil() as i32).step_by(5){
                for x in (surface.bounds.left().floor() as i32-2..surface.bounds.right().ceil() as i32+2).step_by(5){
                    let point=pos2(x as f32+0.314,y as f32+0.271);
                    let covers=mesh.indices.chunks_exact(3).filter(|indices|{
                        let [a,b,c]=[mesh.vertices[indices[0] as usize].pos,mesh.vertices[indices[1] as usize].pos,mesh.vertices[indices[2] as usize].pos];
                        cross(a,b,point)>0.0&&cross(b,c,point)>0.0&&cross(c,a,point)>0.0
                    }).count();
                    assert_eq!(covers,usize::from(contains_outline(&surface.outline,point)),"concave fill must have neither alpha overlap nor holes at {point:?}: {spec:?}");
                }
            }
        }}
    }

    #[test]
    fn fluid_join_shoulders_have_no_tangent_cusps_and_reflect_for_upward_disclosure(){
        for width in [80.0,520.0]{
            let down=spec(width,1.0,[0.5,0.4],1.0);let a=JoinedSurface::new(down);
            let mirror=|r:Rect|Rect::from_min_max(pos2(r.left(),-r.bottom()),pos2(r.right(),-r.top()));
            let b=JoinedSurface::new(JoinSpec{resident:mirror(down.resident),panel:mirror(down.panel),room:mirror(down.room),..down});
            assert!((a.bounds.width()-b.bounds.width()).abs()<0.001);
            for point in &a.outline{
                let point=pos2(point.x,-point.y);
                let distance=b.outline.iter().zip(b.outline.iter().cycle().skip(1)).take(b.outline.len()).map(|(&a,&b)|{
                    let d=b-a;point.distance(a+d*((point-a).dot(d)/d.length_sq()).clamp(0.0,1.0))
                }).fold(f32::INFINITY,f32::min);
                assert!(distance<0.1,"upward shoulder differs by {distance}pt");
            }
            let n=a.outline.len();
            for i in 0..n{
                let p=a.outline[i];
                if p.y<down.resident.bottom()-6.0||p.y>down.panel.top()+6.0{continue;}
                let before=(p-a.outline[(i+n-1)%n]).normalized();let after=(a.outline[(i+1)%n]-p).normalized();
                assert!(before.dot(after)>0.85,"shoulder has a sharp turn at {p:?}, {width}: {before:?}, {after:?}");
            }
        }
    }

    #[test]
    fn fluid_join_excludes_the_physical_camera_throughout_reveal(){
        let room=Rect::from_min_size(Pos2::ZERO,vec2(1512.0,922.0));
        let camera=Rect::from_min_size(pos2(666.0,0.0),vec2(180.0,38.0));
        for position in [[0.405,0.0],[0.5,0.0],[0.595,0.0]]{for progress in [0.01,0.1,0.4,0.65,1.0]{
            let (_,plan)=crate::island_position::plan_with_camera(room.size(),position,Dock::Top,vec2(80.0,30.0),vec2(419.0,286.0),1.0,Some(camera));
            let resident=plan.resident_body(Dock::Top);
            let panel=plan.panel_body(Dock::Top,vec2(80.0+339.0*crate::anim::smoothstep(0.0,0.85,progress),286.0*progress));
            let surface=JoinedSurface::new(JoinSpec{resident,panel,resident_radius:15.0,panel_radius:(panel.height()*0.5).min(26.0),resident_flare:0.0,progress,room:plan.room,camera:plan.camera});
            let camera=plan.camera.unwrap().shrink(0.0001);
            let n=surface.outline.len();
            for i in 0..n{for t in [0.0,0.2,0.5,0.8,1.0]{
                let p=surface.outline[i]+(surface.outline[(i+1)%n]-surface.outline[i])*t;
                assert!(!camera.contains(p),"paint/input/blur must exclude the hardware camera at {p:?}; {position:?}, {progress}");
            }}
            for y in 1..38{for x in (667..846).step_by(7){
                let p=pos2(x as f32,y as f32)+plan.room.min.to_vec2();
                assert!(!contains_outline(&surface.outline,p));
            }}
        }}
    }

    #[test]
    #[ignore="Measured debug geometry generation, no timing threshold"]
    fn fluid_join_generation_timing(){
        for width in [80.0,520.0]{
            let started=std::time::Instant::now();let mut count=0;
            for i in 1..=120{
                let surface=JoinedSurface::new(spec(width,i as f32/120.0,[0.04,0.0],1.0));
                count+=std::hint::black_box(surface).outline.len();
            }
            eprintln!("fluid join {width}pt resident / 419x286 panel: {:.3}ms per geometry (120 changing samples, debug); {:.1} outline vertices average",started.elapsed().as_secs_f64()*1000.0/120.0,count as f32/120.0);
        }
    }
}

/// 贴着 `dock` 边缘的抗锯齿填充。
#[allow(clippy::too_many_arguments)]
pub fn fill_docked(painter: &egui::Painter, rect: Rect, dock: Dock, flare: f32, flare_h: f32, radius: f32, color: Color32, feather: f32) {
    let (map, frame) = EdgeMap::of(rect, dock);
    fill_mapped(painter, frame, map, flare, flare_h, radius, color, color, frame.height(), feather);
}

/// 从贴边处向外的渐变（毛玻璃柔光）。
#[allow(clippy::too_many_arguments)]
pub fn fill_gradient_docked(painter: &egui::Painter, rect: Rect, dock: Dock, flare: f32, flare_h: f32, radius: f32, top: Color32, bottom: Color32, span: f32, feather: f32) {
    let (map, frame) = EdgeMap::of(rect, dock);
    fill_mapped(painter, frame, map, flare, flare_h, radius, top, bottom, span, feather);
}

/// 贴着 `dock` 边缘的柔和落影。
pub fn shadow_docked(painter: &egui::Painter, rect: Rect, dock: Dock, flare: f32, flare_h: f32, radius: f32, strength: f32) {
    let (map, frame) = EdgeMap::of(rect, dock);
    shadow_mapped(painter, frame, map, flare, flare_h, radius, strength);
}

/// (y, 左 x, 右 x) 轮廓采样行。
pub fn sections(rect: Rect, flare: f32, flare_h: f32, radius: f32) -> Vec<(f32, f32, f32)> {
    let (x0, y0, x1, y1) = (rect.left(), rect.top(), rect.right(), rect.bottom());
    let r = radius.min(rect.height() * 0.5).min(rect.width() * 0.5).max(0.0);
    let fh = flare_h.min(rect.height() * 0.5).max(0.5);
    let mut rows = Vec::with_capacity(48);
    const N_FLARE: usize = 14;
    for i in 0..=N_FLARE {
        let t = i as f32 / N_FLARE as f32;
        // 肩部曲线：(1-t)^2.4 比三次方更饱满，过渡更柔。
        let wing = flare * (1.0 - t).powf(2.4);
        rows.push((y0 + fh * t, x0 - wing, x1 + wing));
    }
    if y1 - r > y0 + fh {
        rows.push((y1 - r, x0, x1));
    }
    const N_CORNER: usize = 20;
    for i in 1..=N_CORNER {
        let a = i as f32 / N_CORNER as f32 * std::f32::consts::FRAC_PI_2;
        rows.push((y1 - r + r * a.sin(), x0 + r - r * a.cos(), x1 - r + r * a.cos()));
    }
    rows
}

/// 闭合外轮廓（顺时针：右侧自上而下，再左侧自下而上）。
pub fn outline(rect: Rect, flare: f32, flare_h: f32, radius: f32) -> Vec<Pos2> {
    let rows = sections(rect, flare, flare_h, radius);
    outline_rows(&rows)
}

// 顶边不参与羽化；端点只取肩部切线，避免整条顶边扭曲法线。
fn edge_normal(pts: &[Pos2], i: usize) -> Vec2 {
    let normal = |a: Pos2, b: Pos2| {
        let d = (b - a).normalized();
        Vec2::new(d.y, -d.x)
    };
    if i == 0 {
        return normal(pts[0], pts[1]);
    }
    if i == pts.len() - 1 {
        return normal(pts[i - 1], pts[i]);
    }
    let prev = normal(pts[i - 1], pts[i]);
    let next = normal(pts[i], pts[i + 1]);
    // 相邻单位法线的交点补偿，保持两段的垂直羽化宽度一致。
    (prev + next) / (1.0 + prev.dot(next))
}

/// 抗锯齿填充。`feather` 是羽化宽度（逻辑点，通常 = 1 / ppp）。
pub fn fill(painter: &egui::Painter, rect: Rect, flare: f32, flare_h: f32, radius: f32, color: Color32, feather: f32) {
    fill_gradient(painter, rect, flare, flare_h, radius, color, color, rect.height(), feather);
}

/// 自上而下的渐变填充：顶边为 `top`，向下 `span` 逻辑点处过渡到 `bottom`，此后保持 `bottom`。
/// 毛玻璃皮肤用它画顶部柔光。
#[allow(clippy::too_many_arguments)]
pub fn fill_gradient(painter: &egui::Painter, rect: Rect, flare: f32, flare_h: f32, radius: f32, top: Color32, bottom: Color32, span: f32, feather: f32) {
    fill_mapped(painter, rect, EdgeMap::TOP, flare, flare_h, radius, top, bottom, span, feather);
}

#[allow(clippy::too_many_arguments)]
fn fill_mapped(painter: &egui::Painter, rect: Rect, map: EdgeMap, flare: f32, flare_h: f32, radius: f32, top: Color32, bottom: Color32, span: f32, feather: f32) {
    let span=if top==bottom {0.0} else {span};
    let key=([rect.min.x,rect.min.y,rect.max.x,rect.max.y,flare,flare_h,radius,feather,span,map.code(),map.edge].map(f32::to_bits),top,bottom);
    let mesh=painter.ctx().memory_mut(|memory|memory.caches.cache::<FillCache>().get(key));
    painter.add(egui::Shape::Mesh(mesh));
}

type FillKey=([u32;11],Color32,Color32);
type FillCache=egui::cache::FrameCache<std::sync::Arc<egui::Mesh>,FillComputer>;
#[derive(Default)]
struct FillComputer;
impl egui::cache::ComputerMut<FillKey,std::sync::Arc<egui::Mesh>> for FillComputer {
    fn compute(&mut self,(bits,top,bottom):FillKey)->std::sync::Arc<egui::Mesh>{
        let [x0,y0,x1,y1,flare,flare_h,radius,feather,span,dock,edge]=bits.map(f32::from_bits);
        let rect=Rect::from_min_max(Pos2::new(x0,y0),Pos2::new(x1,y1));
        let rows=if dock!=0.0{side_sections(rect,flare,flare_h,radius)}else{sections(rect,flare,flare_h,radius)};
        let mut mesh=gradient_mesh_rows(rect,rows,top,bottom,span,feather);
        if dock!=0.0 {
            let map=EdgeMap{dock:if dock==1.0{Dock::Left}else{Dock::Right},edge};
            for v in &mut mesh.vertices {v.pos=map.apply(v.pos);}
        }
        std::sync::Arc::new(mesh)
    }
}

/// 预乘颜色的线性插值。
fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let ch = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_premultiplied(ch(a.r(), b.r()), ch(a.g(), b.g()), ch(a.b(), b.b()), ch(a.a(), b.a()))
}

// 仅保留当前帧仍使用的轮廓；尺寸、透明度或 DPI 改变时重新生成，动画不复用旧形状。
#[cfg(test)]
fn fill_mesh(rect:Rect,flare:f32,flare_h:f32,radius:f32,color:Color32,feather:f32)->egui::Mesh {
    gradient_mesh(rect,flare,flare_h,radius,color,color,0.0,feather)
}

#[allow(clippy::too_many_arguments)]
fn gradient_mesh(rect:Rect,flare:f32,flare_h:f32,radius:f32,top:Color32,bottom:Color32,span:f32,feather:f32)->egui::Mesh {
    gradient_mesh_rows(rect,sections(rect,flare,flare_h,radius),top,bottom,span,feather)
}

fn gradient_mesh_rows(rect:Rect,rows:Vec<(f32,f32,f32)>,top:Color32,bottom:Color32,span:f32,feather:f32)->egui::Mesh{
    let at=|y:f32| if span<=0.0 {top} else {lerp_color(top,bottom,(y-rect.top())/span)};
    let mut mesh = egui::Mesh::default();
    mesh.vertices.reserve((rows.len()*3-2)*4);
    mesh.indices.reserve((rows.len()*3-2)*6);
    // 1) 实心梯形条
    for w in rows.windows(2) {
        let (ya, la, ra) = w[0];
        let (yb, lb, rb) = w[1];
        let i = mesh.vertices.len() as u32;
        mesh.colored_vertex(Pos2::new(la, ya), at(ya));
        mesh.colored_vertex(Pos2::new(ra, ya), at(ya));
        mesh.colored_vertex(Pos2::new(rb, yb), at(yb));
        mesh.colored_vertex(Pos2::new(lb, yb), at(yb));
        mesh.add_triangle(i, i + 1, i + 2);
        mesh.add_triangle(i, i + 2, i + 3);
    }
    // 2) 外轮廓羽化带（不含顶边：贴着屏幕顶）
    let pts: Vec<Pos2> = rows
        .iter()
        .map(|&(y, _, r)| Pos2::new(r, y))
        .chain(rows.iter().rev().map(|&(y, l, _)| Pos2::new(l, y)))
        .collect();
    let n = pts.len();
    let clear = Color32::TRANSPARENT;
    for i in 0..n - 1 {
        let (a, b) = (pts[i], pts[i + 1]);
        // 顶边两端（右上角 → 左上角那条闭合边）不羽化
        if a.y <= rect.top() + 0.01 && b.y <= rect.top() + 0.01 {
            continue;
        }
        let (na, nb) = (edge_normal(&pts, i), edge_normal(&pts, i + 1));
        let base = mesh.vertices.len() as u32;
        mesh.colored_vertex(a, at(a.y));
        mesh.colored_vertex(b, at(b.y));
        mesh.colored_vertex(b + nb * feather, clear);
        mesh.colored_vertex(a + na * feather, clear);
        mesh.add_triangle(base, base + 1, base + 2);
        mesh.add_triangle(base, base + 2, base + 3);
    }
    mesh
}

/// 柔和的落影：多层半透明轮廓叠加，近似高斯模糊（不依赖 GPU 模糊）。
pub fn shadow(painter: &egui::Painter, rect: Rect, flare: f32, flare_h: f32, radius: f32, strength: f32) {
    shadow_mapped(painter, rect, EdgeMap::TOP, flare, flare_h, radius, strength);
}

fn shadow_mapped(painter: &egui::Painter, rect: Rect, map: EdgeMap, flare: f32, flare_h: f32, radius: f32, strength: f32) {
    if strength <= 0.01 {
        return;
    }
    for (grow, a) in [(10.0f32, 0.035f32), (6.5, 0.05), (3.5, 0.07), (1.5, 0.09)] {
        let r = Rect::from_min_max(
            Pos2::new(rect.left() - grow * 0.6, rect.top()),
            Pos2::new(rect.right() + grow * 0.6, rect.bottom() + grow),
        );
        let c = Color32::from_black_alpha((255.0 * a * strength) as u8);
        fill_mapped(painter, r, map, flare, flare_h, radius + grow * 0.8, c, c, r.height(), grow * 0.9);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inside(points: &[Pos2], p: Pos2) -> bool {
        let mut inside = false;
        let mut prev = *points.last().unwrap();
        for &next in points {
            if (next.y > p.y) != (prev.y > p.y)
                && p.x < (prev.x - next.x) * (p.y - next.y) / (prev.y - next.y) + next.x
            {
                inside = !inside;
            }
            prev = next;
        }
        inside
    }

    #[test]
    fn shoulder_feather_keeps_one_pixel_perpendicular_to_edge() {
        for (width, height, flare, flare_h, radius) in [
            (143.0, 25.0, 5.0, 8.0, 12.5),
            (419.0, 140.0, 17.6, 16.0, 33.0),
        ] {
            let pts = outline(Rect::from_min_size(Pos2::ZERO, Vec2::new(width, height)), flare, flare_h, radius);
            for ppp in [1.0, 1.25, 1.5, 2.0] {
                for i in 0..pts.len() - 1 {
                    let d = (pts[i + 1] - pts[i]).normalized();
                    let normal = Vec2::new(d.y, -d.x);
                    for j in [i, i + 1] {
                        let offset = edge_normal(&pts, j) / ppp;
                        assert!((offset.dot(normal) * ppp - 1.0).abs() < 0.001,
                            "羽化必须沿每段轮廓保持一物理像素：边 {i}，顶点 {j}，缩放 {ppp}");
                    }
                }
            }
        }
    }

    #[test]
    fn wings_extend_outside_body_only_at_top() {
        let rect = Rect::from_min_size(Pos2::new(110.0, 0.0), Vec2::new(419.0, 140.0));
        let pts = outline(rect, 17.6, 16.0, 33.0);
        assert!(pts.iter().any(|p| p.x < rect.left() - 17.0));
        assert!(inside(&pts, Pos2::new(rect.left() + 0.2, 70.0)));
        assert!(!inside(&pts, Pos2::new(rect.left() - 0.5, 70.0)));
        // 底部圆角外侧是空的
        assert!(!inside(&pts, Pos2::new(rect.left() + 2.0, rect.bottom() - 2.0)));
    }
}

#[cfg(test)]
mod dock_tests {
    use super::*;
    #[test]
    fn side_outlines_hug_the_screen_edge_with_the_wings_along_it() {
        let rect = Rect::from_min_size(Pos2::new(0.0, 100.0), Vec2::new(419.0, 140.0));
        let left = outline_docked(rect, Dock::Left, 17.6, 16.0, 33.0);
        let b = Rect::from_points(&left);
        assert!((b.left() - 0.0).abs() < 0.01 && (b.right() - 419.0).abs() < 0.01, "{b:?}");
        assert!(b.top() < 100.0 - 17.0 && b.bottom() > 240.0 + 17.0, "肩部沿屏幕边缘上下展开：{b:?}");
        // 肩部只在贴边处
        assert!(left.iter().filter(|p| p.y < 99.0).all(|p| p.x < 16.5));
        let right_rect = Rect::from_min_size(Pos2::new(81.0, 100.0), Vec2::new(419.0, 140.0));
        let right = outline_docked(right_rect, Dock::Right, 17.6, 16.0, 33.0);
        let b = Rect::from_points(&right);
        assert!((b.right() - 500.0).abs() < 0.01 && (b.left() - 81.0).abs() < 0.01, "{b:?}");
        assert!(right.iter().filter(|p| p.y < 99.0).all(|p| p.x > 500.0 - 16.5));
        assert_eq!(outline_docked(rect, Dock::Top, 5.0, 8.0, 12.0), outline(rect, 5.0, 8.0, 12.0));
    }

    #[test]
    fn side_shoulders_join_the_screen_and_panel_with_matching_tangents(){
        let rect=Rect::from_min_size(Pos2::new(34.0,0.0),Vec2::new(532.0,292.0));
        let rows=side_sections(rect,22.0,22.0,26.0);
        let first=Vec2::new(rows[1].1-rows[0].1,rows[1].0-rows[0].0);
        assert!(first.y/first.x.abs()<0.04,"screen join must be tangent, not diagonal: {first:?}");
        let end=Vec2::new(rows[24].1-rows[23].1,rows[24].0-rows[23].0);
        assert!(end.x.abs()/end.y<0.04,"shoulder must enter the panel's horizontal top with a matching tangent: {end:?}");
        for t in [0.0,0.05,0.25,0.5,0.75,1.0]{
            let w=6.0+(292.0-6.0)*t;let h=52.0+(532.0-52.0)*t;
            let (flare,depth,radius)=crate::side_dock::shape_parameters(w,h,t);
            let left_rect=Rect::from_min_size(Pos2::new(0.0,300.0-h/2.0),Vec2::new(w,h));
            let right_rect=Rect::from_min_size(Pos2::new(500.0-w,300.0-h/2.0),Vec2::new(w,h));
            let left=outline_docked(left_rect,Dock::Left,flare,depth,radius);let right=outline_docked(right_rect,Dock::Right,flare,depth,radius);
            assert!(left.iter().chain(&right).all(|p|p.is_finite()&&p.y>=0.0&&p.y<=600.0));
            for (a,b) in left.iter().zip(&right){assert!((a.x-(500.0-b.x)).abs()<0.001&&(a.y-b.y).abs()<0.001);}
            assert!(contains_outline(&left,Pos2::new(0.01,300.0)));assert!(!contains_outline(&left,Pos2::new(w+1.0,300.0)));
        }
    }
}


#[cfg(test)]
mod cache_tests {
    use super::*;
    fn frame(ctx:&egui::Context,width:f32,feather:f32)->std::sync::Arc<egui::Mesh>{
        let out=ctx.run(egui::RawInput::default(),|ctx|{
            fill(&ctx.layer_painter(egui::LayerId::background()),Rect::from_min_size(Pos2::ZERO,Vec2::new(width,30.0)),5.0,8.0,12.5,Color32::BLACK,feather);
        });
        out.shapes.into_iter().find_map(|s|if let egui::Shape::Mesh(m)=s.shape{Some(m)}else{None}).unwrap()
    }
    #[test]
    fn unchanged_outline_reuses_geometry_across_frames(){
        let ctx=egui::Context::default();let first=frame(&ctx,300.0,1.0);let second=frame(&ctx,300.0,1.0);
        assert!(std::sync::Arc::ptr_eq(&first,&second),"稳定轮廓不能每帧重新分配网格");
        let changed=frame(&ctx,320.0,1.0);assert_ne!(first.vertices,changed.vertices);
        let scaled=frame(&ctx,320.0,0.5);assert_ne!(changed.vertices,scaled.vertices);
    }

    #[test]
    fn animated_outlines_do_not_accumulate_old_meshes(){
        let ctx=egui::Context::default();
        for i in 0..300 {
            let mesh=frame(&ctx,300.0+i as f32/10.0,1.0);
            let expected=fill_mesh(Rect::from_min_size(Pos2::ZERO,Vec2::new(300.0+i as f32/10.0,30.0)),5.0,8.0,12.5,Color32::BLACK,1.0);
            assert_eq!(mesh.vertices,expected.vertices);assert_eq!(mesh.indices,expected.indices);
            let count=ctx.memory_mut(|m|egui::cache::CacheTrait::len(m.caches.cache::<FillCache>()));
            assert!(count<=2,"动画不能累积历史轮廓：{count}");
        }
    }
}

const ROW_STEP: f32 = 6.0;
const COL_STEP: f32 = 9.0;
const COLS_MIN: usize = 10;
const COLS_MAX: usize = 64;

/// 把行表补密：相邻两行之间按需插入等距行，左右边界线性插值。
fn densify(rows: Vec<(f32, f32, f32)>) -> Vec<(f32, f32, f32)> {
    let mut out = Vec::with_capacity(rows.len() * 3);
    for w in rows.windows(2) {
        let (ya, la, ra) = w[0];
        let (yb, lb, rb) = w[1];
        out.push(w[0]);
        let n = ((yb - ya) / ROW_STEP).ceil() as usize;
        for i in 1..n {
            let t = i as f32 / n as f32;
            out.push((ya + (yb - ya) * t, la + (lb - la) * t, ra + (rb - ra) * t));
        }
    }
    if let Some(last) = rows.last() {
        out.push(*last);
    }
    out
}

pub(crate) fn shade_mesh(
    rect: Rect,
    rows: Vec<(f32,f32,f32)>,
    feather: f32,
    shade: &dyn Fn(f32, f32) -> Color32,
) -> egui::Mesh {
    let rows = densify(rows);
    let cols = ((rect.width() / COL_STEP).ceil() as usize).clamp(COLS_MIN, COLS_MAX);
    let mut mesh = egui::Mesh::default();
    mesh.vertices.reserve(rows.len() * cols * 4);
    mesh.indices.reserve(rows.len() * cols * 6);
    // 1) 二维网格：每行横向切 cols 段，四个角各自取色，光斑才不会跨行被抹平
    for w in rows.windows(2) {
        let (ya, la, ra) = w[0];
        let (yb, lb, rb) = w[1];
        for i in 0..cols {
            let t0 = i as f32 / cols as f32;
            let t1 = (i + 1) as f32 / cols as f32;
            let ax0 = la + (ra - la) * t0;
            let ax1 = la + (ra - la) * t1;
            let bx0 = lb + (rb - lb) * t0;
            let bx1 = lb + (rb - lb) * t1;
            let id = mesh.vertices.len() as u32;
            mesh.colored_vertex(Pos2::new(ax0, ya), shade(ax0, ya));
            mesh.colored_vertex(Pos2::new(ax1, ya), shade(ax1, ya));
            mesh.colored_vertex(Pos2::new(bx1, yb), shade(bx1, yb));
            mesh.colored_vertex(Pos2::new(bx0, yb), shade(bx0, yb));
            mesh.add_triangle(id, id + 1, id + 2);
            mesh.add_triangle(id, id + 2, id + 3);
        }
    }
    // 2) 外轮廓羽化带（不含顶边）：外沿保持同色但完全透明
    let pts: Vec<Pos2> = rows
        .iter()
        .map(|&(y, _, r)| Pos2::new(r, y))
        .chain(rows.iter().rev().map(|&(y, l, _)| Pos2::new(l, y)))
        .collect();
    let n = pts.len();
    for i in 0..n - 1 {
        let (a, b) = (pts[i], pts[i + 1]);
        if a.y <= rect.top() + 0.01 && b.y <= rect.top() + 0.01 {
            continue;
        }
        let (na, nb) = (edge_normal(&pts, i), edge_normal(&pts, i + 1));
        let clear = |p: Pos2| {
            let c = shade(p.x, p.y);
            Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), 0)
        };
        let base = mesh.vertices.len() as u32;
        mesh.colored_vertex(a, shade(a.x, a.y));
        mesh.colored_vertex(b, shade(b.x, b.y));
        mesh.colored_vertex(b + nb * feather, clear(b + nb * feather));
        mesh.colored_vertex(a + na * feather, clear(a + na * feather));
        mesh.add_triangle(base, base + 1, base + 2);
        mesh.add_triangle(base, base + 2, base + 3);
    }
    mesh
}

