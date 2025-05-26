use proc_macro::TokenStream as LegacyTokenStream;
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::{
    braced,
    parse::{discouraged::Speculative, Parse, ParseStream},
    parse_macro_input, parse_quote,
    punctuated::Punctuated,
    token,
    token::{Colon, Pub},
    Attribute, Field, FieldMutability, Ident, Token, Type, Variant, Visibility,
};

use crate::common::{
    extract_conspiracy_attributes, restart_required_single_field_comparison, ConspiracyAttribute,
};
use crate::config::NestableVariant::Variant;

fn restart_required(input: &mut NestableStruct) -> TokenStream {
    let comparison = build_restart_comparison(input);
    let ty = &input.ty;

    quote! {
        impl ::conspiracy::config::RestartRequired for #ty {
            // This is effectively a specialization of PartialEq, which is inlined in derive
            // generated impls so we do the same here.
            #[inline]
            fn restart_required(&self, other: &Self) -> bool {
                #comparison
            }
        }
    }
}

fn build_restart_comparison(input: &mut NestableStruct) -> TokenStream {
    let mut lineage = Vec::new();
    let mut comparisons = Vec::new();
    build_restart_comparison_for_struct(&mut lineage, &mut comparisons, input);

    if comparisons.is_empty() {
        // If no fields were marked restart required, then a restart is never required
        quote! { false }
    } else {
        quote! { #(#comparisons)||* }
    }
}

fn build_restart_comparison_for_struct(
    lineage: &mut Vec<Ident>,
    output: &mut Vec<TokenStream>,
    item: &mut NestableStruct,
) {
    for field in item.fields.iter_mut() {
        match field {
            NestableField::NestedStruct((field, nested_struct)) => {
                build_restart_comparison_for_field(lineage, output, field);

                lineage.push(field.ident.clone().expect("All fields must be named"));
                build_restart_comparison_for_struct(lineage, output, nested_struct);
                lineage.pop();
            }
            NestableField::NestedEnum((field, _)) => {
                build_restart_comparison_for_field(lineage, output, field);
            }
            NestableField::Field(field) => {
                build_restart_comparison_for_field(lineage, output, field)
            }
        }
    }
}

fn build_restart_comparison_for_field(
    lineage: &mut Vec<Ident>,
    output: &mut Vec<TokenStream>,
    field: &mut Field,
) {
    if let Some(attr) = extract_conspiracy_attributes(&mut field.attrs) {
        match attr {
            ConspiracyAttribute::Restart => output.push(comparison_for_field(lineage, field)),
        }
    }
}

fn comparison_for_field(lineage: &mut Vec<Ident>, field: &Field) -> TokenStream {
    let field_name = field.ident.as_ref().expect("All fields must be named");
    restart_required_single_field_comparison(if lineage.is_empty() {
        quote! { #field_name }
    } else {
        quote! { #(#lineage).*.#field_name }
    })
}

pub(super) fn config_struct(input: LegacyTokenStream) -> LegacyTokenStream {
    let input = parse_macro_input!(input as NestableStruct);
    LegacyTokenStream::from(config_struct_impl(input))
}

fn config_struct_impl(mut input: NestableStruct) -> TokenStream {
    let mut output = restart_required(&mut input);
    output.extend(generate_compact_struct(&input));
    output.extend(generate_config_structs(input, &mut vec![]));
    output
}

fn compact_ty_name(ty: &Type) -> Ident {
    format_ident!(
        "Compact{}",
        Ident::new(&quote! { #ty }.to_string(), Span::call_site())
    )
}

fn generate_compact_struct(input: &NestableStruct) -> TokenStream {
    let mut output = TokenStream::new();
    let ty = &input.ty;
    let compact_ty = compact_ty_name(ty);

    let fields = input
        .fields
        .iter()
        .map(|field| {
            let field = match field {
                NestableField::NestedStruct((field, nested_struct)) => {
                    output.extend(generate_compact_struct(nested_struct));
                    let mut field = field.clone();
                    field.ty = ident_to_type(compact_ty_name(&nested_struct.ty));
                    field
                }
                NestableField::Field(field) => field.clone(),
                NestableField::NestedEnum((field, _)) => field.clone(),
            };

            Field {
                attrs: vec![],
                vis: Visibility::Public(Pub::default()),
                mutability: FieldMutability::None,
                ident: field.ident.clone(),
                colon_token: Some(Colon::default()),
                ty: field.ty,
            }
        })
        .collect::<Vec<Field>>()
        .into_iter();

    output.extend(quote! {
        pub struct #compact_ty {
            #(#fields),*
        }
    });

    let arcified_fields = input.fields.iter().map(|field| match field {
        NestableField::Field(field) => {
            let ident = field.ident.clone();
            quote! { #ident: self.#ident }
        }
        NestableField::NestedStruct((field, _)) => {
            let ident = field.ident.clone();
            quote! { #ident: self.#ident.arcify() }
        }
        NestableField::NestedEnum((field, _)) => {
            let ident = field.ident.clone();
            quote! { #ident: self.#ident.arcify() }
        }
    });

    output.extend(quote! {
        impl #compact_ty {
            // This isn't inlined because it's only intended to be used under test
            pub fn arcify(self) -> std::sync::Arc<#ty> {
                std::sync::Arc::new(#ty {
                    #(#arcified_fields),*
                })
            }
        }
    });

    output
}

fn generate_config_structs(input: NestableStruct, lineage: &mut Vec<(Ident, Type)>) -> TokenStream {
    let mut output = TokenStream::new();
    let fields = input
        .fields
        .iter()
        .map(|config_field| match config_field {
            NestableField::NestedStruct((field, nested)) => {
                lineage.push((
                    field
                        .ident
                        .clone()
                        .expect("At this stage, only named fields can be present"),
                    input.ty.clone(),
                ));
                output.extend(impl_as_field_for_lineage(lineage, nested));
                output.extend(generate_config_structs((*nested).clone(), lineage));
                lineage.pop();
                field
            }
            NestableField::Field(field) => field,
            NestableField::NestedEnum((field, nested)) => {
                // Because an enum varies, there is no canonical representation for nesting
                // operations. It depends on which variant is present at runtime. However, we still
                // potentially have further nested structs. For features to apply to those relevant
                // subsets, we must fork off code generation as a new root.
                output.extend(recurse_for_variants(nested));

                field
            }
        })
        .cloned()
        .collect::<Vec<Field>>()
        .into_iter();

    let attrs = input.attrs;
    let vis = input.vis;
    let struct_token = input.struct_token;
    let ty = input.ty;

    output.extend(quote! {
        #[derive(Clone, PartialEq)]
        #(#attrs)*
        #vis #struct_token #ty {
            #(#fields),*
        }
    });

    let compact_ty = compact_ty_name(&ty);
    let compacted_fields = input.fields.iter().map(|field| match field {
        NestableField::NestedStruct((field, _)) => {
            let ident = field.ident.clone();
            quote! { #ident: (*self.#ident).clone().compact() }
        }
        NestableField::Field(field) => {
            let ident = field.ident.clone();
            quote! { #ident: self.#ident.clone() }
        }
        NestableField::NestedEnum((field, _)) => {
            let ident = field.ident.clone();
            quote! { #ident: (*self.#ident).clone().compact() }
        }
    });

    output.extend(quote! {
        impl #ty {
            // This isn't inlined because it's only intended to be used under test
            pub fn compact(&self) -> #compact_ty {
                #compact_ty {
                    #(#compacted_fields),*
                }
            }
        }
    });

    output
}

/// Because an enum varies, there is no canonical representation for nesting operations. It depends
/// on which variant is present at runtime. However, we still potentially have further nested
/// structs. For features to apply to those relevant subsets, we must fork off code generation as a
/// new root.
fn recurse_for_variants(nested: &NestableEnum) -> TokenStream {
    let mut output = TokenStream::new();

    for variant in nested.variants.iter() {
        match variant {
            NestableVariant::NestedStruct((_, nested_struct)) => {
                output.extend(config_struct_impl(nested_struct.clone()));
            }
            NestableVariant::NestedEnum((_, nested_enum)) => {
                output.extend(recurse_for_variants(nested_enum));
            }
            NestableVariant::Variant(_) => {}
        }
    }

    output
}

fn impl_as_field_for_lineage(lineage: &[(Ident, Type)], nested: &NestableStruct) -> TokenStream {
    let mut output = TokenStream::new();

    for i in (0..lineage.len()).rev() {
        output.extend(impl_as_field(&lineage[i..], nested.ty.clone()));
    }

    output
}

fn impl_as_field(lineage: &[(Ident, Type)], child_ty: Type) -> TokenStream {
    let root_ty = lineage[0].1.clone();
    let lineage = lineage.iter().map(|ancestor| ancestor.0.clone());

    let fields = quote! {
        #(#lineage).*
    };

    quote! {
        impl ::conspiracy::config::AsField<#child_ty> for #root_ty {
            // One-liner, no reason not to inline
            #[inline]
            fn share(&self) -> std::sync::Arc<#child_ty> {
                self.#fields.clone()
            }
        }
    }
}

#[derive(Clone)]
struct NestableStruct {
    attrs: Vec<Attribute>,
    vis: Visibility,
    struct_token: Token![struct],
    ty: Type,
    _brace_token: token::Brace,
    fields: Punctuated<NestableField, Token![,]>,
}

#[derive(Clone)]
struct NestableEnum {
    attrs: Vec<Attribute>,
    vis: Visibility,
    enum_token: Token![enum],
    _brace_token: token::Brace,
    variants: Punctuated<NestableVariant, Token![,]>,
}

#[derive(Clone)]
enum NestableVariant {
    NestedStruct((Variant, NestableStruct)),
    NestedEnum((Variant, NestableEnum)),
    Variant(Variant),
}

#[derive(Clone)]
enum NestableField {
    NestedStruct((Field, NestableStruct)),
    NestedEnum((Field, NestableEnum)),
    Field(Field),
}

impl Parse for NestableStruct {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let raw_fields;
        Ok(NestableStruct {
            attrs: input.call(Attribute::parse_outer)?,
            vis: input.parse()?,
            struct_token: input.parse()?,
            ty: ident_to_type(input.parse()?),
            _brace_token: braced!(raw_fields in input),
            fields: raw_fields.parse_terminated(NestableField::parse, Token![,])?,
        })
    }
}

