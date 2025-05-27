use std::{mem::ManuallyDrop, ops::DerefMut as _};

use bevy::prelude::*;
use bevy_ecs::system::ScheduleSystem;
use bevy_simple_subsecond_system::prelude::*;
fn main() -> AppExit {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(SimpleSubsecondPlugin::default())
        .add_systems(Update, greet)
        .enable_hotpatching()
        .run()
}

trait HotAppExt {
    fn enable_hotpatching(&mut self) -> &mut App;
}
impl HotAppExt for App {
    fn enable_hotpatching(&mut self) -> &mut App {
        let mut old_schedules =
            std::mem::take(self.world_mut().resource_mut::<Schedules>().deref_mut());

        let mut new_schedules = Schedules::default();
        for (_label_ref, schedule) in old_schedules.iter_mut() {
            schedule.initialize(self.world_mut()).unwrap();
            let interned_label = schedule.label();
            for (_node_id, system) in schedule.systems().unwrap() {
                let cloned = unsafe { shallow_clone_system(system) };
                let hot_system = cloned.with_hotpatching();
                new_schedules.add_systems(interned_label, hot_system);
            }
        }
        *self.world_mut().resource_mut::<Schedules>() = new_schedules;
        self.insert_resource(OriginalSchedules(old_schedules));
        self
    }
}

#[derive(Resource)]
struct OriginalSchedules(#[allow(dead_code)] Schedules);

trait HotSystemExt {
    fn with_hotpatching(self) -> ScheduleSystem;
}
impl HotSystemExt for ScheduleSystem {
    fn with_hotpatching(mut self) -> ScheduleSystem {
        let a = IntoSystem::into_system(move |world: &mut World| self.run((), world));
        Box::new(a)
    }
}

unsafe fn shallow_clone_system(original: &ScheduleSystem) -> ScheduleSystem {
    // Create a raw pointer to the original Box
    let raw: *const Box<dyn System<In = (), Out = Result>> = original;

    // Bit-copy it (duplicate the fat pointer)
    unsafe {
        let cloned: ScheduleSystem = (*raw).clone_unchecked();
        cloned
    }
}

trait CloneUnchecked {
    unsafe fn clone_unchecked(&self) -> ScheduleSystem;
}

impl CloneUnchecked for ScheduleSystem {
    unsafe fn clone_unchecked(&self) -> ScheduleSystem {
        // Bitwise copy of the fat pointer
        let raw = self.as_ref() as *const dyn System<In = (), Out = Result>;
        let raw_copy: *const dyn System<In = (), Out = Result> = raw;
        // Create a new box pointing to the same data
        unsafe {
            let copied: ScheduleSystem = std::mem::transmute(raw_copy);
            copied
        }
    }
}

fn greet(time: Res<Time>) {
    info_once!(
        "Hello from a hotpatched system! Try changing this string while the app is running! Patched at t = {} s",
        time.elapsed_secs()
    );
}
