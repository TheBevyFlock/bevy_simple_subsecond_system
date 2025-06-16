//! API for hot-patching new systems into your running app.
//! See [`HotPatchedAppExt::with_hot_patch`] for the main API.

use crate::__macros_internal::__ReloadPositions as ReloadPositions;
use bevy_app::{
    App, NonSendMarker, PostStartup, PostUpdate, PreStartup, PreUpdate, Startup, Update,
};
use bevy_derive::{Deref, DerefMut};
use bevy_ecs::prelude::*;
#[cfg(debug_assertions)]
use bevy_ecs::system::{Commands, Res};
use bevy_ecs_macros::ScheduleLabel;
use bevy_log::{debug, error};

use crate::HotPatched;

/// Wrapper around [`App`] used by [`HotPatchedAppExt::with_hot_patch`], which allows you to add and remove systems at runtime.
#[derive(Deref, DerefMut)]
struct HotPatchedApp(send_wrapper::SendWrapper<App>);

impl Default for HotPatchedApp {
    fn default() -> Self {
        HotPatchedApp(send_wrapper::SendWrapper::new(App::default()))
    }
}

/// The [`Startup`] schedule, but rerun on hot-reload.
/// Only valid inside the context of [`HotPatchedAppExt::with_hot_patch`].
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct StartupRerunHotPatch;

#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash, Default)]
struct HotPatchUpdate;

#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash, Default)]
struct HotPatchPostUpdate;

#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash, Default)]
struct HotPatchPreUpdate;

