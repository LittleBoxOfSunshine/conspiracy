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
    Attribute, Field, FieldMutability, Ident, Token, Type, Visibility,
};

use crate::common::{
    extract_conspiracy_attributes, restart_required_single_field_comparison, ConspiracyAttribute,
};

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
    let mut input = parse_macro_input!(input as NestableStruct);
    let mut output = restart_required(&mut input);
    output.extend(generate_compact_struct(&input));
    output.extend(generate_config_structs(input, &mut vec![]));

    LegacyTokenStream::from(output)
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
        })
        .cloned()
        .collect::<Vec<Field>>()
        .into_iter();

    let attrs = input.attrs;
    let vis = input.vis;
    let struct_token = input.struct_token;
    let ty = input.ty;

    output.extend(quote! {
        #[derive(Clone, PartialEq, ::serde::Serialize, ::serde::Deserialize)]
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
enum NestableField {
    NestedStruct((Field, NestableStruct)),
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

impl Parse for NestableField {
    // Here we mostly mirror [`syn::data::Field::parse_named`]
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;
        let vis: Visibility = input.parse()?;
        let ident = input.parse()?;
        let colon_token: Token![:] = input.parse()?;

        let ty: Type;
        let mut nested_struct: Option<NestableStruct> = None;

        let fork = input.fork();
        if let Ok(nested) = fork.parse::<NestableStruct>() {
            input.advance_to(&fork);
            ty = wrap_in_arc(nested.ty.clone());
            nested_struct = Some(nested);
        } else {
            ty = input.parse::<Type>()?;
        }

        let field = Field {
            attrs,
            vis,
            mutability: FieldMutability::None,
            ident: Some(ident),
            colon_token: Some(colon_token),
            ty,
        };

        Ok(match nested_struct {
            None => NestableField::Field(field),
            Some(nested_struct) => NestableField::NestedStruct((field, nested_struct)),
        })
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
