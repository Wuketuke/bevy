use crate::bevy_ecs_path;
use bevy_macro_utils::get_struct_fields;
use proc_macro::TokenStream;
use quote::{quote, ToTokens};
use syn::{parse_macro_input, DeriveInput, Index};

enum BundleFieldKind {
    Component,
    Ignore,
}

const BUNDLE_ATTRIBUTE_NAME: &str = "bundle";
const BUNDLE_ATTRIBUTE_IGNORE_NAME: &str = "ignore";
const BUNDLE_ATTRIBUTE_NO_FROM_COMPONENTS: &str = "ignore_from_components";

#[derive(Debug)]
struct BundleAttributes {
    impl_from_components: bool,
}

impl Default for BundleAttributes {
    fn default() -> Self {
        Self {
            impl_from_components: true,
        }
    }
}

struct BundleInfo {
    ast: DeriveInput,
    active_field_types: Vec<syn::Type>, // this used to be a &&
    errors: Vec<proc_macro2::TokenStream>,
    active_field_tokens: Vec<proc_macro2::TokenStream>,
    inactive_field_tokens: Vec<proc_macro2::TokenStream>,
    attributes: BundleAttributes,
    ecs_path: syn::Path,
}

impl BundleInfo {
    fn work(input: TokenStream, code: impl FnOnce(Self) -> TokenStream) -> TokenStream {
        let ast = parse_macro_input!(input as DeriveInput);
        let ecs_path = bevy_ecs_path();

        let mut errors = vec![];
        let mut attributes = BundleAttributes::default();

        for attr in &ast.attrs {
            if attr.path().is_ident(BUNDLE_ATTRIBUTE_NAME) {
                let parsing = attr.parse_nested_meta(|meta| {
                if meta.path.is_ident(BUNDLE_ATTRIBUTE_NO_FROM_COMPONENTS) {
                    attributes.impl_from_components = false;
                    return Ok(());
                }

                Err(meta.error(format!("Invalid bundle container attribute. Allowed attributes: `{BUNDLE_ATTRIBUTE_NO_FROM_COMPONENTS}`")))
            });

                if let Err(error) = parsing {
                    errors.push(error.into_compile_error());
                }
            }
        }

        let named_fields = match get_struct_fields(&ast.data, "derive(Bundle)") {
            Ok(fields) => fields,
            Err(e) => return e.into_compile_error().into(),
        };

        let mut field_kind = Vec::with_capacity(named_fields.len());

        for field in named_fields {
            let mut kind = BundleFieldKind::Component;

            for attr in field
                .attrs
                .iter()
                .filter(|a| a.path().is_ident(BUNDLE_ATTRIBUTE_NAME))
            {
                if let Err(error) = attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident(BUNDLE_ATTRIBUTE_IGNORE_NAME) {
                        kind = BundleFieldKind::Ignore;
                        Ok(())
                    } else {
                        Err(meta.error(format!(
                            "Invalid bundle attribute. Use `{BUNDLE_ATTRIBUTE_IGNORE_NAME}`"
                        )))
                    }
                }) {
                    return error.into_compile_error().into();
                }
            }

            field_kind.push(kind);
        }

        let field = named_fields
            .iter()
            .map(|field| field.ident.as_ref())
            .collect::<Vec<_>>();

        let field_type = named_fields
            .iter()
            .map(|field| &field.ty)
            .collect::<Vec<_>>();

        let mut active_field_types = Vec::new();
        let mut active_field_tokens = Vec::new();
        let mut inactive_field_tokens = Vec::new();
        for (((i, field_type), field_kind), field) in field_type
            .iter()
            .enumerate()
            .zip(field_kind.iter())
            .zip(field.iter())
        {
            let field_tokens = match field {
                Some(field) => field.to_token_stream(),
                None => Index::from(i).to_token_stream(),
            };
            match field_kind {
                BundleFieldKind::Component => {
                    active_field_types.push((*field_type).clone());
                    active_field_tokens.push(field_tokens);
                }

                BundleFieldKind::Ignore => inactive_field_tokens.push(field_tokens),
            }
        }

        let info = Self {
            ecs_path,
            ast,
            active_field_types,
            active_field_tokens,
            attributes,
            inactive_field_tokens,
            errors,
        };
        code(info)
    }
}

