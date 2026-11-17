//! Apple HIG presentation tokens and original generated Qingyu navigation.
use eframe::egui::{self, Color32, FontId, Rect, Stroke, Vec2};
use std::time::{Duration, Instant};
extern "C" {
    fn qy_services_palette(dark: bool, colors: *mut u32);
    fn qy_services_preferences() -> u32;
}
#[derive(Clone, Copy)]
pub struct Palette {
    pub window: Color32, pub sidebar: Color32, pub group: Color32, pub control: Color32,
    pub text: Color32, pub secondary: Color32, pub separator: Color32, pub accent: Color32, pub selected: Color32,
}
fn rgb(value:u32)->Color32 {Color32::from_rgb((value>>16) as u8,(value>>8) as u8,value as u8)}
pub fn palette(ctx:&egui::Context)->Palette {
    palette_for(ctx,ctx.style().visuals.dark_mode)
}
pub(crate) fn palette_for(ctx:&egui::Context,dark:bool)->Palette {
    let id=egui::Id::new(("macos-native-palette",dark));
    if let Some((at,p))=ctx.data(|d|d.get_temp::<(Instant,Palette)>(id)){if at.elapsed()<Duration::from_secs(1){return p;}}
    let mut colors=[0u32;9];unsafe{qy_services_palette(dark,colors.as_mut_ptr())};
    let mut p=Palette{window:rgb(colors[0]),sidebar:rgb(colors[1]),group:rgb(colors[2]),control:rgb(colors[3]),
        text:rgb(colors[4]),secondary:rgb(colors[5]),separator:rgb(colors[6]),accent:rgb(colors[7]),selected:rgb(colors[8])};
    p.sidebar=crate::paint::mix(p.window,Color32::from_rgb(171,220,221),if dark{0.045}else{0.16});
    p.accent=if dark{Color32::from_rgb(124,215,214)}else{Color32::from_rgb(28,109,118)};
    p.selected=Color32::from_rgb(29,108,118);
    // Native secondary labels can be translucent; egui draws opaque text, so enforce the small-text floor.
    if contrast(p.secondary,p.group)<4.5 {p.secondary=if dark{Color32::from_gray(180)}else{Color32::from_gray(100)};}
    ctx.data_mut(|d|d.insert_temp(id,(Instant::now(),p)));p
}
pub fn preferences(ctx:&egui::Context)->u32 {
    let id=egui::Id::new("macos-accessibility-preferences");
    if let Some((at,v))=ctx.data(|d|d.get_temp::<(Instant,u32)>(id)){if at.elapsed()<Duration::from_secs(1){return v;}}
    let v=unsafe{qy_services_preferences()};ctx.data_mut(|d|d.insert_temp(id,(Instant::now(),v)));v
}
pub fn system_dark(ctx:&egui::Context)->bool {preferences(ctx)&1!=0}
pub fn reduce_motion(ctx:&egui::Context)->bool {preferences(ctx)&2!=0}
pub fn style(ctx:&egui::Context,style:&mut egui::Style) {
    let p=palette_for(ctx,style.visuals.dark_mode);
    style.text_styles.insert(egui::TextStyle::Body,FontId::proportional(13.0));
    style.text_styles.insert(egui::TextStyle::Button,FontId::proportional(13.0));
    style.text_styles.insert(egui::TextStyle::Small,FontId::proportional(11.0));
    style.text_styles.insert(egui::TextStyle::Heading,FontId::proportional(20.0));
    style.spacing.interact_size=egui::vec2(28.0,28.0);
    style.spacing.button_padding=egui::vec2(10.0,5.0);
    style.spacing.item_spacing=egui::vec2(8.0,6.0);
    style.spacing.icon_width=16.0;
    style.visuals.override_text_color=Some(p.text);
    style.visuals.panel_fill=p.window;
    style.visuals.window_fill=p.window;
    style.visuals.extreme_bg_color=p.group;
    style.visuals.selection.bg_fill=p.selected;
    style.visuals.selection.stroke=Stroke::new(1.0,Color32::WHITE);
    style.visuals.window_corner_radius=egui::CornerRadius::same(10);
    style.visuals.hyperlink_color=p.accent;
    style.animation_time=if reduce_motion(ctx){0.0}else{0.12};
    for widget in [&mut style.visuals.widgets.inactive,&mut style.visuals.widgets.hovered,&mut style.visuals.widgets.active] {
        widget.corner_radius=egui::CornerRadius::same(6);
        widget.bg_fill=p.control;widget.weak_bg_fill=p.control;
        widget.bg_stroke=Stroke::new(0.5,p.separator);widget.fg_stroke=Stroke::new(1.0,p.text);
    }
    style.visuals.widgets.noninteractive.bg_stroke=Stroke::new(0.5,p.separator);
    style.visuals.widgets.noninteractive.bg_fill=p.separator;
    style.visuals.widgets.noninteractive.weak_bg_fill=p.separator;
    // egui paints slider rails with inactive.bg_fill; buttons use weak_bg_fill.
    style.visuals.widgets.inactive.bg_fill=p.separator;
    style.visuals.slider_trailing_fill=true;
    style.spacing.slider_rail_height=4.0;
}
pub fn symbol(ctx:&egui::Context,name:&str)->Option<egui::TextureHandle> {crate::theme::glyph(ctx,name)}

