//! Top toolbar panel — save/load, new entity, delete entity.

/// Draw the top toolbar panel.
pub(crate) fn toolbar_panel(ctx: &egui::Context) {
    egui::TopBottomPanel::top("editor_toolbar").show(ctx, |ui| {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.label("necs editor");
            ui.separator();

            if ui.button("New Entity").clicked() {
                log::info!("[editor] New Entity clicked (TODO)");
            }
            if ui.button("Delete Entity").clicked() {
                log::info!("[editor] Delete Entity clicked (TODO)");
            }

            ui.separator();

            if ui.button("Save Scene").clicked() {
                log::info!("[editor] Save Scene clicked (TODO)");
            }
            if ui.button("Load Scene").clicked() {
                log::info!("[editor] Load Scene clicked (TODO)");
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label("F12 to toggle");
            });
        });
    });
}
