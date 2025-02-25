use std::iter::zip;

use convert_case::{Case, Casing};
use proc_macro::TokenStream;
use proc_macro2::Ident;
use quote::{format_ident, quote};
use syn::{
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::Punctuated,
    token::Comma,
    Expr, LitBool, Path, PathSegment, Token, Visibility,
};

struct Features {
    visibility: Visibility,
    name: Ident,
    features: Punctuated<Feature, Token![,]>,
    state_name: Ident,
    state_builder_name: Ident,
}

impl Features {
    fn names(&self, case: Case) -> impl Iterator<Item = Ident> + use<'_> {
        self.features
            .pairs()
            .map(move |f| format_ident!("{}", f.value().name.to_string().to_case(case)))
    }

    fn default_fns(&self) -> proc_macro2::TokenStream {
        let mut functions = proc_macro2::TokenStream::new();

        for feature in &self.features {
            let function_name =
                format_ident!("default_{}", feature.name.to_string().to_case(Case::Snake));
            let default = feature.default.clone();
            functions.extend(quote::quote! {
                pub fn #function_name() -> bool {
                    #default
                }
            })
        }

        functions
    }

    fn builder_fns(&self) -> proc_macro2::TokenStream {
        let mut functions = proc_macro2::TokenStream::new();

        for feature in &self.features {
            let function_name = format_ident!("{}", feature.name.to_string().to_case(Case::Snake));
            functions.extend(quote::quote! {
                pub fn #function_name(mut self, value: bool) -> Self {
                    self.state.#function_name = value;
                    self
                }
            })
        }

        functions
    }

    fn default_impl(&self) -> proc_macro2::TokenStream {
        let mut fields = proc_macro2::TokenStream::new();

        for name in self.names(Case::Snake) {
            let default_fn = format_ident!("default_{}", name);
            fields.extend(quote::quote! {
                #name: Self::#default_fn(),
            })
        }

        let features_state = format_ident!("{}State", &self.name);
        quote! {
            impl Default for #features_state {
                fn default() -> Self {
                    Self {
                        #fields
                    }
                }
            }
        }
    }

    fn as_feature_and_feature_set_impls(&self) -> proc_macro2::TokenStream {
        let features_name = &self.name;

        let mut branches = proc_macro2::TokenStream::new();
        for (variant_name, field_name) in zip(self.names(Case::Pascal), self.names(Case::Snake)) {
            branches.extend(quote::quote! {
                #features_name::#variant_name => self.#field_name,
            })
        }

        let features_state = format_ident!("{}State", &self.name);
        quote! {
            impl ::conspiracy::feature_control::AsFeature for #features_state {
                type Feature = #features_name;

                #[inline]
                fn as_feature(&self, feature: #features_name) -> bool {
                    match feature {
                        #branches
                    }
                }
            }

            impl ::conspiracy::feature_control::FeatureSet for #features_name {
                type State = #features_state;
            }
        }
    }
}

struct Feature {
    name: Ident,
    default: LitBool,
}

impl Parse for Feature {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let name: Ident = input.parse()?;
        input.parse::<Token![=>]>()?;
        let default: LitBool = input.parse()?;
        Ok(Feature { name, default })
    }
}

impl Parse for Features {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let visibility: Visibility = input.parse()?;
        input.parse::<Token![enum]>()?;
        let name: Ident = input.parse()?;
        let content;
        syn::braced!(content in input);
        let features = content.parse_terminated(Feature::parse, Comma)?;
        let state_name = format_ident!("{}State", name);
        let state_builder_name = format_ident!("{}Builder", state_name);

        Ok(Features {
            visibility,
            name,
            features,
            state_name,
            state_builder_name,
        })
    }
}

pub(super) fn declare_features(input: TokenStream) -> TokenStream {
    let features = parse_macro_input!(input as Features);

    let features_enum = make_features_enum(&features);
    let features_state_struct = make_features_state_struct(&features);
    let features_state_default_impl = features.default_impl();
    let as_feature_impl = features.as_feature_and_feature_set_impls();
    let builder = make_builder(&features);

    TokenStream::from(quote! {
        #features_enum
        #features_state_struct
        #features_state_default_impl
        #as_feature_impl
        #builder
    })
}

fn make_features_enum(features: &Features) -> proc_macro2::TokenStream {
    let vis = &features.visibility;
    let name = &features.name;
    let variants = features.names(Case::Pascal);
    let state_name = &features.state_name;
    let state_builder_name = &features.state_builder_name;

    quote! {
        #vis enum #name {
            #(#variants),*
        }

        impl #name {
            pub fn builder() -> #state_builder_name {
                #state_name::builder()
            }
        }
    }
}

fn make_features_state_struct(features: &Features) -> proc_macro2::TokenStream {
    let vis = &features.visibility;
    let state_name = &features.state_name;
    let state_builder_name = &features.state_builder_name;

    let feature_names = features.names(Case::Snake);
    let default_fns = features.default_fns();

    quote! {
        #[derive(::serde::Serialize, ::serde::Deserialize, Debug, PartialEq)]
        #vis struct #state_name {
            #(#feature_names: bool),*
        }

        impl #state_name {
            pub fn builder() -> #state_builder_name {
                #state_builder_name::new()
            }

            #default_fns
        }

    }
}

