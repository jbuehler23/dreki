//! Component inspector panel — right side panel showing the selected entity's
//! components with editable fields.

use crate::ecs::Entity;
use crate::ecs::world::World;
use crate::math::Transform;

/// Draw the component inspector panel for the selected entity.
pub(crate) fn inspector_panel(
    ctx: &egui::Context,
    world: &mut World,
    selected: Option<Entity>,
) {
    egui::SidePanel::right("inspector_panel")
        .default_width(280.0)
        .resizable(true)
        .show(ctx, |ui| {
            ui.heading("Inspector");
            ui.separator();

            let Some(entity) = selected else {
                ui.label("No entity selected");
                return;
            };

            // Entity header.
            ui.label(format!("Entity {}", entity.index));
            if let Some(name) = world.entity_name(entity) {
                ui.label(format!("Name: {}", name));
            }
            let tags = world.entity_tags(entity);
            if !tags.is_empty() {
                ui.label(format!("Tags: {}", tags.join(", ")));
            }
            ui.separator();

            // Transform component (editable).
            if let Some(tf) = world.get_mut::<Transform>(entity) {
                egui::CollapsingHeader::new("Transform")
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Position");
                        });
                        ui.horizontal(|ui| {
                            ui.label("X:");
                            ui.add(egui::DragValue::new(&mut tf.translation.x).speed(1.0));
                            ui.label("Y:");
                            ui.add(egui::DragValue::new(&mut tf.translation.y).speed(1.0));
                            ui.label("Z:");
                            ui.add(egui::DragValue::new(&mut tf.translation.z).speed(1.0));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Scale");
                        });
                        ui.horizontal(|ui| {
                            ui.label("X:");
                            ui.add(egui::DragValue::new(&mut tf.scale.x).speed(0.01));
                            ui.label("Y:");
                            ui.add(egui::DragValue::new(&mut tf.scale.y).speed(0.01));
                            ui.label("Z:");
                            ui.add(egui::DragValue::new(&mut tf.scale.z).speed(0.01));
                        });
                        let (mut yaw, mut pitch, mut roll) =
                            tf.rotation.to_euler(glam::EulerRot::YXZ);
                        yaw = yaw.to_degrees();
                        pitch = pitch.to_degrees();
                        roll = roll.to_degrees();
                        ui.horizontal(|ui| {
                            ui.label("Rotation (deg)");
                        });
                        let mut changed = false;
                        ui.horizontal(|ui| {
                            changed |= ui.add(egui::DragValue::new(&mut yaw).speed(0.5).prefix("Y: ")).changed();
                            changed |= ui.add(egui::DragValue::new(&mut pitch).speed(0.5).prefix("X: ")).changed();
                            changed |= ui.add(egui::DragValue::new(&mut roll).speed(0.5).prefix("Z: ")).changed();
                        });
                        if changed {
                            tf.rotation = glam::Quat::from_euler(
                                glam::EulerRot::YXZ,
                                yaw.to_radians(),
                                pitch.to_radians(),
                                roll.to_radians(),
                            );
                        }
                    });
            }

            // List other component types (read-only for now).
            let type_names = world.entity_component_names(entity);
            for name in &type_names {
                if *name == "Transform" {
                    continue; // Already handled above.
                }
                egui::CollapsingHeader::new(*name)
                    .default_open(false)
                    .show(ui, |ui| {
                        ui.label("(read-only view)");
                    });
            }
        });
}
