use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{
    Data, DeriveInput, Fields, FnArg, Ident, Item, ItemFn, ItemMod, LitStr, Pat, ReturnType, Type,
    parse_macro_input, parse_quote,
};

/// Top-level `#[casper(...)]` attribute entry point that dispatches to specific handlers like `contract` or `export`.
///
/// Usage:
/// - `#[casper(export)] fn entrypoint(arg1: String, arg2: u64) { ... }`
///   Generates a `#[no_mangle] pub extern "C" fn entrypoint()` wrapper that fetches named args
///   via `casper_contract::contract_api::runtime::get_named_arg("arg")` and calls `entrypoint_impl`.
/// - `#[casper(contract)] mod name { ... }`
///   Appends a `CallBuilder` with methods for each exported function, calling `*_impl` variants.
#[proc_macro_attribute]
pub fn casper(attr: TokenStream, item: TokenStream) -> TokenStream {
    // Parse attribute as a simple path like `export` or `contract`
    let path = parse_macro_input!(attr as syn::Path);
    let kind = path
        .get_ident()
        .cloned()
        .unwrap_or_else(|| Ident::new("", proc_macro2::Span::call_site()));

    match kind.to_string().as_str() {
        "export" => export_impl(item),
        "contract" => contract_impl(item),
        _ => {
            // Fallback: return item unchanged
            item
        }
    }
}

