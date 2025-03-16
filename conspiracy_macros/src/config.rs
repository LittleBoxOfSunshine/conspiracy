use proc_macro::TokenStream as LegacyTokenStream;
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote, ToTokens};
use syn::{
    braced,
    parse::{discouraged::Speculative, Parse, ParseStream},
    parse_macro_input, parse_quote,
    punctuated::Punctuated,
    token,
    token::{Colon, Pub},
    Attribute, Expr, Field, FieldMutability, Ident, Token, Type, Visibility,
};

pub(super) fn restart_required(input: LegacyTokenStream) -> LegacyTokenStream {
    let input = parse_macro_input!(input as NestableStruct);
    let comparison = build_restart_comparison(&input);
    let ty = input.ty;

    LegacyTokenStream::from(quote! {
        impl ::conspiracy::config::RestartRequired for #ty {
            fn restart_required(&self, other: &Self) -> bool {
                #comparison
            }
        }
    })
}

fn build_restart_comparison(input: &NestableStruct) -> TokenStream {
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
    item: &NestableStruct,
) {
    for field in &item.fields {
        match field {
            NestableField::Struct((field, nested_struct)) => {
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
    field: &Field,
) {
    let has_restart_attr = field
        .attrs
        .iter()
        .any(|attr| attr.path().is_ident("restart"));

    if has_restart_attr {
        output.push(comparison_for_field(lineage, field))
    }
}

fn comparison_for_field(lineage: &mut Vec<Ident>, field: &Field) -> TokenStream {
    let field_name = field.ident.as_ref().expect("All fields must be named");
    let field_expr = if lineage.is_empty() {
        quote! { #field_name }
    } else {
        quote! { #(#lineage).*.#field_name }
    };

    quote! {
        self.#field_expr != other.#field_expr
    }
}

pub(super) fn config_struct(input: LegacyTokenStream) -> LegacyTokenStream {
    let input = parse_macro_input!(input as NestableStruct);
    let mut output = generate_compact_struct(&input);
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
                NestableField::Struct((field, nested_struct)) => {
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
        NestableField::Struct((field, _)) => {
            let ident = field.ident.clone();
            quote! { #ident: self.#ident.arcify() }
        }
    });

    output.extend(quote! {
        impl #compact_ty {
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
            NestableField::Struct((field, nested)) => {
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
        #[derive(Clone, PartialEq, ::conspiracy::config::RestartRequired, ::serde::Serialize, ::serde::Deserialize)]
        #(#attrs)*
        #vis #struct_token #ty {
            #(#fields),*
        }
    });

    let compact_ty = compact_ty_name(&ty);
    let compacted_fields = input.fields.iter().map(|field| match field {
        NestableField::Struct((field, _)) => {
            let ident = field.ident.clone();
            quote! { #ident: (*self.#ident).clone().compact() }
        }
        NestableField::Field(field) => {
            let ident = field.ident.clone();
            quote! { #ident: self.#ident }
        }
    });

    output.extend(quote! {
        impl #ty {
            pub fn compact(self) -> #compact_ty {
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
    Struct((Field, NestableStruct)),
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
            Some(nested_struct) => NestableField::Struct((field, nested_struct)),
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

pub(super) fn arcify(input: LegacyTokenStream) -> LegacyTokenStream {
    let input = parse_macro_input!(input as ArcStruct);
    LegacyTokenStream::from(input.to_token_stream())
}

struct ArcStruct {
    ty: Type,
    _brace_token: token::Brace,
    fields: Punctuated<ArcStructField, Token![,]>,
}

struct ArcStructField {
    ident: Ident,
    _separator: Token![:],
    value: ArcStructFieldValue,
}

enum ArcStructFieldValue {
    Nested(ArcStruct),
    Value(Expr),
}

impl Parse for ArcStruct {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let raw_fields;
        Ok(ArcStruct {
            ty: input.parse()?,
            _brace_token: braced!(raw_fields in input),
            fields: raw_fields.parse_terminated(ArcStructField::parse, Token![,])?,
        })
    }
}

impl Parse for ArcStructField {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let ident = input.parse()?;
        let _separator = input.parse()?;

        let lookahead = input.lookahead1();
        let value = if lookahead.peek(Ident) {
            // Identifier could be a variable, or the beginning of a nested definition
            if input.peek2(token::Brace) {
                ArcStructFieldValue::Nested(input.parse()?)
            } else {
                ArcStructFieldValue::Value(input.parse()?)
            }
        } else {
            ArcStructFieldValue::Value(input.parse()?)
        };

        Ok(ArcStructField {
            ident,
            _separator,
            value,
        })
    }
}

impl ToTokens for ArcStruct {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let ty = self.ty.clone();
        let fields = &self.fields;
        tokens.extend(quote! {
            #ty {
                #fields
            }
        });
    }
}

impl ToTokens for ArcStructField {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let ident = &self.ident;
        let value = &self.value.to_token_stream();

        tokens.extend(quote! {
            #ident: #value
        })
    }
}

impl ToTokens for ArcStructFieldValue {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match &self {
            ArcStructFieldValue::Nested(x) => {
                let expr = x.to_token_stream();
                tokens.extend(quote! {
                    std::sync::Arc::new(#expr)
                });
            }
            ArcStructFieldValue::Value(x) => x.to_tokens(tokens),
        }
    }
}
