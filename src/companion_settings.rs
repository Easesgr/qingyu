//! Pet identities stay independent from Agent execution and account data.
use crate::{app::App,island_companion::{self,AGENTS,binding_key},model::PetKind,paint::Card};
use eframe::egui;

impl App{
    pub(crate) fn resident_pet_picker(&mut self,ui:&mut egui::Ui){
        let picker_id=ui.id().with("resident-pet-picker");
        ui.horizontal(|ui|{
            ui.label("左侧宠物");
            let follows=AGENTS.into_iter().find(|agent|self.settings.resident_pet_agent.as_deref()==Some(binding_key(*agent)));
            egui::ComboBox::from_id_salt("resident-pet-agent").selected_text(follows.map_or("自由选择",|agent|agent.label())).width(128.0).show_ui(ui,|ui|{
                ui.selectable_value(&mut self.settings.resident_pet_agent,None,"自由选择");
                for agent in AGENTS{ui.selectable_value(&mut self.settings.resident_pet_agent,Some(binding_key(agent).into()),format!("跟随 {}",agent.label()));}
            });
        });
        ui.add_space(7.0);
        ui.horizontal_wrapped(|ui|{
            for kind in PetKind::ALL{
                let selected=self.settings.resident_pet_kind()==kind;
                let (rect,_)=ui.allocate_exact_size(egui::vec2(54.0,64.0),egui::Sense::hover());
                // A stable ID makes keyboard focus survive changes in the
                // generated artwork and lets tests exercise the actual picker.
                let response=ui.interact(rect,picker_id.with(kind.key()),egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
                response.widget_info(||egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel,true,selected,kind.label()));
                if selected||response.hovered(){ui.painter().rect_filled(rect,10.0,if selected{ui.visuals().selection.bg_fill.gamma_multiply(0.16)}else{ui.visuals().widgets.hovered.bg_fill});}
                if selected||response.has_focus(){ui.painter().rect_stroke(rect.shrink(0.7),10.0,egui::Stroke::new(1.3,ui.visuals().selection.stroke.color),egui::StrokeKind::Inside);}
                let c=Card::new(ui,rect.min,ui.clip_rect()).ghost();
                island_companion::pet_icon(&c,kind,27.0,23.0,34.0);
                ui.painter().text(egui::pos2(rect.center().x,rect.bottom()-12.0),egui::Align2::CENTER_CENTER,kind.label(),egui::FontId::proportional(11.0),if selected{ui.visuals().strong_text_color()}else{ui.visuals().text_color()});
                if response.clicked(){self.settings.resident_pet=kind;self.settings.resident_pet_agent=None;}
            }
        });
        ui.add_space(6.0);
        ui.collapsing("Agent 绑定",|ui|self.pet_binding_settings(ui));
    }

    pub(crate) fn pet_binding_settings(&mut self,ui:&mut egui::Ui){
        for agent in AGENTS{
            ui.horizontal(|ui|{
                crate::agent_icons::ui(ui,agent,20.0);
                ui.add_sized([104.0,28.0],egui::Label::new(agent.label()));
                let mut selected=self.settings.agent_pet(agent);
                let old=selected;
                egui::ComboBox::from_id_salt(("agent-pet-binding",binding_key(agent))).selected_text(selected.label()).width(126.0).show_ui(ui,|ui|{
                    for kind in PetKind::ALL{
                        let (rect,response)=ui.allocate_exact_size(egui::vec2(130.0,30.0),egui::Sense::click());
                        if selected==kind||response.hovered(){ui.painter().rect_filled(rect,6.0,if selected==kind{ui.visuals().selection.bg_fill.gamma_multiply(0.16)}else{ui.visuals().widgets.hovered.bg_fill});}
                        let c=Card::new(ui,rect.min,ui.clip_rect()).ghost();
                        island_companion::pet_icon(&c,kind,16.0,15.0,23.0);
                        ui.painter().text(egui::pos2(rect.left()+35.0,rect.center().y),egui::Align2::LEFT_CENTER,kind.label(),egui::FontId::proportional(12.0),ui.visuals().text_color());
                        response.widget_info(||egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel,true,selected==kind,kind.label()));
                        if response.clicked(){selected=kind;ui.close_menu();}
                    }
                });
                let (rect,_)=ui.allocate_exact_size(egui::vec2(27.0,28.0),egui::Sense::hover());
                let c=Card::new(ui,rect.min,ui.clip_rect()).ghost();island_companion::pet_icon(&c,selected,13.5,14.0,24.0);
                if selected!=old{self.settings.agent_pets.insert(binding_key(agent).into(),selected);}
            });
        }
        if ui.small_button("恢复默认搭档").clicked(){self.settings.agent_pets.clear();}
    }
}