pub(super) fn derive_bundle(input: TokenStream) -> TokenStream {
    BundleInfo::work(
        input,
        |BundleInfo {
             ast,
             errors,
             ecs_path,
             active_field_types,
             active_field_tokens,
             ..
         }| {
            let attribute_errors = &errors;
            let struct_name = &ast.ident;
            let (impl_generics, ty_generics, where_clause) = ast.generics.split_for_impl();

            let bundle_impl = quote! {
                // SAFETY:
                // - ComponentId is returned in field-definition-order. [get_components] uses field-definition-order
                // - `Bundle::get_components` is exactly once for each member. Rely's on the Component -> Bundle implementation to properly pass
                //   the correct `StorageType` into the callback.
                #[allow(deprecated)]
                unsafe impl #impl_generics #ecs_path::bundle::Bundle for #struct_name #ty_generics #where_clause {
                    fn component_ids(
                        components: &mut #ecs_path::component::ComponentsRegistrator,
                        ids: &mut impl FnMut(#ecs_path::component::ComponentId)
                    ) {
                        #(<#active_field_types as #ecs_path::bundle::Bundle>::component_ids(components, ids);)*
                    }

                    fn get_component_ids(
                        components: &#ecs_path::component::Components,
                        ids: &mut impl FnMut(Option<#ecs_path::component::ComponentId>)
                    ) {
                        #(<#active_field_types as #ecs_path::bundle::Bundle>::get_component_ids(components, &mut *ids);)*
                    }

                    fn register_required_components(
                        components: &mut #ecs_path::component::ComponentsRegistrator,
                        required_components: &mut #ecs_path::component::RequiredComponents
                    ) {
                        #(<#active_field_types as #ecs_path::bundle::Bundle>::register_required_components(components, required_components);)*
                    }
                }
            };

            let dynamic_bundle_impl = quote! {
                #[allow(deprecated)]
                impl #impl_generics #ecs_path::bundle::DynamicBundle for #struct_name #ty_generics #where_clause {
                    type Effect = ();
                    #[allow(unused_variables)]
                    #[inline]
                    fn get_components(
                        self,
                        func: &mut impl FnMut(#ecs_path::component::StorageType, #ecs_path::ptr::OwningPtr<'_>)
                    ) {
                        #(<#active_field_types as #ecs_path::bundle::DynamicBundle>::get_components(self.#active_field_tokens, &mut *func);)*
                    }
                }
            };

            TokenStream::from(quote! {
                #(#attribute_errors)*
                #bundle_impl
                #dynamic_bundle_impl
            })
        },
    )
}

pub(super) fn derive_bundle_from_component(input: TokenStream) -> TokenStream {
    BundleInfo::work(
        input,
        |BundleInfo {
             active_field_types,
             errors,
             active_field_tokens,
             ast,
             ecs_path,
             inactive_field_tokens,
             attributes,
         }| {
            let attribute_errors = &errors;
            let struct_name = &ast.ident;
            let (impl_generics, ty_generics, where_clause) = ast.generics.split_for_impl();

            let from_components_impl = attributes.impl_from_components.then(|| quote! {
                // SAFETY:
                // - ComponentId is returned in field-definition-order. [from_components] uses field-definition-order
                #[allow(deprecated)]
                unsafe impl #impl_generics #ecs_path::bundle::BundleFromComponents for #struct_name #ty_generics #where_clause {
                    #[allow(unused_variables, non_snake_case)]
                    unsafe fn from_components<__T, __F>(ctx: &mut __T, func: &mut __F) -> Self
                    where
                        __F: FnMut(&mut __T) -> #ecs_path::ptr::OwningPtr<'_>
                    {
                        Self {
                            #(#active_field_tokens: <#active_field_types as #ecs_path::bundle::BundleFromComponents>::from_components(ctx, &mut *func),)*
                            #(#inactive_field_tokens: ::core::default::Default::default(),)*
                        }
                    }
                }
            });

            TokenStream::from(quote! {
                   #(#attribute_errors)*
                   #from_components_impl
            })
        },
    )
}
