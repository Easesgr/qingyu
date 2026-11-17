//! The native renderer owns presentation cadence. Timers are for ambient and
//! idle work; transitions request the next vsync without an extra frame wait.
use eframe::egui;
use std::time::Duration;
pub fn schedule(ctx:&egui::Context,interval:Duration){
    if interval.is_zero(){ctx.request_repaint();return;}
    let predicted=ctx.input(|i|Duration::from_secs_f32(i.predicted_dt));
    ctx.request_repaint_after(interval.saturating_add(predicted));
}
