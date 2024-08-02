use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{quote, ToTokens, TokenStreamExt};
use syn::{
    self, braced,
    parse::Parse,
    parse_macro_input, parse_quote,
    punctuated::{Pair, Punctuated},
    token::{self, Comma},
    Field, FieldMutability, Fields, FnArg, Generics, Ident, ItemEnum, ItemFn, ItemTrait, LitStr,
    Pat, Signature, Token, TraitItem, Type, Variant, Visibility,
};

#[derive(Default)]
struct InvokeBindingAttrs {
    cmd_prefix: Option<String>,
}

impl Parse for InvokeBindingAttrs {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut attrs: Self = Default::default();
        while !input.is_empty() {
            let kv: KeyValuePair = input.parse()?;
            if kv.key.as_str() == "cmd_prefix" {
                attrs.cmd_prefix = Some(kv.value)
            }
        }
        Ok(attrs)
    }
}

struct KeyValuePair {
    key: String,
    value: String,
}

impl Parse for KeyValuePair {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let key: Ident = input.parse()?;
        let _: Token![=] = input.parse()?;
        let value: LitStr = input.parse()?;
        Ok(Self {
            key: key.to_string(),
            value: value.value(),
        })
    }
}

/// Apply this to a trait, and generate an implementation for it's fns in the
/// same scope that call `invoke` using the fn name as the command
///
/// # Examples
///
/// ```ignore
/// #[allow(async_fn_in_trait)]
/// #[tauri_bindgen_rs_macros::invoke_bindings]
/// pub trait Commands {
///     async hello(name: String) -> Result<String, String>;
/// }
///
/// async fn hello_world() -> Result<String, String> {
///     hello("world".into())
/// }
/// ```
#[proc_macro_attribute]
pub fn invoke_bindings(attrs: TokenStream, tokens: TokenStream) -> TokenStream {
    let attrs = parse_macro_input!(attrs as InvokeBindingAttrs);
    let trait_item = parse_macro_input!(tokens as ItemTrait);
    let fn_items = trait_item.items.iter().fold(Vec::new(), |mut m, item| {
        if let TraitItem::Fn(fn_item) = item {
            let fields: Punctuated<Field, Token![,]> =
                Punctuated::from_iter(fn_item.sig.inputs.iter().fold(Vec::new(), |mut m, arg| {
                    let pt = match arg {
                        FnArg::Typed(pt) => pt,
                        FnArg::Receiver(_) => {
                            panic!("receiver arguments not supported");
                        }
                    };
                    let ident = match pt.pat.as_ref() {
                        Pat::Ident(pi) => Some(pi.ident.clone()),
                        _ => panic!("argument not supported"),
                    };
                    let colon_token = Some(pt.colon_token);
                    let ty = pt.ty.as_ref().clone();
                    m.push(Field {
                        attrs: Vec::new(),
                        vis: Visibility::Inherited,
                        mutability: FieldMutability::None,
                        ident,
                        colon_token,
                        ty,
                    });
                    m
                }));
            let field_names: Punctuated<Ident, Token![,]> =
                Punctuated::from_iter(fields.iter().map(|field| field.ident.clone().unwrap()));
            let fn_name = fn_item.sig.ident.to_string();
            let fn_name = attrs
                .cmd_prefix
                .clone()
                .map_or(fn_name.clone(), |prefix| prefix + fn_name.as_str());
            m.push(ItemFn {
                attrs: Vec::new(),
                vis: trait_item.vis.clone(),
                sig: fn_item.sig.clone(),
                block: parse_quote!({
                    #[derive(::serde::Serialize)]
                    #[serde(rename_all = "camelCase")]
                    struct Args {
                        #fields
                    }
                    let args = Args { #field_names };
                    let args: JsValue = ::serde_wasm_bindgen::to_value(&args).unwrap();
                    match invoke(#fn_name, args).await {
                        Ok(value) => Ok(::serde_wasm_bindgen::from_value(value).unwrap()),
                        Err(err) => Err(::serde_wasm_bindgen::from_value(err).unwrap()),
                    }
                }),
            });
        }
        m
    });
    let fn_items = ItemList { list: fn_items };
    let ret = quote! {
        #trait_item

        use wasm_bindgen::prelude::*;

        #[wasm_bindgen]
        extern "C" {
            #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], catch)]
            async fn invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;
        }

        #fn_items
    };

    TokenStream::from(ret)
}

/// # Examples
///
/// ```ignore
/// #[derive(Events, Debug, Clone, ::serde::Serialize, ::serde::Deserialize)]
/// enum Event {
///     SomethingHappened { payload: Vec<u8> },
///     SomeoneSaidHello(String),
///     NoPayload,
/// }
///
/// fn emit_event(app_handle: tauri::AppHandle, event: Event) -> anyhow::Result<()> {
///     Ok(app_handle.emit(event.event_name(), event)?)
/// }
///
/// // ...
///
/// let listener = EventBinding::SomethingHappened.listen(|event: Event| {
///     // ...
/// }).await;
/// drop(listener); // unlisten
/// ```
#[proc_macro_derive(Events)]
pub fn derive_event(tokens: TokenStream) -> TokenStream {
    let item_enum = parse_macro_input!(tokens as ItemEnum);
    let ItemEnum {
        attrs: _,
        vis,
        enum_token: _,
        ident,
        generics,
        brace_token: _,
        variants,
    } = item_enum;

    fn derive_impl_display(
        vis: Visibility,
        _generics: Generics, // TODO: support generics
        ident: Ident,
        variants: Punctuated<Variant, Comma>,
    ) -> TokenStream2 {
        let match_arms: Punctuated<TokenStream2, Comma> = variants
            .iter()
            .map(|v| -> TokenStream2 {
                let ident = ident.clone();
                let v_ident = &v.ident;
                let v_ident_str = v_ident.to_string();
                let fields: TokenStream2 = match &v.fields {
                    Fields::Unit => quote! {}.into(),
                    Fields::Unnamed(fields) => {
                        let placeholders: Punctuated<TokenStream2, Comma> = fields
                            .unnamed
                            .iter()
                            .map(|_| -> TokenStream2 { quote! { _ }.into() })
                            .collect();
                        quote! { (#placeholders) }.into()
                    }
                    Fields::Named(fields) => {
                        let placeholders: Punctuated<TokenStream2, Comma> = fields
                            .named
                            .iter()
                            .map(|f| -> TokenStream2 {
                                let ident = f.ident.as_ref().unwrap();
                                quote! { #ident: _ }.into()
                            })
                            .collect();
                        quote! { {#placeholders} }.into()
                    }
                };
                quote! {
                    #ident::#v_ident #fields => #v_ident_str
                }
                .into()
            })
            .collect();
        let ret = quote! {
            impl #ident {
                #vis fn event_name(&self) -> &'static str {
                    match self {
                        #match_arms
                    }
                }
            }
        };
        TokenStream2::from(ret)
    }

    fn derive_event_binding(
        _generics: Generics, // TODO: support generics
        ident: Ident,
        variants: Punctuated<Variant, Comma>,
    ) -> TokenStream2 {
        let event_binding_ident =
            Ident::new(&format!("{}Binding", ident.to_string()), Span::call_site());
        let variant_names: Punctuated<Ident, Comma> =
            variants.iter().map(|v| v.ident.clone()).collect();
        let variant_to_str_match_arms: Punctuated<TokenStream2, Comma> = variants
            .iter()
            .map(|v| -> TokenStream2 {
                let ident = &v.ident;
                let ident_str = ident.to_string();
                quote! {
                    #event_binding_ident::#ident => #ident_str
                }
                .into()
            })
            .collect();
        let ret = quote! {
            pub enum #event_binding_ident {
                #variant_names
            }

            impl #event_binding_ident {
                pub async fn listen<F>(&self, handler: F) -> Result<EventListener, JsValue>
                where
                    F: Fn(#ident) + 'static,
                {
                    let event_name = self.as_str();
                    EventListener::new(event_name, move |event| {
                        let event: TauriEvent<#ident> = ::serde_wasm_bindgen::from_value(event).unwrap();
                        handler(event.payload);
                    })
                    .await
                }

                fn as_str(&self) -> &str {
                    match self {
                        #variant_to_str_match_arms
                    }
                }
            }
        };
        TokenStream2::from(ret)
    }

    // TODO: break this out into another crate (it doesn't need to be in a macro)
    fn events_mod(vis: Visibility) -> TokenStream2 {
        quote! {
            use wasm_bindgen::prelude::*;

            #[wasm_bindgen]
            extern "C" {
                #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "event"], catch)]
                async fn listen(
                    event_name: &str,
                    handler: &Closure<dyn FnMut(JsValue)>,
                ) -> Result<JsValue, JsValue>;
            }

            #vis struct EventListener {
                event_name: String,
                _closure: Closure<dyn FnMut(JsValue)>,
                unlisten: js_sys::Function,
            }

            impl EventListener {
                pub async fn new<F>(event_name: &str, handler: F) -> Result<Self, JsValue>
                where
                    F: Fn(JsValue) + 'static,
                {
                    let closure = Closure::new(handler);
                    let unlisten = listen(event_name, &closure).await?;
                    let unlisten = js_sys::Function::from(unlisten);

                    tracing::trace!("EventListener created for {event_name}");

                    Ok(Self {
                        event_name: event_name.to_string(),
                        _closure: closure,
                        unlisten,
                    })
                }
            }

            impl Drop for EventListener {
                fn drop(&mut self) {
                    tracing::trace!("EventListener dropped for {}", self.event_name);
                    let context = JsValue::null();
                    self.unlisten.call0(&context).unwrap();
                }
            }

            #[derive(::serde::Deserialize)]
            struct TauriEvent<T> {
                pub payload: T,
            }
        }
    }

    let impl_display = derive_impl_display(
        vis.clone(),
        generics.clone(),
        ident.clone(),
        variants.clone(),
    );
    let event_binding = derive_event_binding(generics, ident, variants);
    let events_mod = events_mod(vis);

    let ret = quote! {
        #impl_display

        #event_binding

        #events_mod
    };
    TokenStream::from(ret)
}

struct ImplTrait {
    trait_ident: Ident,
    fns: ItemList<ItemFn>,
}

impl Parse for ImplTrait {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let fns;
        let trait_ident = input.parse()?;
        let _: Token![,] = input.parse()?;
        let _: token::Brace = braced!(fns in input);
        let fns = fns.parse()?;
        Ok(ImplTrait { trait_ident, fns })
    }
}

struct ItemList<I: ToTokens> {
    list: Vec<I>,
}

impl<I: Parse + ToTokens> Parse for ItemList<I> {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut list = Vec::new();

        while !input.is_empty() {
            let item: I = input.parse()?;
            list.push(item);
        }

        Ok(ItemList { list })
    }
}

