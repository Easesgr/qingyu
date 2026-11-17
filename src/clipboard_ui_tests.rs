use super::*;
use crate::clipboard::{Clip,Media,MediaKind};

fn seed_clipboard(s:&mut Side)->(std::path::PathBuf,std::path::PathBuf){
    let folder=crate::model::config_dir().join("clipboard-ui-fixtures");std::fs::create_dir_all(&folder).unwrap();
    let picture=folder.join("海边照片.png");
    std::fs::write(&picture,include_bytes!("../assets/qingyu/music-placeholder.png")).unwrap();
    let video=folder.join("旅行视频.mov");std::fs::write(&video,b"fixture-video").unwrap();
    let make=|path:&std::path::Path,kind|{
        let name=path.file_name().unwrap().to_string_lossy().into_owned();
        Clip{text:name.clone(),at:"09:41".into(),pinned:false,media:Some(Media{path:path.into(),name,kind,bytes:std::fs::metadata(path).unwrap().len()})}
    };
    s.h.app.clipboard.lock().items=vec![make(&picture,MediaKind::Image),make(&video,MediaKind::Video),Clip{text:"稍后发送的文字".into(),at:"09:40".into(),pinned:false,media:None}];
    s.h.app.settings.clipboard_history=true;
    (picture,video)
}

#[test]
fn clipboard_is_independent_from_shelf_on_every_dock(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for dock in [Dock::Top,Dock::Left,Dock::Right]{
        let mut s=Side::new(dock,CLIPBOARD_PAGE);seed_clipboard(&mut s);
        assert_eq!(s.h.app.page(),CLIPBOARD_PAGE);
        s.h.app.set_page_visible(SHELF_PAGE,false);s.frame(vec![]);
        assert!(s.h.app.clipboard_active(),"closing shelf must not stop clipboard");
        assert_eq!(s.h.app.page(),CLIPBOARD_PAGE);
        s.h.app.settings_tab=14;s.h.app.settings.save();
        let restored=crate::model::Settings::load();
        assert!(!restored.visible_pages[SHELF_PAGE]);assert!(restored.visible_pages[CLIPBOARD_PAGE]);
        assert!(crate::app::PAGE_SETTINGS_TABS[SHELF_PAGE]!=crate::app::PAGE_SETTINGS_TABS[CLIPBOARD_PAGE]);
        let pin=if dock==Dock::Top{"clip-pin"}else{"portrait-clip-pin"};
        s.click((pin,0usize));assert!(s.h.app.clipboard.lock().items[0].pinned);
        let remove=if dock==Dock::Top{"clip-delete"}else{"portrait-clip-delete"};
        s.click((remove,1usize));
        let items=s.h.app.clipboard.lock().items.clone();
        assert_eq!(items.len(),2);assert!(items[0].media.as_ref().is_some_and(|media|media.kind==MediaKind::Image));
        assert!(s.h.app.shelf.items.is_empty(),"media history must not add files to shelf");
        s.h.app.set_page_visible(CLIPBOARD_PAGE,false);s.frame(vec![]);
        assert!(!s.h.app.clipboard_active());assert_eq!(s.h.app.clipboard.lock().items.len(),2,"closing a page must preserve records");
    }
}

#[test]
fn clipboard_media_rows_show_previews_open_files_and_fit_side_panels(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let preview=std::env::var_os("QINGYU_CLIPBOARD_PREVIEW_DIR").map(std::path::PathBuf::from);
    if let Some(folder)=&preview{std::fs::create_dir_all(folder).unwrap();}
    for dock in [Dock::Top,Dock::Left,Dock::Right]{
        let mut s=Side::fresh(dock,CLIPBOARD_PAGE);let (picture,video)=seed_clipboard(&mut s);
        let size=s.h.app.canvas_size();s.h.ctx.set_pixels_per_point(2.0);
        let mut raster=preview.as_ref().map(|_|crate::preview_raster::Raster::new((size.x*2.0) as usize,(size.y*2.0) as usize));
        for frame in 0..70{
            let output=s.frame(vec![]);
            if let Some(raster)=&mut raster{
                raster.update_textures(&output.textures_delta);
                if frame==69{
                    raster.buf.fill([0.13,0.18,0.20,1.0]);raster.draw(&s.h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);
                    let mut rgba=vec![];raster.to_rgba8(&mut rgba);
                    image::save_buffer(preview.as_ref().unwrap().join(format!("clipboard-{}.png",dock.key())),&rgba,(size.x*2.0) as u32,(size.y*2.0) as u32,image::ColorType::Rgba8).unwrap();
                }
                raster.free_textures(&output.textures_delta);
            }
        }
        for name in ["海边照片.png","旅行视频.mov","稍后发送的文字"]{
            assert!(s.h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(text) if text.galley.text().contains(name))),"{dock:?} must show {name}");
        }
        if dock.is_side(){
            for (index,path) in [(0usize,picture),(1usize,video)]{
                s.h.cmds.clear();s.click(("portrait-clip-open",index));
                assert!(s.h.cmds.contains(&crate::app::Cmd::Open(path)),"open must use media file, not its display text");
            }
            let body=s.h.app.island_rect(Rect::from_min_size(Pos2::ZERO,size),s.h.app.expanded_width(),s.h.app.expanded_height());
            for page in s.h.app.settings.page_order{
                s.h.cursor=None;s.h.app.show_page(page);s.settle();
                let response=s.h.ctx.read_response(egui::Id::new(("card",true,("portrait-tab",page)))).unwrap();
                assert!(body.expand(1.0).contains_rect(response.rect),"navigation must fit {dock:?}");
                assert!(response.rect.width()>=20.0);
            }
            s.h.app.show_page(CLIPBOARD_PAGE);s.settle();
            s.click("portrait-clip-clear");s.click("portrait-clip-clear");assert!(s.h.app.clipboard.lock().items.is_empty());
        }else{
            s.click("clip-clear");s.click("clip-clear");assert!(s.h.app.clipboard.lock().items.is_empty());
        }
    }
}
