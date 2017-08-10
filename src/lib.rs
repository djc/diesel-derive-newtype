#![recursion_limit = "1024"] // the new default in rust 1.19, quote! takes a lot

//! # `#[Derive(DieselNewType)]
//!
//! This crate exposes a single custom-derive macro `DieselNewType` which
//! implements `ToSql`, `FromSql`, `FromSqlRow`, `Queryable`, `AsExpression`
//! and `QueryId` for the single-field tuple struct ([NewType][]) it is applied
//! to.
//!
//! ## Example
//!
//! This implementation:
//!
//! ```
//! #[macro_use]
//! extern crate diesel_newtype;
//!
//! #[derive(DieselNewType)] // Doesn't need to be on its own line
//! #[derive(Debug, Hash, PartialEq, Eq)] // required by diesel
//! struct MyId(i64);
//! # fn main() {}
//! ```
//!
//! Allows you to use the `MyId` struct inside your entities as though they were
//! the underlying type:
//!
//! ```
//! # #[macro_use] extern crate diesel;
//! # #[macro_use] extern crate diesel_codegen;
//! # #[macro_use] extern crate diesel_newtype;
//! # use diesel::prelude::*;
//! #
//! table! {
//!     my_items {
//!         id -> Integer,
//!         val -> Integer,
//!     }
//! }
//!
//! # #[derive(DieselNewType)] // Doesn't need to be on its own line
//! # #[derive(Debug, Hash, PartialEq, Eq)] // required by diesel
//! # struct MyId(i64);
//! #[derive(Debug, PartialEq, Identifiable, Queryable, Associations)]
//! struct MyItem {
//!     id: MyId,
//!     val: u8,
//! }
//! # fn main() {}
//! ```
//!
//! Oooohhh. Ahhhh.
//!

extern crate syn;
#[macro_use]
extern crate quote;
extern crate proc_macro;

use proc_macro::TokenStream;

#[proc_macro_derive(DieselNewType)]
#[doc(hidden)]
pub fn diesel_new_type(input: TokenStream) -> TokenStream {
    let source = input.to_string();

    let ast = syn::parse_derive_input(&source).unwrap();

    expand_sql_types(&ast).parse().unwrap()
}

fn expand_sql_types(ast: &syn::DeriveInput) -> quote::Tokens {
    let body = match ast.body {
        syn::Body::Enum(_) => {
            panic!("#[derive(DieselNewType)] can only be used with structs with a single field")
        }
        syn::Body::Struct(ref data) if data.fields().len() != 1 => {
            panic!("#[derive(DieselNewType)] can only be used with structs with exactly one field")
        }
        syn::Body::Struct(ref data) => data.fields()[0].clone(),
    };

    let name = &ast.ident;
    let wrapped_ty = body.ty;

    // Required to be able to insert/read from the db, don't allow searching
    let to_sql_impl = gen_tosql(&name, &wrapped_ty);
    let as_expr_impl = gen_asexpresions(&name, &wrapped_ty);

    // raw deserialization
    let from_sql_impl = gen_from_sql(&name, &wrapped_ty);
    let from_sqlrow_impl = gen_from_sqlrow(&name, &wrapped_ty);

    // querying
    let queryable_impl = gen_queryable(&name, &wrapped_ty);

    // since our query doesn't take varargs it's fine for the DB to cache it
    let query_id_impl = gen_query_id(&name);

    wrap_impls_in_const(name, quote! {
        #to_sql_impl
        #as_expr_impl

        #from_sql_impl
        #from_sqlrow_impl

        #queryable_impl

        #query_id_impl
    })
}

#[cfg(feature = "diesel015")]
fn gen_tosql(name: &syn::Ident, wrapped_ty: &syn::Ty) -> quote::Tokens {
    quote! {
        impl<ST, DB> diesel::types::ToSql<ST, DB> for #name
        where
            #wrapped_ty: diesel::types::ToSql<ST, DB>,
            DB: diesel::backend::Backend,
            DB: diesel::types::HasSqlType<ST>,
        {
            fn to_sql<W: ::std::io::Write>(&self, out: &mut diesel::types::ToSqlOutput<W, DB>)
            -> Result<diesel::types::IsNull, Box<::std::error::Error + Send + Sync>>
            {
                self.0.to_sql(out)
            }
        }
    }
}

