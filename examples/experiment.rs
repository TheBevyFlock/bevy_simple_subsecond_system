use std::ops::DerefMut as _;

use bevy::prelude::*;
use bevy_ecs::system::ScheduleSystem;
use bevy_simple_subsecond_system::prelude::*;
use dioxus_devtools::subsecond::HotFn;
fn main() -> AppExit {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(SimpleSubsecondPlugin::default())
        .add_systems(Update, greet)
        .enable_hotpatching()
        .run()
}

fn greet(time: Res<Time>) {
    info_once!(
        "Hello from a hotpatched system! Try changing this string while the app is running! Patched at t = {} s",
        time.elapsed_secs()
    );
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
                let cloned = unsafe { system.clone_shallow() };
                let hot_system = cloned.with_hotpatching();
                new_schedules.add_systems(interned_label, hot_system);
            }
        }
        *self.world_mut().resource_mut::<Schedules>() = new_schedules;
        Box::leak(Box::new(old_schedules));
        self
    }
}

trait HotSystemExt {
    fn with_hotpatching(self) -> ScheduleSystem;
}
impl HotSystemExt for ScheduleSystem {
    fn with_hotpatching(mut self) -> ScheduleSystem {
        let a = IntoSystem::into_system(move |world: &mut World| {
            HotFn::current(|world: &mut World| self.run((), world)).call((world,))
        });
        Box::new(a)
    }
}

trait CloneUnchecked {
    unsafe fn clone_shallow(&self) -> ScheduleSystem;
}

impl CloneUnchecked for ScheduleSystem {
    unsafe fn clone_shallow(&self) -> ScheduleSystem {
        // Bitwise copy of the fat pointer
        let raw = self.as_ref() as *const dyn System<In = (), Out = Result>;
        // Create a new box pointing to the same data
        unsafe { std::mem::transmute(raw) }
    }
}
