use proc_macro2::TokenStream;
use quote::quote;
use syn::{Attribute, Path};

#[derive(Clone)]
pub(crate) enum ConspiracyAttribute {
    Restart,
}

pub(crate) fn extract_conspiracy_attributes(
    attrs: &mut Vec<Attribute>,
) -> Option<ConspiracyAttribute> {
    let mut extracted_attr = None;
    attrs.retain(|attr| {
        if attr.path().is_ident("conspiracy") {
            let kind: Path = attr.parse_args().unwrap();
            if kind.is_ident("restart") {
                try_set_attribute(&mut extracted_attr, ConspiracyAttribute::Restart);
                return false;
            }
        }

        true
    });

    extracted_attr
}

fn try_set_attribute(old_attr: &mut Option<ConspiracyAttribute>, attr: ConspiracyAttribute) {
    if old_attr.is_none() {
        *old_attr = Some(attr)
    } else {
        panic!("You can't use multiple conspiracy attributes on a single field")
    }
}

pub(crate) fn restart_required_single_field_comparison(field_expr: TokenStream) -> TokenStream {
    quote! {
        self.#field_expr != other.#field_expr
    }
}