impl Parse for NestableEnum {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let raw_fields;
        Ok(NestableEnum {
            attrs: input.call(Attribute::parse_outer)?,
            vis: input.parse()?,
            enum_token: input.parse()?,
            _brace_token: braced!(raw_fields in input),
            variants: raw_fields.parse_terminated(NestableVariant::parse, Token![,])?,
        })
    }
}

impl Parse for NestableField {
    // Here we mostly mirror [`syn::data::Field::parse_named`] after the initial pass.
    fn parse(input: ParseStream) -> syn::Result<Self> {
        // First check if input is just an ordinary field. If parsing fails, then attempt to parse
        // our syntax. This is simpler than on the fly determining which type of input we're dealing
        // with at the cost of very minimal reparsing should the difference be found towards the end
        // of the input.
        let fork = input.fork();
        if let Ok(field) = Field::parse_named(&fork) {
            input.advance_to(&fork);
            return Ok(NestableField::Field(field));
        }

        let attrs = input.call(Attribute::parse_outer)?;
        let vis: Visibility = input.parse()?;
        let ident = input.parse()?;
        let colon_token: Token![:] = input.parse()?;

        let nested_struct = input.parse::<NestableStruct>()?;
        let ty = wrap_in_arc(nested_struct.ty.clone());

        let field = Field {
            attrs,
            vis,
            mutability: FieldMutability::None,
            ident: Some(ident),
            colon_token: Some(colon_token),
            ty,
        };

        Ok(NestableField::NestedStruct((field, nested_struct)))
    }
}

