//! Entity hierarchy panel — left side panel showing all entities as a tree.

use crate::ecs::hierarchy::{Children, Parent};
use crate::ecs::Entity;
use crate::ecs::world::World;

/// Draw the entity hierarchy panel. Returns the currently selected entity.
pub(crate) fn hierarchy_panel(
    ctx: &egui::Context,
    world: &World,
    selected: Option<Entity>,
) -> Option<Entity> {
    let mut new_selected = selected;

    egui::SidePanel::left("hierarchy_panel")
        .default_width(200.0)
        .resizable(true)
        .show(ctx, |ui| {
            ui.heading("Hierarchy");
            ui.separator();

            // Collect root entities (those without a Parent component).
            let mut roots = Vec::new();
            for (entity, _name) in world.named_entities() {
                if world.get::<Parent>(entity).is_none() {
                    roots.push(entity);
                }
            }

            // Also find unnamed root entities.
            let all_entities = world.all_entities();
            for &entity in &all_entities {
                if world.get::<Parent>(entity).is_none()
                    && !roots.contains(&entity)
                {
                    roots.push(entity);
                }
            }

            // Sort roots for stable display order.
            roots.sort_by_key(|e| e.index);

            egui::ScrollArea::vertical().show(ui, |ui| {
                for &root in &roots {
                    draw_entity_tree(ui, world, root, &mut new_selected, 0);
                }
            });
        });

    new_selected
}

fn draw_entity_tree(
    ui: &mut egui::Ui,
    world: &World,
    entity: Entity,
    selected: &mut Option<Entity>,
    depth: usize,
) {
    let label = entity_display_name(world, entity);
    let is_selected = *selected == Some(entity);
    let children = world.get::<Children>(entity);
    let has_children = children.map_or(false, |c| !c.0.is_empty());

    if has_children {
        let id = ui.make_persistent_id(entity.index);
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, depth < 2)
            .show_header(ui, |ui| {
                if ui.selectable_label(is_selected, &label).clicked() {
                    *selected = Some(entity);
                }
            })
            .body(|ui| {
                if let Some(children) = children {
                    for &child in &children.0 {
                        draw_entity_tree(ui, world, child, selected, depth + 1);
                    }
                }
            });
    } else {
        ui.horizontal(|ui| {
            ui.add_space(18.0); // Indent for leaf nodes
            if ui.selectable_label(is_selected, &label).clicked() {
                *selected = Some(entity);
            }
        });
    }
}

fn entity_display_name(world: &World, entity: Entity) -> String {
    if let Some(name) = world.entity_name(entity) {
        format!("{} ({})", name, entity.index)
    } else {
        // Check for tags.
        let tags = world.entity_tags(entity);
        if let Some(first_tag) = tags.first() {
            format!("[{}] ({})", first_tag, entity.index)
        } else {
            format!("Entity {}", entity.index)
        }
    }
}