#[cfg(feature = "diesel014")]
fn gen_tosql(name: &syn::Ident, wrapped_ty: &syn::Ty) -> quote::Tokens {
    quote! {
        impl<ST, DB> diesel::types::ToSql<ST, DB> for #name
        where
            #wrapped_ty: diesel::types::ToSql<ST, DB>,
            DB: diesel::backend::Backend,
            DB: diesel::types::HasSqlType<ST>,
        {
            fn to_sql<W: ::std::io::Write>(&self, out: &mut W)
            -> Result<diesel::types::IsNull, Box<::std::error::Error + Send + Sync>>
            {
                self.0.to_sql(out)
            }
        }
    }
}

fn gen_asexpresions(name: &syn::Ident, wrapped_ty: &syn::Ty) -> quote::Tokens {
    quote! {

        impl<ST> diesel::expression::AsExpression<ST> for #name
        where
            diesel::expression::bound::Bound<ST, #wrapped_ty>:
                diesel::expression::Expression<SqlType=ST>,
        {
            type Expression = diesel::expression::bound::Bound<ST, #wrapped_ty>;

            fn as_expression(self) -> Self::Expression {
                diesel::expression::bound::Bound::new(self.0)
            }
        }

        impl<'expr, ST> diesel::expression::AsExpression<ST> for &'expr #name
        where
            diesel::expression::bound::Bound<ST, #wrapped_ty>:
                diesel::expression::Expression<SqlType=ST>
        {
            type Expression = diesel::expression::bound::Bound<ST, &'expr #wrapped_ty>;

            fn as_expression(self) -> Self::Expression {
                diesel::expression::bound::Bound::new(&self.0)
            }
        }
    }
}

fn gen_from_sql(name: &syn::Ident, wrapped_ty: &syn::Ty) -> quote::Tokens {
    quote! {
        impl<ST, DB> diesel::types::FromSql<ST, DB> for #name
        where
            #wrapped_ty: diesel::types::FromSql<ST, DB>,
            DB: diesel::backend::Backend,
            DB: diesel::types::HasSqlType<ST>,
        {
            fn from_sql(raw: Option<&<DB as diesel::backend::Backend>::RawValue>)
            -> Result<Self, Box<::std::error::Error + Send + Sync>>
            {
                diesel::types::FromSql::<ST, DB>::from_sql(raw)
                    .map(#name)
            }
        }
    }
}

fn gen_from_sqlrow(name: &syn::Ident, wrapped_ty: &syn::Ty) -> quote::Tokens {
    quote! {
        impl<ST, DB> diesel::types::FromSqlRow<ST, DB> for #name
        where
            #wrapped_ty: diesel::types::FromSql<ST, DB>,
            DB: diesel::backend::Backend,
            DB: diesel::types::HasSqlType<ST>,
        {
            fn build_from_row<R: diesel::row::Row<DB>>(row: &mut R)
            -> Result<Self, Box<::std::error::Error + Send + Sync>>
            {
                diesel::types::FromSql::<ST, DB>::from_sql(row.take())
            }
        }
    }
}

fn gen_queryable(name: &syn::Ident, wrapped_ty: &syn::Ty) -> quote::Tokens {
    quote! {
        impl<ST, DB> diesel::query_source::Queryable<ST, DB> for #name
        where
            #wrapped_ty: diesel::types::FromSqlRow<ST, DB>,
            DB: diesel::backend::Backend,
            DB: diesel::types::HasSqlType<ST>,
        {
            type Row = #wrapped_ty;

            fn build(row: Self::Row) -> Self {
                #name(row)
            }
        }
    }
}

fn gen_query_id(name: &syn::Ident) -> quote::Tokens {
    quote! {
        impl diesel::query_builder::QueryId for #name {
            type QueryId = Self;
            fn has_static_query_id() -> bool {
                true
            }
        }
    }
}

/// This guarantees that items we generate don't polute the module scope
///
/// We use the const name as a form of documentation of the generated code
fn wrap_impls_in_const(ty_name: &syn::Ident, item: quote::Tokens) -> quote::Tokens {
    let name = ty_name.to_string().to_uppercase();
    let dummy_const: syn::Ident = format!("_IMPL_DIESEL_NEW_TYPE_FOR_{}", name).into();
    quote! {
        const #dummy_const: () = {
            extern crate diesel;
            #item
        };
    }
}
