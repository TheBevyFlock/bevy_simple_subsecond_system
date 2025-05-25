# Bevy Simple Subsecond System

[![crates.io](https://img.shields.io/crates/v/bevy_simple_subsecond_system)](https://crates.io/crates/bevy_simple_subsecond_system)
[![docs.rs](https://docs.rs/bevy_simple_subsecond_system/badge.svg)](https://docs.rs/bevy_simple_subsecond_system)


Hotpatch your Bevy systems, allowing you to change their code while the app is running and directly see the results!
This is an intermediate solution you can use until [Bevy implements this feature upstream](https://github.com/bevyengine/bevy/issues/19296).

Powered by [Dioxus' subsecond](https://github.com/DioxusLabs/dioxus/releases/tag/v0.7.0-alpha.0#rust-hot-patching)  
Please report all hotpatch-related problems to them :)


<https://github.com/user-attachments/assets/a44e446b-b2bb-4e10-81c3-3f20cccadea0>



## First Time Installation

First, we need to install a specific version of the Dioxus CLI.

```sh
cargo install dioxus-cli --git https://github.com/DioxusLabs/dioxus --rev b2bd1f
```

Depending on your OS, you'll have to set up your environment a bit more:

<details>
<summary>
Windows
</summary>

If you're lucky, you don't need to change anything.
However, some users may experience issues with their path length.
If that happens, move your crate closer to your drive, e.g. `C:\my_crate`.
If that is not enough, set the following in your `~\.cargo\config.toml`:
```toml
[profile.dev]
codegen-units = 1
```
Note that this may increase compile times significantly if your crate is very large. If you can verify that this solved your issue,
try increasing this number until you find a happy middle ground. For reference, the default number
for incremental builds is `256`, and for non-incremental builds `16`.

You also cannot set `linker = "rust-lld.exe"`, as subsecond currently crashes when `linker` is set.

</details>

<details>
<summary>
macOS
</summary>

You're in luck! Everything should work out of the box if you use the default system linker.

</details>

<details>
<summary>
Linux
</summary>

Execute the following:
```sh
readlink -f "$(which cc)"
```
If this points to `clang`, you're good. Otherwise, we'll need to symlink it.
Read the path returned by the following command:
```
which cc
```
and `cd` into it. For example,

```sh
$ which cc
/usr/bin/cc
$ cd /usr/bin
```

Assuming you have `clang` installed, run the following commands:

```
mv cc cc-real
ln -s "$(which clang)" cc
```
Note that the above commands may require `sudo`.

Now everything should work. If not, install `lld` on your system and add the following to your `~/.cargo/config.toml`:

```toml
[target.x86_64-unknown-linux-gnu]
rustflags = [
  "-Clink-arg=-fuse-ld=lld",
]
```

If you prefer to use `mold`, you can set it up like this:

```toml
[target.x86_64-unknown-linux-gnu]
#linker = clang
rustflags = [
  "-Clink-arg=-fuse-ld=mold",
]
```
Note that the `linker` key needs to be commented out.
You will also need to replace your system `ld` with `mold`.
```sh
cd /usr/bin
sudo mv ld ld-real
sudo ln -s mold ld
```

On NixOS you can do this in a shell by replacing:
```nix
pkgs.mkShell {
    # ..
}
```
with:
```nix
pkgs.mkShell.override {
    stdenv = pkgs.stdenvAdapters.useMoldLinker pkgs.clangStdenv;
} {
    # ..
}
```

</details>

## Usage

Add the crate to your dependencies:

```sh
cargo add bevy_simple_subsecond_system
```

Then add the plugin to your app and annotate any system you want with `#[hot]`:

```rust,ignore
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
```

Now run your app with

```sh
dx serve --hot-patch
```

Now try changing that string at runtime and then check your logs!

Note that changing the `greet` function's signature at runtime by e.g. adding a new parameter will still require a restart.
In general, you can only change the code *inside* the function at runtime. See the *Advanced Usage* section for more.

## Examples

Run the examples with
```sh
dx serve --hot-patch --example name_of_the_example
```

e.g.
```sh
dx serve --hot-patch --example patch_on_update
```



## Advanced Usage
There are some more things you can hot-patch, but they come with extra caveats right now

<details>
<summary>Limitations when using these features</summary>

- Annotating a function relying on local state will clear it every frame. Notably, this means you should not use `#[hot]` on a system that uses any of the following:
  - `EventReader`
  - `Local`
  - Queries filtering with `Added`, `Changed`, or `Spawned`
- Some signatures are not supported, see the tests. Some have `#[hot]` commented out to indicate this
- All hotpatched systems run as exclusive systems, meaning they won't run in parallel
- For component migration:
  - While top level component definitions can be changed and renamed (and will be migrated if using `HotPatchMigrate`), changing definitions of the types used as fields of the components isn't supported. It might work in some cases but most probably will be an undefined behaviour
</details>


<details>
<summary>
<sig>Setup Methods</sig>
</summary>

UI is often spawned in `Startup` or `OnEnter` schedules. Hot-patching such setup systems would be fairly useless, as they wouldn't run again.
For this reason, the plugin supports automatically rerunning systems that have been hot-patched. To opt-in, replace `#[hot]` with `#[hot(rerun_on_hot_patch = true)]`.
See the `rerun_setup` example for detailed instructions.

</details>

<details>
<summary>
<sig>Change signatures at runtime</sig>
</summary>

Replace `#[hot]` with `#[hot(hot_patch_signature = true)]` to allow changing a system's signature at runtime.
This allows you to e.g. add additional `Query` or `Res` parameters or modify existing ones.
</details>


## Features

- Change systems' code and see the effect live at runtime
- If your system calls other functions, you can also change those functions' code at runtime
- Extremely small API: You only need the plugin struct and the `#[hot]` attribute
- Automatically compiles itself out on release builds and when targetting Wasm. The `#[hot]` attribute does simply nothing on such builds.

## Known Limitations

- A change in the definition of structs that appear in hot-patched systems at runtime will result in your query failing to match, as that new type does not exist in `World` yet.
  - Practically speaking, this means you should not change the definition of `Resource`s and `Component`s of your system at runtime
- Only [the topmost binary is hotpatched](https://github.com/DioxusLabs/dioxus/issues/4160), meaning your app is not allowed to have a `lib.rs` or a workspace setup.
- Attaching a debugger is problaby not going to work. Let me know if you try!
- I did not test all possible ways in which systems can be used. Does piping work? Does `bevy_mod_debugdump` still work? Maybe. Let me know!
- Only functions that exist when the app is launched are considered while hotpatching. This means that if you have a system `A` that calls a function `B`, 
  changing `B` will only work at runtime if that function existed already when the app was launched.
- Does nothing on Wasm. This is not a technical limitation, just something we didn't implement yet..

## Compatibility

| bevy        | bevy_simple_subsecond_system |
|-------------|------------------------|
| 0.16        | 0.2                    |
