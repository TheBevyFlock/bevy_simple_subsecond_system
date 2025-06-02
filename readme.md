# Bevy Simple Subsecond System

[![crates.io](https://img.shields.io/crates/v/bevy_simple_subsecond_system)](https://crates.io/crates/bevy_simple_subsecond_system)
[![docs.rs](https://docs.rs/bevy_simple_subsecond_system/badge.svg)](https://docs.rs/bevy_simple_subsecond_system)


Hotpatch your Bevy systems and observers, allowing you to change their code while the app is running and directly see the results!
This is an intermediate solution you can use until [Bevy implements this feature upstream](https://github.com/bevyengine/bevy/issues/19296).

Powered by [Dioxus' subsecond](https://github.com/DioxusLabs/dioxus/releases/tag/v0.7.0-alpha.0#rust-hot-patching)  
Please report all hotpatch-related problems to them :)


<https://github.com/user-attachments/assets/a44e446b-b2bb-4e10-81c3-3f20cccadea0>


## First Time Installation


First, we need to install the Dioxus CLI of the newest alpha build.
```sh
cargo install dioxus-cli@0.7.0-alpha.1 -y
```
> Building the CLI like this can take a while. To speed this up,
consider setting up [cargo-binstall](https://github.com/cargo-bins/cargo-binstall) first.

Depending on your OS, you'll have to set up your environment a bit more:

### Windows
For some users, this should work out of the box on Windows

<details>
<summary>
See here if you have issues with path length
</summary>


If that happens, move your crate closer to your drive, e.g. `C:\my_crate`.

If that is not enough, create or edit either a global `~\.cargo\config.toml` or a local `.\.cargo\config.toml` with this config:
```toml
[profile.dev]
codegen-units = 1
```
Note that this may increase compile times significantly if your crate is very large. 
When changing this number, always run `cargo clean` before rebuilding.
If you can verify that this solved your issue,
try increasing this number until you find a happy middle ground. For reference, the default number
for incremental builds is `256`, and for non-incremental builds `16`.

</details>

### MacOS


You're in luck! Everything should work out of the box if you use the default system linker.


### Linux

Prerequisites: `clang` and either `lld` (recommended) or `mold` (faster, but less stable)

<details>
<summary>
Minimal config
</summary>

Create or edit either a global `~/.cargo/config.toml` or a local `./.cargo/config.toml` with this minimal config
```toml
[target.x86_64-unknown-linux-gnu]
linker = "clang"
rustflags = [
  "-C",
  "link-arg=-fuse-ld=lld",
]
```

> ⚠️ WARNING
> In the past we recommended symlinking mold over /usr/bin/ld
> Please make sure to undo this to avoid issues with your installation
> cause by incompatibilities, such as DKMS failing to load modules

</details>

<details>
<summary>
Steps to get maximum performance
</summary>

- Use nightly Rust
- Install mold and clang through your package manager
- Install cranelift with `rustup component add rustc-codegen-cranelift-preview --toolchain nightly`
- Put the following config in your global `~/.cargo/config.toml` or local `./.cargo/config.toml`:
```toml
[unstable]
codegen-backend = true

[profile]
incremental = true

[profile.dev]
codegen-backend = "cranelift"
debug = "line-tables-only"

[profile.dev.package."*"]
codegen-backend = "llvm"

[profile.test.package."*"]
codegen-backend = "llvm"

[profile.release]
codegen-backend = "llvm"

[profile.web]
codegen-backend = "llvm"

[target.x86_64-unknown-linux-gnu]
linker = "clang"
rustflags = [
  "-Clink-arg=-fuse-ld=mold",
  "-Zshare-generics=y",
  "-Zthreads=8",
]
```

If you run into trouble, replace `mold` with `lld`.

This repo also includes `./.cargo/config_faster_builds.toml` which contains more advanced compile-time improving configs known to work with subsecond.


</details>


## Usage

Add the crate to your dependencies.

```sh
cargo add bevy_simple_subsecond_system
```

Then add the plugin to your app and annotate any system you want with `#[hot]`:

```rust,ignore
use bevy::prelude::*;
use bevy_simple_subsecond_system::prelude::*;

fn main() -> AppExit {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(SimpleSubsecondPlugin::default())
        .add_systems(Update, greet)
        .run()
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
BEVY_ASSET_ROOT="." dx serve --hot-patch
```

or on Windows' PowerShell
```pwsh
$env:BEVY_ASSET_ROOT="." ; dx serve --hot-patch
```

Now try changing that string at runtime and then check your logs!

Note that changing the `greet` function's signature at runtime by e.g. adding a new parameter will still require a restart.
In general, you can only change the code *inside* the function at runtime. See the *Advanced Usage* section for more.

## Examples

Run the examples with
```sh
BEVY_ASSET_ROOT="." dx serve --hot-patch --example name_of_the_example
```

e.g.
```sh
BEVY_ASSET_ROOT="." dx serve --hot-patch --example patch_on_update
```

## Features

- Change systems' and observers' code and see the effect live at runtime
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

## Language Servers

In general, rust-analyzer will play nice with the `#[hot]` attribute.
If you're running into issues, you can configure your editor like this:
<details><summary>VSCode settings.json</summary>

```json
"rust-analyzer.procMacro.ignored": {
    "bevy_simple_subsecond_system_macros": [
        "hot"
    ]
},
"rust-analyzer.diagnostics.disabled": [
    "proc-macro-disabled"
]
```
</details>
<br/>

<details><summary>Vim lspconfig</summary>

```lua
lspconfig.rust_analyzer.setup({
  capabilities = capabilities,
  settings = {
    ["rust-analyzer"] = {
      procMacro = {
        ignored = {
          bevy_simple_subsecond_system_macros = { "hot" },
        },
      },
      diagnostics = {
        disabled = { "proc-macro-disabled" },
      },
    },
  },
})
```
</details>


## Advanced Usage
There are some more things you can hot-patch, but they come with extra caveats right now

<details>
<summary>Limitations when using these features</summary>

- Annotating a function relying on local state will clear it every frame. Notably, this means you should not use `#[hot(rerun_on_hot_patch)]` or `#[hot(hot_patch_signature)]` on a system that uses any of the following:
  - `EventReader`
  - `Local`
  - Queries filtering with `Added`, `Changed`, or `Spawned`
- Some signatures are not supported, see the tests. Some have `#[hot(rerun_on_hot_patch)]` or `#[hot(hot_patch_signature)]` commented out to indicate this
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


## Compatibility

| bevy | bevy_simple_subsecond_system |
| ---- | ---------------------------- |
| 0.16 | 0.2                          |
