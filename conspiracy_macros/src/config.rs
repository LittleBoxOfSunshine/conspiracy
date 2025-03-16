use proc_macro::TokenStream;
use quote::{quote, ToTokens};
use syn::{
    braced,
    parse::{discouraged::Speculative, Parse, ParseStream},
    parse_macro_input, parse_quote,
    punctuated::Punctuated,
    token, Attribute, Expr, Field, FieldMutability, Ident, Path, PathArguments, PathSegment, Token,
    Type, TypePath, Visibility,
};

pub(super) fn config_struct(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as NestableStruct);
    TokenStream::from(generate_config_structs(input, &mut vec![]))
}

fn generate_config_structs(
    input: NestableStruct,
    lineage: &mut Vec<(Ident, Type)>,
) -> proc_macro2::TokenStream {
    let mut output = proc_macro2::TokenStream::new();
    let fields = input
        .fields
        .into_iter()
        .map(|config_field| match config_field {
            NestableField::Struct((field, nested)) => {
                lineage.push((
                    field
                        .ident
                        .clone()
                        .expect("At this stage, only named fields can be present"),
                    input.ty.clone(),
                ));
                output.extend(impl_as_field_for_lineage(lineage, &nested));
                output.extend(generate_config_structs(nested, lineage));
                lineage.pop();
                field
            }
            NestableField::Field(field) => field,
        })
        .collect::<Vec<Field>>()
        .into_iter();

    let attrs = input.attrs;
    let vis = input.vis;
    let struct_token = input.struct_token;
    let ty = input.ty;

    output.extend(quote! {
        #[derive(::serde::Serialize, ::serde::Deserialize)]
        #(#attrs)*
        #vis #struct_token #ty {
            #(#fields),*
        }
    });

    output
}

fn impl_as_field_for_lineage(
    lineage: &[(Ident, Type)],
    nested: &NestableStruct,
) -> proc_macro2::TokenStream {
    let mut output = proc_macro2::TokenStream::new();

    for i in (0..lineage.len()).rev() {
        output.extend(impl_as_field(&lineage[i..], nested.ty.clone()));
    }

    output
}

fn impl_as_field(lineage: &[(Ident, Type)], child_ty: Type) -> proc_macro2::TokenStream {
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

struct NestableStruct {
    attrs: Vec<Attribute>,
    vis: Visibility,
    struct_token: Token![struct],
    ty: Type,
    _brace_token: token::Brace,
    fields: Punctuated<NestableField, Token![,]>,
}

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
            ty: create_type_from_ident(input.parse()?),
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

fn create_type_from_ident(ident: Ident) -> Type {
    Type::Path(TypePath {
        qself: None,
        path: Path {
            leading_colon: None,
            segments: vec![PathSegment {
                ident,
                arguments: PathArguments::None,
            }]
            .into_iter()
            .collect(),
        },
    })
}

fn wrap_in_arc(ty: Type) -> Type {
    parse_quote! {
        std::sync::Arc<#ty>
    }
}

pub(super) fn arcify(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as ArcStruct);
    TokenStream::from(input.to_token_stream())
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
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
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
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let ident = &self.ident;
        let value = &self.value.to_token_stream();

        tokens.extend(quote! {
            #ident: #value
        })
    }
}

impl ToTokens for ArcStructFieldValue {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
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
