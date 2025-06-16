//! Implements the Bevy Simple Subsecond System derives.
#![warn(missing_docs)]

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::spanned::Spanned;
use syn::{
    DeriveInput, FnArg, Ident, ItemFn, LitBool, Pat, PatIdent, ReturnType, Token, Type, TypePath,
    TypeReference,
    parse::{Parse, ParseStream},
    parse_macro_input,
};

struct HotArgs {
    rerun_on_hot_patch: Option<bool>,
    hot_patch_signature: Option<bool>,
}

impl Parse for HotArgs {
    fn parse(input: ParseStream) -> std::result::Result<HotArgs, syn::Error> {
        let mut rerun_on_hot_patch = None;
        let mut hot_patch_signature = None;

        while !input.is_empty() {
            let ident: Ident = input.parse()?;
            input.parse::<Token![=]>()?;

            if ident == "rerun_on_hot_patch" {
                let value: LitBool = input.parse()?;
                rerun_on_hot_patch = Some(value.value);
            } else if ident == "hot_patch_signature" {
                let value: LitBool = input.parse()?;
                hot_patch_signature = Some(value.value);
            } else {
                return Err(syn::Error::new_spanned(ident, "Unknown attribute key"));
            }

            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(HotArgs {
            rerun_on_hot_patch,
            hot_patch_signature,
        })
    }
}

/// Annotate your systems with `#[hot]` to enable hotpatching for them.
#[proc_macro_attribute]
pub fn hot(attr: TokenStream, item: TokenStream) -> TokenStream {
    // Parse the attribute as a Meta
    let args = syn::parse::<HotArgs>(attr.clone());
    let args = match args {
        Ok(parsed) => parsed,
        Err(_) => return item, // If parsing the attributes fails, just return the original function.
    };
    let rerun_on_hot_patch = args.rerun_on_hot_patch.unwrap_or(false);
    let hot_patch_signature = args.hot_patch_signature.unwrap_or(false);

    let input_fn = syn::parse::<ItemFn>(item.clone());
    let input_fn = match input_fn {
        Ok(parsed) => parsed,
        Err(_) => return item, // If parsing the function fails, return it unchanged.
    };

    let vis = &input_fn.vis;
    let sig = &input_fn.sig;
    let original_output = &sig.output;
    let original_fn_name = &sig.ident;
    let block = &input_fn.block;
    let inputs = &sig.inputs;
    let generics = &sig.generics;

    // Generate new identifiers
    let hotpatched_fn = format_ident!("__{}_hotpatched", original_fn_name);
    let original_wrapper_fn = format_ident!("__{}_original", original_fn_name);

    let newlines = if let Some(source_text) = block.span().unwrap().source_text() {
        source_text.chars().filter(|ch| *ch == '\n').count() as u32
    } else {
        0
    };

    // Capture parameter types, names, and mutability
    let mut param_types = Vec::new();
    let mut param_idents = Vec::new();
    let mut param_mutability = Vec::new();

    for input in inputs {
        match input {
            FnArg::Typed(pat_type) => {
                param_types.push(&pat_type.ty);
                if let Pat::Ident(PatIdent {
                    ident, mutability, ..
                }) = &*pat_type.pat
                {
                    param_idents.push(ident);
                    param_mutability.push(mutability.is_some());
                } else {
                    panic!("`#[hot]` only supports simple identifiers in parameter patterns.");
                }
            }
            FnArg::Receiver(_) => {
                panic!("`#[hot]` does not support `self` methods.");
            }
        }
    }

    // Generate correct destructuring pattern for parameters
    let destructure = param_idents
        .iter()
        .zip(param_mutability.iter())
        .map(|(ident, is_mut)| {
            if *is_mut {
                quote! { mut #ident }
            } else {
                quote! { #ident }
            }
        });
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let maybe_generics = if generics.params.is_empty() {
        quote! {}
    } else {
        quote! { ::#ty_generics }
    };

    let hot_fn = quote! {
        ::bevy_simple_subsecond_system::dioxus_devtools::subsecond::HotFn::current(#hotpatched_fn #maybe_generics)
    };

    if !hot_patch_signature && !rerun_on_hot_patch {
        let result = quote! {
            //#[cfg(any(target_family = "wasm", not(debug_assertions)))]
            //#vis fn #original_fn_name #impl_generics(#inputs) #where_clause #original_output {
            //    #block
            //}

            #[cfg(debug_assertions)]
            #[allow(unused_mut)]
            #vis fn #original_fn_name #impl_generics(#inputs) #where_clause #original_output {
                #hot_fn.call((#(#param_idents,)*))
            }


            #[cfg(debug_assertions)]
            #vis fn #hotpatched_fn #impl_generics(#inputs) #where_clause #original_output {
                #block
            }
        };
        return result.into();
    }

    let maybe_run_call = if rerun_on_hot_patch {
        quote! {
            let name = ::bevy_simple_subsecond_system::__macros_internal::IntoSystem::into_system(#original_fn_name #maybe_generics).name();
            ::bevy_simple_subsecond_system::__macros_internal::debug!("Hot-patched and rerunning system {name}");
            #hot_fn.call((world,))
        }
    } else {
        quote! {
            let name = ::bevy_simple_subsecond_system::__macros_internal::IntoSystem::into_system(#original_fn_name #maybe_generics).name();
            bevy::prelude::debug!("Hot-patched system {name}");
        }
    };

    let early_return = if is_result_unit(original_output) {
        quote! {
            return Ok(());
        }
    } else {
        quote! {
            return;
        }
    };

    let hotpatched_fn_definition = match has_single_world_param(sig) {
        WorldParam::Mut | WorldParam::Ref => quote! {
            #vis fn #hotpatched_fn #impl_generics(world: &mut ::bevy_simple_subsecond_system::__macros_internal::World) #where_clause #original_output {
                if let Some(mut reload_positions) = world.get_resource_mut::<::bevy_simple_subsecond_system::__macros_internal::__ReloadPositions>() {
                    reload_positions.insert((file!(), line!(), line!() + #newlines));
                }
                #original_wrapper_fn #maybe_generics(world)
            }
        },
        WorldParam::None => quote! {
            #vis fn #hotpatched_fn #impl_generics(world: &mut ::bevy_simple_subsecond_system::__macros_internal::World) #where_clause #original_output {
                if let Some(mut reload_positions) = world.get_resource_mut::<::bevy_simple_subsecond_system::__macros_internal::__ReloadPositions>() {
                    reload_positions.insert((file!(), line!(), line!() + #newlines));
                }
                use ::bevy_simple_subsecond_system::__macros_internal::SystemState;
                let mut __system_state: SystemState<(#(#param_types),*)> = SystemState::new(world);
                let __unsafe_world = world.as_unsafe_world_cell_readonly();

                let __validation = unsafe { SystemState::validate_param(&__system_state, __unsafe_world) };

                match __validation {
                    Ok(()) => (),
                    Err(e) => {
                        if e.skipped {
                            #early_return
                        }
                    }
                }

                let (#(#destructure),*) = __system_state.get_mut(world);
                let __result = #original_wrapper_fn(#(#param_idents),*);
                __system_state.apply(world);
                #[allow(clippy::unused_unit)]
                __result
            }
        },
    };

    let result = quote! {
        //#[cfg(any(target_family = "wasm", not(debug_assertions)))]
        //#vis fn #original_fn_name #impl_generics(#inputs) #where_clause #original_output {
        //    #block
        //}
        // Outer entry point: stable ABI, hot-reload safe
        #[cfg(debug_assertions)]
        #vis fn #original_fn_name #impl_generics(world: &mut ::bevy_simple_subsecond_system::__macros_internal::World) #where_clause #original_output {
            use std::any::Any as _;
            let type_id = #hotpatched_fn #maybe_generics.type_id();
            let contains_system = world.get_resource::<::bevy_simple_subsecond_system::__macros_internal::__HotPatchedSystems>().unwrap().0.contains_key(&type_id);
            if !contains_system {
                let hot_fn_ptr = #hot_fn.ptr_address();
                let system = move |world: &mut ::bevy_simple_subsecond_system::__macros_internal::World| {
                    let needs_update = {
                        let mut hot_patched_systems = world.get_resource_mut::<::bevy_simple_subsecond_system::__macros_internal::__HotPatchedSystems>().unwrap();
                        let mut hot_patched_system = hot_patched_systems.0.get_mut(&type_id).unwrap();
                        hot_patched_system.current_ptr = #hot_fn.ptr_address();
                        let needs_update = hot_patched_system.current_ptr != hot_patched_system.last_ptr;
                        hot_patched_system.last_ptr = hot_patched_system.current_ptr;
                        needs_update
                    };
                    if !needs_update {
                        return;
                    }
                    // TODO: we simply ignore the `Result` here, but we should be propagating it
                    let _ = {#maybe_run_call};
                };
                world.resource_mut::<::bevy_simple_subsecond_system::__macros_internal::Schedules>()
                  .add_systems(
                    ::bevy_simple_subsecond_system::__macros_internal::PreUpdate,
                    system.in_set(::bevy_simple_subsecond_system::SimpleSubsecondSystemSet::UpdateFunctionPtrs)
                );
                let system = ::bevy_simple_subsecond_system::__macros_internal::__HotPatchedSystem {
                    current_ptr: hot_fn_ptr,
                    last_ptr: hot_fn_ptr,
                };
                world.get_resource_mut::<::bevy_simple_subsecond_system::__macros_internal::__HotPatchedSystems>().unwrap().0.insert(type_id, system);
            }

            #hot_fn.call((world,))
        }

        // Hotpatched version with stable signature
        #[cfg(debug_assertions)]
        #hotpatched_fn_definition

        // Original function body moved into a standalone fn
        #[cfg(debug_assertions)]
        #vis fn #original_wrapper_fn #impl_generics(#inputs) #where_clause #original_output {
            #block
        }
    };

    result.into()
}

enum WorldParam {
    Ref,
    Mut,
    None,
}

fn has_single_world_param(sig: &syn::Signature) -> WorldParam {
    if sig.inputs.len() != 1 {
        return WorldParam::None;
    }

    let param = sig.inputs.first().unwrap();

    let pat_type = match param {
        FnArg::Typed(pt) => pt,
        _ => return WorldParam::None,
    };

    match &*pat_type.ty {
        Type::Reference(TypeReference {
            mutability, elem, ..
        }) => {
            match &**elem {
                Type::Path(type_path) => {
                    let segments = &type_path.path.segments;

                    let Some(last_segment) = segments.last().cloned() else {
                        return WorldParam::None;
                    };

                    // TODO: Make this more robust :D
                    if last_segment.ident != "World" {
                        return WorldParam::None;
                    }

                    if mutability.is_some() {
                        WorldParam::Mut
                    } else {
                        WorldParam::Ref
                    }
                }
                _ => WorldParam::None,
            }
        }
        _ => WorldParam::None,
    }
}

fn is_result_unit(output: &ReturnType) -> bool {
    match output {
        ReturnType::Default => false, // no return type, i.e., returns ()
        ReturnType::Type(_, ty) => match &**ty {
            Type::Path(TypePath { path, .. }) => {
                // Match on the outer type
                let Some(seg) = path.segments.last() else {
                    return false;
                };
                if seg.ident != "Result" {
                    return false;
                }

                // Match on the generic args: Result<(), BevyError>
                match seg.arguments {
                    syn::PathArguments::AngleBracketed(ref generics) => {
                        let args = &generics.args;

                        let Some(first) = args.first() else {
                            // Not sure this case can even happen
                            return true;
                        };

                        // Check first generic arg is ()
                        matches!(
                            first,
                            syn::GenericArgument::Type(Type::Tuple(t)) if t.elems.is_empty()
                        )
                    }
                    syn::PathArguments::Parenthesized(_) => false,
                    // TODO: This could also be a result that has a non-unit Ok variant
                    syn::PathArguments::None => true,
                }
            }
            _ => false,
        },
    }
}

/// Derive `HotPatchMigrate` and reflect it for your struct to be migrated
/// when a hot patch happens. You will also need to implement/derive and
/// reflect `Component` and `Default`.
#[proc_macro_derive(HotPatchMigrate)]
pub fn derive_hot_patch_migrate(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let expanded = quote! {
        impl ::bevy_simple_subsecond_system::migration::HotPatchMigrate for #name {
            fn current_type_id() -> ::core::any::TypeId {
                ::bevy_simple_subsecond_system::dioxus_devtools::subsecond::HotFn::current(|| ::core::any::TypeId::of::<Self>()).call(())
            }
        }
    };

    TokenStream::from(expanded)
}