impl<I: ToTokens> ToTokens for ItemList<I> {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        tokens.append_all(self.list.iter());
    }
}

/// Takes the name of a trait and an impl block, and emits a ghost struct that
/// implements that trait using the provided fn signatures—stripping away any
/// generics and arguments with `tauri` as the first path segment.
///
/// TODO: accept a list of arguments to ignore vs relying on the `tauri::` prefix.
///
/// # Examples
///
/// ```ignore
/// trait Commands {
///     async foo(bar: String) -> Result<(), String>;
///     async bar(foo: String) -> Result<(), String>;
/// }
///
/// // ignore this (here so the example can compile)
/// mod tauri {
///     struct State {}
/// }
///
/// tauri_bindgen_rs_macros::impl_trait!(Commands, {
///     // we'll also need a #[tauri::command] attribute here
///     async foo(state: tauri::State, bar: String) -> Result<(), String> {
///         Ok(())
///     }
///
///     // we'll also need a #[tauri::command] attribute here
///     async bar(state: tauri::State, foo: String) -> Result<(), String> {
///         Ok(())
///     }
/// });
/// ```
#[proc_macro]
pub fn impl_trait(tokens: TokenStream) -> TokenStream {
    let ImplTrait { trait_ident, fns } = parse_macro_input!(tokens as ImplTrait);

    let mut trait_fns = Vec::new();

    fn map_fn_input(mut item: Pair<FnArg, Comma>) -> Pair<FnArg, Comma> {
        let value = item.value_mut();
        if let FnArg::Typed(pt) = value {
            if let Pat::Ident(pi) = pt.pat.as_mut() {
                pi.ident = Ident::new(
                    // add an _ prefix to all fn arguments so we don't trigger unused variable warnings
                    { "_".to_string() + pi.ident.to_string().as_str() }.as_str(),
                    pi.ident.span(),
                );
            }
        }
        item
    }

    fn filter_map_fn_inputs(inputs: Punctuated<FnArg, Comma>) -> Punctuated<FnArg, Comma> {
        let tauri_ident = Ident::new("tauri", Span::call_site());
        Punctuated::from_iter(inputs.into_pairs().fold(Vec::new(), |mut m, item| {
            if let Some(tp) = match item.value() {
                FnArg::Typed(pt) => match pt.ty.as_ref() {
                    Type::Path(path) => Some(path),
                    _ => None,
                },
                _ => None,
            } {
                if let Some(s) = tp.path.segments.first() {
                    if s.ident == tauri_ident {
                        return m;
                    }
                }
            }
            m.push(map_fn_input(item));
            m
        }))
    }

    fns.list.iter().for_each(|func| {
        let sig = &func.sig;
        trait_fns.push(ItemFn {
            attrs: Vec::new(),
            vis: func.vis.clone(),
            sig: Signature {
                constness: None,
                asyncness: sig.asyncness,
                unsafety: None,
                abi: None,
                fn_token: sig.fn_token,
                generics: Default::default(),
                ident: sig.ident.clone(),
                paren_token: sig.paren_token,
                inputs: filter_map_fn_inputs(sig.inputs.clone()),
                variadic: None,
                output: sig.output.clone(),
            },
            block: parse_quote!({ todo!() }),
        });
    });

    let struct_name = Ident::new(format!("__Impl{}", trait_ident).as_str(), Span::call_site());
    let trait_fns = ItemList { list: trait_fns };

    let ret = quote! {
        struct #struct_name {}

        impl #trait_ident for #struct_name {
            #trait_fns
        }

        #fns
    };

    TokenStream::from(ret)
}
