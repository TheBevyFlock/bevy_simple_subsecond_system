#![warn(missing_docs)]
#![allow(clippy::type_complexity)]
#![doc = include_str!("../readme.md")]

pub mod migration;

#[cfg(all(not(target_family = "wasm"), debug_assertions))]
use __macros_internal::__HotPatchedSystems as HotPatchedSystems;
use bevy_app::{App, Last, Plugin, PostStartup, PreUpdate};
use bevy_ecs::prelude::*;
pub use bevy_simple_subsecond_system_macros::*;
pub use dioxus_devtools;
#[cfg(all(not(target_family = "wasm"), debug_assertions))]
use dioxus_devtools::{subsecond::apply_patch, *};

pub mod hot_patched_app;

/// Everything you need to use hotpatching
pub mod prelude {
    pub use super::{
        HotPatched, SimpleSubsecondPlugin,
        hot_patched_app::{HotPatchedAppExt as _, StartupRerunHotPatch},
    };
    pub use crate::migration::*;
    pub use bevy_simple_subsecond_system_macros::*;
}

/// The plugin you need to add to your app:
///
/// ```ignore
/// use bevy::prelude::*;
/// use bevy_simple_subsecond_system::prelude::*;
///
/// App::new()
///     .add_plugins(DefaultPlugins)
///     .add_plugins(SimpleSubsecondPlugin::default())
///     // rest of the setup
///     .run();
/// ```
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct SimpleSubsecondPlugin;

impl Plugin for SimpleSubsecondPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            PreUpdate,
            (
                SimpleSubsecondSystemSet::UpdateFunctionPtrs,
                SimpleSubsecondSystemSet::ComponentMigrations,
            )
                .chain(),
        );
        #[cfg(target_family = "wasm")]
        {
            warn!("Hotpatching is not supported on Wasm yet. Disabling SimpleSubsecondPlugin.");
            return;
        }
        #[cfg(not(debug_assertions))]
        {
            return;
        }
        #[cfg(all(not(target_family = "wasm"), debug_assertions))]
        {
            let (sender, receiver) = crossbeam_channel::bounded::<HotPatched>(1);
            connect(move |msg| {
                if let DevserverMsg::HotReload(hot_reload_msg) = msg {
                    if let Some(jumptable) = hot_reload_msg.jump_table {
                        // SAFETY: This is not unsafe, but anything using the updated jump table is.
                        // The table must be built carefully
                        unsafe { apply_patch(jumptable).unwrap() };
                        sender.send(HotPatched).unwrap();
                    }
                }
            });

            app.init_resource::<HotPatchedSystems>();

            app.add_event::<HotPatched>().add_systems(
                Last,
                move |mut events: EventWriter<HotPatched>| {
                    if receiver.try_recv().is_ok() {
                        events.write_default();
                    }
                },
            );

            app.init_resource::<migration::ComponentMigrations>();
            app.add_systems(PostStartup, migration::register_migratable_components)
                .add_systems(
                    PreUpdate,
                    migration::migrate.in_set(SimpleSubsecondSystemSet::ComponentMigrations),
                );
        }
    }
}

/// Event sent when the hotpatch is applied.
#[derive(Event, Default)]
pub struct HotPatched;

/// System set in which components are migrated after a hot patch.
/// Belongs to the [`PreUpdate`] schedule.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum SimpleSubsecondSystemSet {
    /// Update the pointers to the current function definitions.
    /// Systems with `#[hot(rerun_on_hot_patch = true)]` will be rerun here.
    UpdateFunctionPtrs,
    /// Migrate components that derive `HotPatchMigrate` that were changed at runtime.
    ComponentMigrations,
}

#[doc(hidden)]
pub mod __macros_internal {
    pub use bevy_app::PreUpdate;
    use bevy_derive::{Deref, DerefMut};
    pub use bevy_ecs::{
        schedule::Schedules,
        system::{IntoSystem, SystemId, SystemState},
        world::World,
    };
    pub use bevy_ecs_macros::Resource;
    pub use bevy_log::debug;
    use bevy_platform::collections::{HashMap, HashSet};
    use dioxus_devtools::subsecond::HotFnPtr;
    use std::any::TypeId;

    #[derive(Resource, Default)]
    pub struct __HotPatchedSystems(pub HashMap<TypeId, __HotPatchedSystem>);

    #[doc(hidden)]
    pub struct __HotPatchedSystem {
        pub current_ptr: HotFnPtr,
        pub last_ptr: HotFnPtr,
    }

    #[doc(hidden)]
    #[derive(Deref, DerefMut, Resource, Default, Debug)]
    pub struct __ReloadPositions(pub HashSet<(&'static str, u32, u32)>);
}
