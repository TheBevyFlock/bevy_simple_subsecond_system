use bevy::prelude::*;
use bevy_simple_subsecond_system::prelude::*;
fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(SimpleSubsecondPlugin::default())
        .add_systems(Update, greet)
        .run();
}

#[hot]
fn greet(time: Res<Time>) {
    info_once!(
        "Hello from a hotpatched system! Try changing this string while the app is running! Patched at t = {} s",
        time.elapsed_secs()
    );
}
