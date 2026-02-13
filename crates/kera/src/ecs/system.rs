//! # System — Functions That Operate on the World
//!
//! A system is just a function that takes `&mut World` and does something with
//! it — query entities, modify components, read resources. That's it.
//!
//! ## Design Philosophy
//!
//! Many ECS frameworks make systems complex — parameter injection, dependency
//! graphs, parallel scheduling. We keep it radically simple:
//!
//! - A system is `FnMut(&mut World)`.
//! - Systems run in the order they're added.
//! - No automatic parallelism (you can use rayon inside a system if you want).
//!
//! This is enough for a learning framework. Automatic parallelism is a
//! significant complexity budget we don't want to spend yet.
//!
//! ## Schedule
//!
//! A [`Schedule`] is just a `Vec` of systems. Call `run()` and they execute
//! sequentially. Startup systems run once; regular systems run every frame.
//!
//! ## Comparison
//!
//! - **hecs**: Doesn't have a built-in system/schedule concept at all.
//! - **bevy_ecs**: Has `SystemParam` for automatic injection, parallel
//!   execution with conflict detection, run conditions, etc. Much more complex.
//!
//! We're closer to hecs: "systems are just functions, scheduling is your
//! problem." But we do provide a simple `Schedule` for convenience.

use super::world::World;

/// A system that can be executed on a [`World`].
///
/// Any `FnMut(&mut World)` implements this trait, so you can use closures or
/// function pointers directly.
pub trait System {
    fn run(&mut self, world: &mut World);
}

/// Blanket impl: any `FnMut(&mut World)` is a `System`.
impl<F: FnMut(&mut World)> System for F {
    fn run(&mut self, world: &mut World) {
        (self)(world);
    }
}

/// A named system wrapping a boxed [`System`] with a short name for diagnostics.
struct NamedSystem {
    #[cfg(any(feature = "diagnostics", test))]
    name: String,
    system: Box<dyn System>,
}

/// Per-system timing recorded during a single frame.
#[cfg(feature = "diagnostics")]
pub(crate) struct SystemTiming {
    pub name: String,
    pub duration_us: f64,
}

/// An ordered list of systems to run.
pub struct Schedule {
    systems: Vec<NamedSystem>,
    /// Per-system timings from the most recent `run()` call.
    #[cfg(feature = "diagnostics")]
    pub(crate) timings: Vec<SystemTiming>,
}

impl Schedule {
    pub fn new() -> Self {
        Self {
            systems: Vec::new(),
            #[cfg(feature = "diagnostics")]
            timings: Vec::new(),
        }
    }

    /// Add a system to the end of the schedule.
    pub fn add_system<S: System + 'static>(&mut self, system: S) {
        self.systems.push(NamedSystem {
            #[cfg(any(feature = "diagnostics", test))]
            name: short_system_name(std::any::type_name::<S>()),
            system: Box::new(system),
        });
    }

    /// Run all systems in order on the given world.
    pub fn run(&mut self, world: &mut World) {
        #[cfg(feature = "diagnostics")]
        {
            self.timings.clear();
            for ns in &mut self.systems {
                let start = std::time::Instant::now();
                ns.system.run(world);
                let elapsed = start.elapsed();
                self.timings.push(SystemTiming {
                    name: ns.name.clone(),
                    duration_us: elapsed.as_secs_f64() * 1_000_000.0,
                });
            }
        }
        #[cfg(not(feature = "diagnostics"))]
        {
            for ns in &mut self.systems {
                ns.system.run(world);
            }
        }
    }

    /// Returns the number of systems in this schedule.
    pub fn len(&self) -> usize {
        self.systems.len()
    }
}

impl Default for Schedule {
    fn default() -> Self {
        Self::new()
    }
}

/// Strip the module path from a fully-qualified type name, keeping only the
/// last meaningful segment (e.g. `hello_2d::movement_system` → `movement_system`,
/// `{{closure}}` → `<closure>`).
#[cfg(any(feature = "diagnostics", test))]
fn short_system_name(full: &str) -> String {
    let name = full.rsplit("::").next().unwrap_or(full);
    if name.contains("closure") {
        "<closure>".to_string()
    } else {
        name.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_system(_world: &mut World) {}

    #[test]
    fn schedule_captures_system_name() {
        let mut schedule = Schedule::new();
        schedule.add_system(dummy_system);
        assert_eq!(schedule.systems.len(), 1);
        assert_eq!(schedule.systems[0].name, "dummy_system");
    }

    #[test]
    fn closure_system_name() {
        let mut schedule = Schedule::new();
        schedule.add_system(|_world: &mut World| {});
        assert_eq!(schedule.systems[0].name, "<closure>");
    }
}
