use proc_macro2::Span;
use quote::quote;
use syn::{parse_macro_input, DataStruct, DeriveInput, Error, GenericParam};

#[proc_macro_derive(ByteParse)]
pub fn derive_byte_parse(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    match &ast.data {
        syn::Data::Struct(strct) => derive_from_bytes_fixed_struct(&ast, strct),
        syn::Data::Enum(_) => {
            Error::new(Span::call_site(), "unsupported on enums").to_compile_error()
        }
        syn::Data::Union(_) => {
            Error::new(Span::call_site(), "unsupported on unions").to_compile_error()
        }
    }
    .into()
}

fn derive_from_bytes_fixed_struct(
    ast: &DeriveInput,
    strct: &DataStruct,
) -> proc_macro2::TokenStream {
    let generics = &ast.generics;
    let param_idents = generics.params.iter().map(|param| match param {
        GenericParam::Type(ty) => {
            let ident = &ty.ident;
            quote!(#ident)
        }
        GenericParam::Lifetime(l) => quote!(#l),
        GenericParam::Const(cnst) => quote!(#cnst),
    });

    let asserts = {
        let types = strct.fields.iter().map(|f| &f.ty);
        quote! {
            struct ImplementsByteParse<F: ?Sized + ::binstream::ByteParse>(::core::marker::PhantomData<F>);
            #( let _: ImplementsByteParse<#types>; )*
        }
    };

    let field_readers = strct.fields.iter().map(|f| {
        let ty = &f.ty;
        let name = &f.ident;
        quote! {
            let #name = <#ty as ::binstream::ByteParse>::parse(p)?;
        }
    });

    let field_names = strct.fields.iter().flat_map(|f| f.ident.as_ref());

    let name = &ast.ident;
    let expanded = quote! {
        impl #generics ::binstream::ByteParse for #name< #(#param_idents),* > {
            fn parse(p: &mut ::binstream::ByteReader) -> Option<Self> {
                #asserts
                #( #field_readers )*
                Some(#name {
                    #( #field_names ),*
                })
            }
        }
    };

    expanded
}