/// Trait for [`App`] to add and remove systems at runtime.
pub trait HotPatchedAppExt {
    /// Call this with plugins and systems and it will auto-add and remove systems in the `Update` schedule to your running app.
    ///
    /// # Example
    ///
    /// ```ignore
    /// # use bevy::prelude::*;
    /// # use bevy_simple_subsecond_system::prelude::*;
    ///
    /// App::new()
    ///     .add_plugins(DefaultPlugins)
    ///     .add_plugins(SimpleSubsecondPlugin::default())
    ///     .with_hot_patch(|app: &mut App| {
    ///         // Systems in the `StartupRerunHotPatch` schedule will be rerun on hot-reload.
    ///         // They require `#[hot(hot_patch_signature = true)]`
    ///         app.add_systems(StartupRerunHotPatch, setup);
    ///         // All other systems do not require `#[hot]`.
    ///         app.add_systems(Update, my_system);
    ///         app.add_systems(PostUpdate, second_system);
    ///     });
    ///
    /// #[hot(hot_patch_signature = true)]
    /// fn setup(mut commands: Commands) {
    ///     commands.spawn(Camera2d::default());
    ///     commands.spawn(Text::new("Hello, world!"));
    /// }
    ///
    /// fn my_system() {
    ///     info!("Hello, world!");
    /// }
    ///
    /// fn second_system() {
    ///     info!("Goodbye, world!");
    /// }
    /// ```
    fn with_hot_patch(&mut self, func: impl FnMut(&mut App) + Send + Sync + 'static) -> &mut App;
}

impl HotPatchedAppExt for App {
    fn with_hot_patch(
        &mut self,
        mut func: impl FnMut(&mut App) + Send + Sync + 'static,
    ) -> &mut App {
        let mut app = App::new();
        app.init_schedule(Startup);
        app.init_schedule(PostStartup);
        app.init_schedule(PreStartup);
        std::mem::swap(
            app.get_schedule_mut(Startup).unwrap(),
            self.get_schedule_mut(Startup).unwrap(),
        );
        std::mem::swap(
            app.get_schedule_mut(PreStartup).unwrap(),
            self.get_schedule_mut(PreStartup).unwrap(),
        );
        std::mem::swap(
            app.get_schedule_mut(PostStartup).unwrap(),
            self.get_schedule_mut(PostStartup).unwrap(),
        );

        func(&mut app);

        std::mem::swap(
            app.get_schedule_mut(Startup).unwrap(),
            self.get_schedule_mut(Startup).unwrap(),
        );
        std::mem::swap(
            app.get_schedule_mut(PreStartup).unwrap(),
            self.get_schedule_mut(PreStartup).unwrap(),
        );
        std::mem::swap(
            app.get_schedule_mut(PostStartup).unwrap(),
            self.get_schedule_mut(PostStartup).unwrap(),
        );

        self.add_systems(PreUpdate, |world: &mut World| {
            let _ = world.try_run_schedule(HotPatchPreUpdate);
        });
        self.add_systems(Update, |world: &mut World| {
            let _ = world.try_run_schedule(HotPatchUpdate);
        });
        self.add_systems(PostUpdate, |world: &mut World| {
            let _ = world.try_run_schedule(HotPatchPostUpdate);
        });

        self.add_systems(Startup, |world: &mut World| {
            world.insert_resource(ReloadPositions::default());
        });

        let hot_patched_func = move |mut hot_patched_app: HotPatchedApp| -> HotPatchedApp {
            func(&mut hot_patched_app.0);
            hot_patched_app
        };
        let reloadable_section =
            std::sync::Mutex::new(dioxus_devtools::subsecond::HotFn::current(hot_patched_func));
        self.add_systems(
            PreUpdate,
            move |_: Option<NonSend<NonSendMarker>>,
                  mut ran_once: Local<bool>,
                  mut schedules: ResMut<Schedules>,
                  mut commands: Commands,
                  hotreload_event: EventReader<HotPatched>| {
                if hotreload_event.is_empty() {
                    if *ran_once {
                        return;
                    }
                    *ran_once = true;
                }

                let reload_app = reloadable_section
                    .lock()
                    .unwrap()
                    .try_call((HotPatchedApp::default(),));

                let mut reload_app = match reload_app {
                    Ok(reload_app) => reload_app,
                    Err(e) => {
                        error!("Failed to call hotpatch function: {e:?}");
                        return;
                    }
                };

                let mut reload_schedules = reload_app
                    .world_mut()
                    .get_resource_mut::<Schedules>()
                    .unwrap();

                if let Some(mut reload_update) = reload_schedules.remove(Update) {
                    schedules.remove(HotPatchUpdate);
                    let hot_reload_update = schedules.entry(HotPatchUpdate);
                    *hot_reload_update.graph_mut() = std::mem::take(reload_update.graph_mut());
                    commands.run_system_cached(|world: &mut World| {
                        world.schedule_scope(HotPatchUpdate, |world, hot_reload_update| {
                            let result = hot_reload_update.initialize(world);
                            if let Err(e) = result {
                                error!("Failed to initialize hotpatch update: {e}");
                            }
                        });
                    });
                };

                if let Some(mut reload_post_update) = reload_schedules.remove(PostUpdate) {
                    schedules.remove(HotPatchPostUpdate);
                    let hot_reload_post_update = schedules.entry(HotPatchPostUpdate);
                    *hot_reload_post_update.graph_mut() =
                        std::mem::take(reload_post_update.graph_mut());
                    commands.run_system_cached(|world: &mut World| {
                        world.schedule_scope(
                            HotPatchPostUpdate,
                            |world, hot_reload_post_update| {
                                let result = hot_reload_post_update.initialize(world);
                                if let Err(e) = result {
                                    error!("Failed to initialize hotpatch post-update: {e}");
                                }
                            },
                        );
                    });
                };

                if let Some(mut reload_pre_update) = reload_schedules.remove(PreUpdate) {
                    schedules.remove(HotPatchPreUpdate);
                    let hot_reload_pre_update = schedules.entry(HotPatchPreUpdate);
                    *hot_reload_pre_update.graph_mut() =
                        std::mem::take(reload_pre_update.graph_mut());
                    commands.run_system_cached(|world: &mut World| {
                        world.schedule_scope(HotPatchPreUpdate, |world, hot_reload_pre_update| {
                            let result = hot_reload_pre_update.initialize(world);
                            if let Err(e) = result {
                                error!("Failed to initialize hotpatch pre-update: {e}");
                            }
                        });
                    });
                };

                if let Some(mut auto_reload_startup) = reload_schedules.remove(StartupRerunHotPatch)
                {
                    schedules.remove(StartupRerunHotPatch);
                    let schedule: &mut Schedule = schedules.entry(StartupRerunHotPatch);
                    *schedule.graph_mut() = std::mem::take(auto_reload_startup.graph_mut());
                    commands.run_system_cached(|world: &mut World| {
                        world.schedule_scope(StartupRerunHotPatch, |world, auto_reload_startup| {
                            let result = auto_reload_startup.initialize(world);
                            if let Err(e) = result {
                                error!("Failed to initialize hotpatch auto_reload_startup: {e}");
                            }
                        });
                    });

                    commands.run_system_cached(
                        |mut commands: Commands,
                         query: Query<Entity>,
                         reload_positions: Res<ReloadPositions>,
                         world: &World| {
                            for e in query.iter() {
                                let Some(location) = world
                                    .entities()
                                    .entity_get_spawned_or_despawned_by(e)
                                    .into_option()
                                else {
                                    continue;
                                };
                                let Some(location) = location else { continue };
                                for (file, line_start, line_end) in reload_positions.iter() {
                                    if location.file() != *file {
                                        continue;
                                    }
                                    if location.line() > *line_start && location.line() < *line_end
                                    {
                                        debug!("despawning an entity at: {location:?}");
                                        commands.entity(e.entity()).despawn();
                                    }
                                }
                            }
                        },
                    );
                    commands.run_system_cached(|world: &mut World| {
                        // we clear our reload positions every time so we can fill them up with new stuff.
                        world.insert_resource(ReloadPositions::default());
                        if let Err(e) = world.try_run_schedule(StartupRerunHotPatch) {
                            error!("Failed to auto-reload startup: {e:?}");
                        }
                    })
                }
            },
        );
        self
    }
}