fn export_impl(item: TokenStream) -> TokenStream {
    let input_fn = parse_macro_input!(item as ItemFn);

    // Capture original signature and name
    let _vis = &input_fn.vis;
    let sig = &input_fn.sig;
    let fn_name = &sig.ident;

    // Collect (arg_ident, arg_type) for wrapper
    let mut arg_idents: Vec<Ident> = Vec::new();
    let mut arg_types: Vec<Type> = Vec::new();
    for arg in &sig.inputs {
        match arg {
            FnArg::Receiver(_) => {
                return syn::Error::new_spanned(
                    arg,
                    "methods with self are not supported by #[casper(export)]",
                )
                .to_compile_error()
                .into();
            }
            FnArg::Typed(pat_ty) => {
                // Pattern must be an identifier
                if let Pat::Ident(pat_ident) = &*pat_ty.pat {
                    arg_idents.push(pat_ident.ident.clone());
                    arg_types.push((*pat_ty.ty).clone());
                } else {
                    return syn::Error::new_spanned(&pat_ty.pat, "unsupported pattern in argument")
                        .to_compile_error()
                        .into();
                }
            }
        }
    }

    // Determine return type
    // Determine return type and whether it's a Result<T, E>
    let (has_return, is_result) = match &sig.output {
        ReturnType::Default => (false, false),
        ReturnType::Type(_, ty) => {
            // ty: Box<Type>
            let is_result = if let Type::Path(type_path) = &**ty {
                type_path
                    .path
                    .segments
                    .last()
                    .map(|s| s.ident == "Result")
                    .unwrap_or(false)
            } else {
                false
            };
            (true, is_result)
        }
    };

    // Generate code to read args using veles_casper_contract_api::casper_contract::contract_api::runtime::get_named_arg
    let get_args = arg_idents.iter().zip(arg_types.iter()).map(|(ident, ty)| {
        let name_str = ident.to_string();
        quote! {
            let #ident: #ty = veles_casper_contract_api::casper_contract::contract_api::runtime::get_named_arg(#name_str);
        }
    });

    let call_args = arg_idents.iter();

    let handle_ret = if is_result {
        quote! {
            {
                let ret: core::result::Result<_, _> = super::#fn_name( #(#call_args),* );

                match ret {
                    Ok(value) => value,
                    Err(err) => veles_casper_contract_api::casper_contract::contract_api::runtime::revert(err),
                }
            }
        }
    } else {
        quote! {
            super::#fn_name( #(#call_args),* )
        }
    };

    let call_stmt = if has_return {
        quote! {
            let ret = #handle_ret;
           veles_casper_contract_api::casper_contract::contract_api::runtime::ret(veles_casper_contract_api::casper_types::CLValue::from_t(ret).unwrap());
        }
    } else {
        quote! { let () = #handle_ret; }
    };

    let mod_name = format_ident!("{}", fn_name);

    let get_args_again = get_args.clone();

    let expanded = quote! {
       // Generated extern shim

        #[allow(clippy::too_many_arguments)]
        #input_fn

        #[doc(hidden)]
        #[allow(unexpected_cfgs)]
        pub mod #mod_name {
            use super::*;

            pub const NAME: &'static str = stringify!(#fn_name);

            #[cfg(not(feature = "as_dependency"))]
            #[unsafe(export_name = stringify!(#fn_name))]
            extern "C" fn entry_point() {
                veles_casper_contract_api::macro_support::set_panic_hook();

                #(#get_args)*
                { #call_stmt }
            }

            #[cfg(feature = "as_dependency")]
            pub fn entry_point() {
                #(#get_args_again)*
                { #call_stmt }
            }


            pub struct Args {
                #(
                    pub #arg_idents: #arg_types,
                )*
            }

            impl veles_casper_contract_api::macro_support::IntoRuntimeArgs for Args {
                fn into_runtime_args(self) -> veles_casper_contract_api::casper_types::RuntimeArgs {
                    let mut runtime_args = veles_casper_contract_api::casper_types::RuntimeArgs::new();
                    #(
                        runtime_args.insert(stringify!(#arg_idents), self.#arg_idents).unwrap();
                    )*
                    runtime_args
                }
            }

            pub fn call_contract<T:  veles_casper_contract_api::casper_types::CLTyped + veles_casper_contract_api::casper_types::bytesrepr::FromBytes>(contract_hash: veles_casper_contract_api::casper_types::contracts::ContractHash, args: Args) -> T {
                veles_casper_contract_api::casper_contract::contract_api::runtime::call_contract::<T>(
                    contract_hash,
                    NAME,
                    veles_casper_contract_api::macro_support::IntoRuntimeArgs::into_runtime_args(args),
                )
            }
        }


    };

    TokenStream::from(expanded)
}

fn contract_impl(item: TokenStream) -> TokenStream {
    let input_mod = parse_macro_input!(item as ItemMod);

    let vis = &input_mod.vis;
    let mod_ident = &input_mod.ident;
    let (brace, content) = match &input_mod.content {
        Some((_, items)) => (true, items.clone()),
        None => (false, Vec::new()),
    };

    // Collect exported functions to generate CallBuilder methods and an entry_points() function
    let mut client_methods = Vec::new();
    let mut entry_builders = Vec::new();
    let mut macro_symbols = Vec::new();
    // let mut export_symbols = Vec::new();

    if brace {
        for it in &content {
            if let Item::Fn(func) = it {
                let mut is_export = false;
                for attr in &func.attrs {
                    if let syn::Meta::List(list) = &attr.meta
                        && let Some(last) = list.path.segments.last()
                    {
                        if last.ident == "casper"
                            && let Ok(p) = syn::parse2::<syn::Path>(list.tokens.clone())
                        {
                            if p.is_ident("export") {
                                is_export = true;
                                break;
                            }
                        } else if last.ident == "unsafe" {
                            let s = list.tokens.to_string();
                            if s.contains("casper") && s.contains("export") {
                                is_export = true;
                                break;
                            }
                        }
                    }
                }
                if is_export {
                    // Build method sig mirroring function
                    let name = func.sig.ident.clone();

                    macro_symbols.push(quote! {
                        #name
                    });

                    let mut arg_pats: Vec<Ident> = Vec::new();
                    let mut arg_types: Vec<Type> = Vec::new();
                    for arg in &func.sig.inputs {
                        match arg {
                            FnArg::Receiver(_) => {
                                // skip methods with self
                            }
                            FnArg::Typed(pat_ty) => {
                                if let Pat::Ident(pat_ident) = &*pat_ty.pat {
                                    arg_pats.push(pat_ident.ident.clone());
                                    arg_types.push((*pat_ty.ty).clone());
                                }
                            }
                        }
                    }

                    let ret_ty_tokens = match &func.sig.output {
                        ReturnType::Default => quote! { () },
                        ReturnType::Type(_, ty) => {
                            // If the return type is Result<Ok, Err>, use Ok; otherwise use the whole type.
                            let ok_type = if let Type::Path(type_path) = &**ty {
                                type_path.path.segments.last().and_then(|seg| {
                                    if seg.ident == "Result"
                                        && let syn::PathArguments::AngleBracketed(args) =
                                            &seg.arguments
                                        && let Some(syn::GenericArgument::Type(ok_ty)) =
                                            args.args.first()
                                    {
                                        return Some(quote! { #ok_ty });
                                    }

                                    None
                                })
                            } else {
                                None
                            };
                            ok_type.unwrap_or_else(|| quote! { #ty })
                        }
                    };

                    let sym_name = format_ident!("{}", name);
                    client_methods.push(quote! {
                        pub fn #name(&self, #(#arg_pats: #arg_types),*) -> #ret_ty_tokens {
                            let args = #mod_ident::#sym_name::Args {
                                #(
                                    #arg_pats,
                                )*
                            };

                            #mod_ident::#sym_name::call_contract::<#ret_ty_tokens>(
                                self.0,
                                args,
                            )
                        }
                    });

                    // Build tokens to populate EntryPoints in generated function using CLTyped
                    let name_lit =
                        syn::LitStr::new(&name.to_string(), proc_macro2::Span::call_site());
                    let params_list = arg_pats.iter().zip(arg_types.iter()).map(|(id, ty)| {
                        let id_lit = syn::LitStr::new(&id.to_string(), proc_macro2::Span::call_site());
                        quote! { veles_casper_contract_api::casper_types::Parameter::new(#id_lit, <#ty as veles_casper_contract_api::casper_types::CLTyped>::cl_type()) }
                    });
                    let ret_cl = match &func.sig.output {
                        ReturnType::Default => {
                            quote! { veles_casper_contract_api::casper_types::CLType::Unit }
                        }
                        ReturnType::Type(_, ty) => {
                            // Try to extract the Ok type from Result<Ok, Err>, otherwise fall back to the whole type.
                            let ok_type_cl = if let Type::Path(type_path) = &**ty {
                                type_path
                                    .path
                                    .segments
                                    .last()
                                    .and_then(|seg| {
                                        if seg.ident == "Result"
                                           && let syn::PathArguments::AngleBracketed(args) = &seg.arguments
                                                && let Some(syn::GenericArgument::Type(ok_ty)) = args.args.first() {
                                                    return Some(quote! { <#ok_ty as veles_casper_contract_api::casper_types::CLTyped>::cl_type() });
                                                }


                                        None
                                    })
                            } else {
                                None
                            };

                            ok_type_cl.unwrap_or_else(|| quote! { <#ty as veles_casper_contract_api::casper_types::CLTyped>::cl_type() })
                        }
                    };
                    entry_builders.push(quote! {
                        veles_casper_contract_api::casper_types::EntityEntryPoint::new(
                            #name_lit,
                            alloc::vec![ #(#params_list),* ],
                            #ret_cl,
                            veles_casper_contract_api::casper_types::EntryPointAccess::Public,
                            veles_casper_contract_api::casper_types::EntryPointType::Called,
                            veles_casper_contract_api::casper_types::EntryPointPayment::Caller,
                        )
                    });
                }
            }
        }
    }

    let builder_struct = quote! {
        pub struct Client(veles_casper_contract_api::casper_types::contracts::ContractHash);

        impl Client {
            pub fn new(contract_hash: veles_casper_contract_api::casper_types::contracts::ContractHash) -> Self {
                Self(contract_hash)
            }
        }

        impl Client {
            #(#client_methods)*
        }
    };

    // Generate entry_points() function that constructs EntryPoints using CLTyped
    let entrypoints_fn = quote! {
        pub fn entry_points_vec() -> alloc::vec::Vec<veles_casper_contract_api::casper_types::EntityEntryPoint> {
            let mut entry_points = alloc::vec::Vec::new();
            #(entry_points.push(#entry_builders);)*
            entry_points
        }

        pub fn entry_points() -> veles_casper_contract_api::casper_types::EntryPoints {
            entry_points_vec().into()
        }
    };

    let enumerate_symbols_macro_name = format_ident!("enumerate_{}_symbols", mod_ident);
    let export_symbols_macro_name = format_ident!("export_{}_symbols", mod_ident);

    let output = if brace {
        let items = content;
        quote! {
            #vis mod #mod_ident {
                #(#items)*
                #builder_struct
                #entrypoints_fn

                pub struct Contract(());


                #[macro_export]
                macro_rules! #enumerate_symbols_macro_name {
                    ($mac:ident) => {
                        $mac! {
                            #(#macro_symbols)*
                        }
                    };
                }

                #[macro_export]
                macro_rules! #export_symbols_macro_name {
                    () => {
                        #(
                            #[cfg(not(feature = "as_dependency"))]
                            const _: () = {
                                #[unsafe(export_name = stringify!(#macro_symbols))]
                                extern "C" fn func() {
                                    casper_contract_extras::#mod_ident::#mod_ident::#macro_symbols::entry_point();
                                }
                            };
                        )*
                    };
                }
            }
        }
    } else {
        // For "mod name;" style, we can't append items here. Return unchanged.
        quote! { #vis mod #mod_ident; }
    };

    TokenStream::from(output)
}

#[proc_macro_derive(CasperMessage, attributes(casper))]
pub fn derive_casper_event(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    let name = &input.ident;

    // Look for #[casper(topic_name = "foobar")]
    let mut topic_str: Option<String> = None;
    for attr in &input.attrs {
        if attr.path().is_ident("casper") {
            let _ = attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("topic_name") {
                    let value = meta.value()?;
                    let lit: syn::LitStr = value.parse()?;
                    topic_str = Some(lit.value());
                }
                Ok(())
            });
        }
    }

    let topic_lit = syn::LitStr::new(
        &topic_str.unwrap_or_else(|| name.to_string()),
        proc_macro2::Span::call_site(),
    );

    let computed_topic_name_hash = compute_blake2b256(topic_lit.value().as_bytes());

    let expanded = quote! {
        impl veles_casper_contract_api::macro_support::CasperMessage for #name {
            const TOPIC_NAME: &'static str = #topic_lit;
            const TOPIC_NAME_HASH: [u8; 32] = [#(#computed_topic_name_hash),*];

            fn into_message_payload(self) -> Result<veles_casper_contract_api::casper_types::contract_messages::MessagePayload, veles_casper_contract_api::casper_types::ApiError> {
                let bytes = self.into_bytes()?;
                let payload = veles_casper_contract_api::casper_types::contract_messages::MessagePayload::Bytes(bytes.into());
                Ok(payload)
            }
        }
    };

    TokenStream::from(expanded)
}

#[proc_macro_derive(DictionaryKey)]
pub fn derive_dictionary_key(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let ident = &input.ident;

    let fields = match &input.data {
        Data::Struct(data) => &data.fields,
        _ => {
            return syn::Error::new_spanned(
                &input,
                "DictionaryKey can only be derived for structs",
            )
            .to_compile_error()
            .into();
        }
    };

    let accessors: Vec<proc_macro2::TokenStream> = match fields {
        Fields::Named(named) => named
            .named
            .iter()
            .map(|f| {
                let name = f.ident.as_ref().unwrap();
                quote! { &self.#name }
            })
            .collect(),
        Fields::Unnamed(unnamed) => unnamed
            .unnamed
            .iter()
            .enumerate()
            .map(|(idx, _)| {
                let index = syn::Index::from(idx);
                quote! { &self.#index }
            })
            .collect(),
        Fields::Unit => Vec::new(),
    };

    let orig_generics = input.generics.clone();
    let (_ty_impl_generics, ty_generics, where_clause) = orig_generics.split_for_impl();

    let mut impl_generics = orig_generics.clone();
    impl_generics.params.insert(0, parse_quote!('dict));
    let (impl_generics, _, _) = impl_generics.split_for_impl();

    let r#gen = if accessors.is_empty() {
        quote! {
            impl #impl_generics veles_casper_contract_api::collections::dictionary_key::DictionaryKey<'dict> for #ident #ty_generics #where_clause {
                fn dictionary_key(&'dict self) -> alloc::borrow::Cow<'dict, str> {
                    alloc::borrow::Cow::Borrowed("")
                }
            }
        }
    } else if accessors.len() == 1 {
        let acc = &accessors[0];
        quote! {
            impl #impl_generics veles_casper_contract_api::collections::dictionary_key::DictionaryKey<'dict> for #ident #ty_generics #where_clause {
                fn dictionary_key(&'dict self) -> alloc::borrow::Cow<'dict, str> {
                    use veles_casper_contract_api::collections::dictionary_key::DictionaryKey as _;
                    (#acc).dictionary_key()
                }
            }
        }
    } else {
        quote! {
            impl #impl_generics veles_casper_contract_api::collections::dictionary_key::DictionaryKey<'dict> for #ident #ty_generics #where_clause {
                // Build aggregated dictionary key string
                fn dictionary_key(&'dict self) -> alloc::borrow::Cow<'dict, str> {
                    use veles_casper_contract_api::collections::dictionary_key::DictionaryKey as _;
                    let parts = [#((#accessors).dictionary_key()),*];
                    let separators = parts.len().saturating_sub(1);
                    let capacity = parts.iter().map(|part| part.len()).sum::<usize>() + separators;
                    let mut combined = alloc::string::String::with_capacity(capacity);
                    for (idx, part) in parts.iter().enumerate() {
                        if idx != 0 {
                            combined.push(':');
                        }
                        combined.push_str(part);
                    }
                    alloc::borrow::Cow::Owned(combined)
                }
            }
        }
    };

    TokenStream::from(r#gen)
}

#[proc_macro]
pub fn blake2b256(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as LitStr);
    let bytes = input.value();

    let hash = compute_blake2b256(bytes.as_bytes());

    TokenStream::from(quote! {
        [ #(#hash),* ]
    })
}

pub(crate) fn compute_blake2b256(bytes: &[u8]) -> [u8; 32] {
    let mut context = blake2_rfc::blake2b::Blake2b::new(32);
    context.update(bytes);
    context.finalize().as_bytes().try_into().unwrap()
}
