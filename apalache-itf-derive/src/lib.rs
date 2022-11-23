extern crate proc_macro;

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{quote, quote_spanned};
use syn::{
    parse_macro_input, parse_quote, spanned::Spanned, Attribute, Data, DataEnum, DeriveInput,
    Fields, FieldsNamed, FieldsUnnamed, GenericParam, Generics, Lit, Meta, NestedMeta, Variant,
};

#[proc_macro_derive(DecodeItfValue, attributes(itf))]
pub fn derive_decode_itf_value(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;
    let generics = add_trait_bounds(input.generics);
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let decode = itf_decode(&input.data, &input.attrs);

    let expanded = quote! {
        impl #impl_generics ::apalache_itf::DecodeItfValue for #name #ty_generics
            #where_clause {

            fn decode(value: ::apalache_itf::Value) -> Result<Self, ::apalache_itf::DecodeError> {
                #decode
            }
        }
    };

    TokenStream::from(expanded)
}

#[proc_macro_derive(TryFromRawState, attributes(itf))]
pub fn derive_try_from_raw_state(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;
    let generics = add_trait_bounds(input.generics);
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let try_from = try_from_raw_state(&input.data, &input.attrs);

    let expanded = quote! {
        impl #impl_generics TryFrom<::apalache_itf::raw::State> for #name #ty_generics
            #where_clause {

            type Error = ::apalache_itf::DecodeError;

            fn try_from(mut raw_state: ::apalache_itf::raw::State) -> Result<Self, Self::Error> {
                #try_from
            }
        }
    };

    TokenStream::from(expanded)
}

/// Add a bound `T: HeapSize` to every type parameter T.
fn add_trait_bounds(mut generics: Generics) -> Generics {
    for param in &mut generics.params {
        if let GenericParam::Type(ref mut type_param) = *param {
            type_param
                .bounds
                .push(parse_quote!(::apalache_itf::DecodeItfValue));
        }
    }
    generics
}

fn itf_decode(data: &Data, attrs: &[Attribute]) -> TokenStream2 {
    match *data {
        Data::Struct(ref data) => match data.fields {
            Fields::Named(ref fields) => {
                let body = derive_struct_named(quote!(Self), fields, quote!(map));

                quote! {
                    use ::std::collections::HashMap;
                    use ::apalache_itf::{Value, DecodeItfValue};

                    let mut map = <HashMap<String, Value> as DecodeItfValue>::decode(value)?;

                    #body
                }
            }
            Fields::Unnamed(ref fields) => derive_struct_unnamed(fields),
            Fields::Unit => quote!(Self),
        },

        Data::Enum(ref data) => {
            if data
                .variants
                .iter()
                .all(|v| matches!(v.fields, Fields::Unit))
            {
                unit_enum(data)
            } else if data
                .variants
                .iter()
                .all(|v| matches!(v.fields, Fields::Named(_)))
            {
                let itf_attrs = parse_itf_attrs(attrs);
                named_enum(data, &itf_attrs.tag)
            } else {
                quote! {
                    ::std::compile_error!("only unit variants or named fields variants are supported")
                }
            }
        }

        Data::Union(_) => unimplemented!(),
    }
}