pub(crate) fn sidebar(ui:&mut egui::Ui,selected:&mut usize) {
    let p=palette(ui.ctx());
    ui.add_space(12.0);
    ui.horizontal(|ui|{
        ui.set_min_height(44.0);
        ui.add_space(8.0);
        let logo=crate::theme::brand_art(ui.ctx(),false);
        let wordmark=crate::theme::brand_art(ui.ctx(),true);
        ui.add(egui::Image::new((logo.id(),Vec2::new(34.0,28.0))).alt_text("轻屿标志"));
        ui.add(egui::Image::new((wordmark.id(),Vec2::new(60.0,26.7))).tint(p.accent).alt_text("轻屿"));
    });
    ui.add_space(20.0);
    for (i,(label,start,end)) in crate::extras::SETTINGS_GROUPS.into_iter().enumerate() {
        let name=match start {0=>"placement",10=>"pages",1=>"music",5=>"shelf",14=>"clipboard",4=>"link",7=>"mail",12=>"weather",13=>"agent",15=>"mac-stats",16=>"bell",_=>"settings"};
        if i==2 {ui.add_space(10.0);}
        let on=(start..=end).contains(selected);
        let (rect,response)=ui.allocate_exact_size(egui::vec2(ui.available_width(),32.0),egui::Sense::click());
        response.widget_info(||egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel,ui.is_enabled(),on,label));
        if response.clicked(){*selected=start;}
        let fill=if on{p.selected}else if response.hovered(){p.control}else{Color32::TRANSPARENT};
        ui.painter().rect_filled(rect,6.0,fill);
        if response.has_focus(){ui.painter().rect_stroke(rect.shrink(1.0),6.0,Stroke::new(2.0,p.accent),egui::StrokeKind::Inside);}
        let ink=if on{Color32::WHITE}else{p.text};
        if name=="mac-stats"{crate::paint::stats_symbol(ui.painter(),egui::pos2(rect.left()+19.0,rect.center().y),19.0,if on{ink}else{p.accent});}
        else if let Some(icon)=symbol(ui.ctx(),name){ui.painter().image(icon.id(),Rect::from_center_size(egui::pos2(rect.left()+19.0,rect.center().y),Vec2::splat(19.0)),Rect::from_min_max(egui::Pos2::ZERO,egui::pos2(1.0,1.0)),if on{ink}else{p.accent});}
        ui.painter().text(egui::pos2(rect.left()+37.0,rect.center().y),egui::Align2::LEFT_CENTER,label,FontId::proportional(13.0),ink);
        ui.add_space(2.0);
    }
}

fn contrast(a:Color32,b:Color32)->f64 {
    fn luminance(c:Color32)->f64 {let values=[c.r(),c.g(),c.b()].map(|v|{let v=f64::from(v)/255.;if v<=0.04045{v/12.92}else{((v+0.055)/1.055).powf(2.4)}});values[0]*0.2126+values[1]*0.7152+values[2]*0.0722}
    let(a,b)=(luminance(a),luminance(b));(a.max(b)+0.05)/(a.min(b)+0.05)
}
#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "Exports actual light and dark sidebar renders to QINGYU_BRAND_PREVIEW"]
    fn native_brand_sidebar_gallery(){
        let destination=std::path::PathBuf::from(std::env::var_os("QINGYU_BRAND_PREVIEW").expect("preview destination"));
        std::fs::create_dir_all(&destination).unwrap();
        for dark in [false,true]{
            let ctx=eframe::egui::Context::default();
            crate::theme::load_fonts(&ctx);
            ctx.set_theme(if dark{eframe::egui::ThemePreference::Dark}else{eframe::egui::ThemePreference::Light});
            ctx.set_pixels_per_point(2.0);
            let mut raster=crate::preview_raster::Raster::new(360,1240);
            let mut selected=0;
            for frame in 0..4 {
                let output=ctx.run(eframe::egui::RawInput{screen_rect:Some(eframe::egui::Rect::from_min_size(eframe::egui::Pos2::ZERO,eframe::egui::vec2(180.0,620.0))),time:Some(frame as f64/60.0),..Default::default()},|ctx|{
                    let mut style=(*ctx.style()).clone();style.visuals=if dark{eframe::egui::Visuals::dark()}else{eframe::egui::Visuals::light()};super::style(ctx,&mut style);ctx.set_style(style);
                    eframe::egui::CentralPanel::default().frame(eframe::egui::Frame::new().fill(super::palette(ctx).sidebar).inner_margin(eframe::egui::Margin::symmetric(10,12))).show(ctx,|ui|super::sidebar(ui,&mut selected));
                });
                raster.update_textures(&output.textures_delta);
                if frame==3{
                    raster.draw(&ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);
                    let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
                    image::save_buffer(destination.join(if dark{"sidebar-dark.png"}else{"sidebar-light.png"}),&rgba,360,1240,image::ColorType::Rgba8).unwrap();
                }
                raster.free_textures(&output.textures_delta);
            }
        }
    }

    #[test]
    fn native_text_tokens_retain_small_text_contrast_in_both_appearances(){
        let ctx=eframe::egui::Context::default();
        for dark in [false,true]{let p=super::palette_for(&ctx,dark);
            println!("dark={dark}, window={:?}, group={:?}, text={:?}, secondary={:?}, accent={:?}; text {:.2}:1, secondary {:.2}:1",p.window,p.group,p.text,p.secondary,p.accent,super::contrast(p.text,p.group),super::contrast(p.secondary,p.group));
            assert!(super::contrast(p.text,p.group)>=4.5);assert!(super::contrast(p.secondary,p.group)>=4.5);
            assert!(super::contrast(eframe::egui::Color32::WHITE,p.selected)>=4.5);
        }
    }
}