fn make_builder(features: &Features) -> proc_macro2::TokenStream {
    let vis = &features.visibility;
    let state_name = format_ident!("{}State", features.name);
    let builder_name = format_ident!("{}Builder", state_name);
    let builder_fns = features.builder_fns();

    quote! {
        #vis struct #builder_name {
            state: #state_name,
        }

        impl #builder_name {
            pub fn new() -> Self {
                Self {
                    state: #state_name::default()
                }
            }

            pub fn build(self) -> #state_name {
                self.state
            }

            #builder_fns
        }

    }
}

pub(super) fn feature_enabled(input: TokenStream) -> TokenStream {
    let variant_path = parse_macro_input!(input as Path);
    let associated_state_path = get_associated_state_path(variant_path.clone());

    use_default_in_cfg_test(
        &variant_path,
        &associated_state_path,
        quote! {
            unsafe {
                let state = ::conspiracy::feature_control::macro_targets::feature_state_unchecked::<#associated_state_path>();
                ::conspiracy::feature_control::AsFeature::as_feature(&*state, #variant_path)
            }
        },
    )
}

fn get_associated_state_path(variant_path: Path) -> Path {
    let mut feature_state_path = variant_path;
    let _variant = feature_state_path.segments.pop().unwrap();
    let enum_name = feature_state_path.segments.pop().unwrap();

    let feature_state_ident = format_ident!("{}State", enum_name.value().ident.to_string());
    let feature_state_segment = PathSegment {
        ident: feature_state_ident,
        arguments: syn::PathArguments::None,
    };

    feature_state_path.segments.push(feature_state_segment);
    feature_state_path
}

fn use_default_in_cfg_test(
    variant: &Path,
    feature_state: &Path,
    stream: proc_macro2::TokenStream,
) -> TokenStream {
    let enabled_or_default = feature_enable_or_default_inner(variant, feature_state);
    TokenStream::from(quote! {
        {
            #[cfg(test)]
            {
                #enabled_or_default
            }
            #[cfg(not(test))]
            {
                #stream
            }
        }
    })
}

struct FeatureVariantOr {
    path: Path,
    default: Expr,
}

impl Parse for FeatureVariantOr {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let path = input.parse()?;
        let _: Token![,] = input.parse()?;
        let default = input.parse()?;

        Ok(FeatureVariantOr { path, default })
    }
}

pub(super) fn feature_enabled_or_default(input: TokenStream) -> TokenStream {
    let variant_path = parse_macro_input!(input as Path);
    let feature_state_path = get_associated_state_path(variant_path.clone());

    TokenStream::from(feature_enable_or_default_inner(
        &variant_path,
        &feature_state_path,
    ))
}

fn feature_enable_or_default_inner(
    variant: &Path,
    feature_state: &Path,
) -> proc_macro2::TokenStream {
    let call_field_default_fn = generate_call_field_default_fn(variant, feature_state);
    quote! {
        unsafe {
            match ::conspiracy::feature_control::macro_targets::try_feature_state::<#feature_state>() {
                Ok(state) => ::conspiracy::feature_control::AsFeature::as_feature(&*state, #variant),
                Err(_) => {
                    #call_field_default_fn
                },
            }
        }
    }
}

fn generate_call_field_default_fn(
    variant: &Path,
    feature_state: &Path,
) -> proc_macro2::TokenStream {
    let variant_as_field_default_fn = format_ident!(
        "default_{}",
        variant
            .segments
            .last()
            .map(|v| v.to_owned().ident)
            .expect("Named variant not found")
            .to_string()
            .to_lowercase()
    );

    quote! {
        <#feature_state>::#variant_as_field_default_fn()
    }
}

pub(super) fn feature_enabled_or(input: TokenStream) -> TokenStream {
    let parsed_input = parse_macro_input!(input as FeatureVariantOr);
    let variant = parsed_input.path.clone();
    let feature_state = get_associated_state_path(parsed_input.path);
    let default = parsed_input.default;

    TokenStream::from(quote! {
        unsafe {
            match ::conspiracy::feature_control::macro_targets::try_feature_state::<#feature_state>() {
                Ok(state) => ::conspiracy::feature_control::AsFeature::as_feature(&*state, #variant),
                Err(_) => #default,
            }
        }
    })
}

pub(super) fn try_feature_enabled(input: TokenStream) -> TokenStream {
    let variant_path = parse_macro_input!(input as Path);
    let feature_state_path = get_associated_state_path(variant_path.clone());

    TokenStream::from(quote! {
        unsafe {
            ::conspiracy::feature_control::macro_targets::try_feature_state::<#feature_state_path>()
                .map(|state| ::conspiracy::feature_control::AsFeature::as_feature(&*state, #variant_path))
        }
    })
}