fn named_enum(data: &DataEnum, tag: &str) -> TokenStream2 {
    let cases = data.variants.iter().map(|v| {
        let fields = match v.fields {
            Fields::Named(ref fields) => fields,
            _ => unreachable!(),
        };

        let ident = &v.ident;
        let attrs = parse_itf_attrs(&v.attrs);
        let name = attrs.rename.unwrap_or_else(|| ident.to_string());
        let cons = quote!(Self::#ident);
        let extract = derive_struct_named(cons, fields, quote!(record));

        quote! {
            #name => {
                #extract
            }
        }
    });

    quote! {
        use ::std::collections::HashMap;
        use ::apalache_itf::{Value, DecodeItfValue, DecodeError};

        let mut record = <HashMap::<String, Value>>::decode(value)?;

        let tag = record
            .remove(#tag)
            .ok_or(DecodeError::UnknownTag(#tag))
            .and_then(<String as DecodeItfValue>::decode)?;

        match tag.as_str() {
            #(#cases ,)*

            _ => Err(DecodeError::UnknownVariant(tag)),
        }
    }
}

fn unit_enum(data: &DataEnum) -> TokenStream2 {
    let cases = data.variants.iter().map(|v| match v.fields {
        Fields::Unit => unit_variant(v),
        _ => unreachable!(),
    });

    quote! {
        use ::apalache_itf::{Value, DecodeItfValue, DecodeError};

        match value {
            #(#cases, )*
            _ => Err(DecodeError::InvalidType("string"))
        }
    }
}

fn unit_variant(v: &Variant) -> TokenStream2 {
    assert!(matches!(v.fields, Fields::Unit));

    let name = &v.ident;
    let attrs = parse_itf_attrs(&v.attrs);
    let value = attrs.rename.unwrap_or_else(|| name.to_string());

    quote_spanned! { v.span() =>
        Value::String(s) if s == #value => Ok(Self::#name)
    }
}

fn try_from_raw_state(data: &Data, _attrs: &[Attribute]) -> TokenStream2 {
    match *data {
        Data::Struct(ref data) => match data.fields {
            Fields::Named(ref fields) => {
                derive_struct_named(quote!(Self), fields, quote!(raw_state.values))
            }
            Fields::Unnamed(ref fields) => derive_struct_unnamed(fields),
            Fields::Unit => quote!(Self),
        },

        Data::Enum(_) | Data::Union(_) => unimplemented!(),
    }
}

fn derive_struct_named(
    cons: TokenStream2,
    fields: &FieldsNamed,
    map: TokenStream2,
) -> TokenStream2 {
    let recurse = fields.named.iter().map(|f| {
        let name = f.ident.as_ref().unwrap();
        let ty = &f.ty;
        let attrs = parse_itf_attrs(&f.attrs);
        let value = attrs.rename.unwrap_or_else(|| name.to_string());

        quote_spanned! { f.span() =>
            #name : <#ty as ::apalache_itf::DecodeItfValue>::decode(
                #map
                    .remove(#value)
                    .ok_or(::apalache_itf::DecodeError::FieldNotFound(#value))?
            )?
        }
    });

    quote! {
        Ok(#cons {
            #(#recurse ,)*
        })
    }
}

fn derive_struct_unnamed(fields: &FieldsUnnamed) -> TokenStream2 {
    let types = fields_to_tuple_type(fields);

    quote! {
        use ::apalache_itf::DecodeItfValue;
        Ok(<#types as DecodeItfValue>::decode(value))
    }
}

#[derive(Debug)]
struct ItfAttributes {
    tag: String,
    rename: Option<String>,
}

impl Default for ItfAttributes {
    fn default() -> Self {
        Self {
            tag: "tag".to_string(),
            rename: None,
        }
    }
}

fn parse_itf_attrs(attrs: &[Attribute]) -> ItfAttributes {
    let mut itf_attrs = ItfAttributes::default();

    for attr in attrs {
        if let Ok(syn::Meta::List(list)) = attr.parse_meta() {
            let is_itf = list.path.get_ident().map_or(false, |i| i == "itf");
            if !is_itf {
                continue;
            }

            for meta in list.nested {
                if let NestedMeta::Meta(Meta::NameValue(meta)) = meta {
                    if let Some(name) = meta.path.get_ident() {
                        if let Lit::Str(value) = meta.lit {
                            match name.to_string().as_str() {
                                "rename" => {
                                    itf_attrs.rename = Some(value.value());
                                }
                                "tag" => {
                                    itf_attrs.tag = value.value();
                                }
                                _ => (),
                            }
                        }
                    }
                }
            }
        }
    }

    itf_attrs
}

fn fields_to_tuple_type(fields: &FieldsUnnamed) -> TokenStream2 {
    let types = fields.unnamed.iter().map(|f| &f.ty);

    quote! {
        (#(#types ,)*)
    }
}