impl Parse for NestableVariant {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        // Here we can rely on that we already have parse implementations for all possible inputs
        // and cycle through them until a match is found.
        // TODO: Could NestableField be refactored the same way? Does it harm usability for determining syntax errors?

        // Ok(
        //     if let Ok(nested_struct) = input.fork().parse::<NestableStruct>() {
        //         let field = Variant {
        //             a
        //         };
        //         Self::NestedStruct((, nested_struct))
        //     } else {
        //         if let Ok(nested_enum) = input.fork().parse::<NestableEnum>() {
        //         } else {
        //             let variant = input.parse::<Variant>()?;
        //             Self::Variant(variant)
        //         }
        //     },
        // )
    }
}

fn ident_to_type(ident: Ident) -> Type {
    syn::parse_quote! { #ident }
}

fn wrap_in_arc(ty: Type) -> Type {
    parse_quote! {
        std::sync::Arc<#ty>
    }
}

pub(super) fn full_serde(_attr: LegacyTokenStream, item: LegacyTokenStream) -> LegacyTokenStream {
    let item = TokenStream::from(item);
    LegacyTokenStream::from(quote! {
        #[derive(::serde::Serialize, ::serde::Deserialize)]
        #item
    })
}

pub(super) fn full_serde_as(
    _attr: LegacyTokenStream,
    item: LegacyTokenStream,
) -> LegacyTokenStream {
    let item = TokenStream::from(item);
    LegacyTokenStream::from(quote! {
        #[serde_with::serde_as]
        #[derive(::serde::Serialize, ::serde::Deserialize)]
        #item
    })
}
