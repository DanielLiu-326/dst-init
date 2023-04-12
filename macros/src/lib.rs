#![feature(proc_macro_diagnostic)]
#![feature(const_trait_impl)]
#![feature(decl_macro)]

use proc_macro::{TokenStream};
use std::alloc::Layout;
use std::any::Any;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::process::Output;
use std::ptr::NonNull;
use convert_case::{Case, Casing};
use proc_macro2::{Ident, Punct, Span};
use quote::{format_ident, ToTokens};
use syn::{Token, GenericParam, ItemEnum, ItemStruct, Path, Type, TypeParam, ImplItem, ImplItemType, GenericArgument, ExprField, Member, Index, Attribute, Fields, ItemImpl};
use syn::ext::IdentExt;
use syn::spanned::Spanned;
use quote::{quote};
use syn::punctuated::Punctuated;
use syn::token::{Comma, Impl};
use syn::parse_quote::ParseQuote;

/// Origin code:
/// ```rust
/// struct Test<const CONSTANT:usize,A,B,C,D>{
///     a:A,
///     b:B,
///     c:C,
///     dst:[(C,D)],
/// }
/// ```
/// Macro output:
/// ```rust
/// #[repr(C)]
/// struct Test<const CONSTANT:usize,A,B,C,D>{
///     a:A,
///     b:B,
///     c:C,
///     dst:[(C,D)],
/// };
/// #[repr(C)]
/// struct TestFst<const CONSTANT:usize,A,B,C,D>{
///     a:A,
///     b:B,
///     c:C,
///     dst:PhantomData<[(C,D)]>,
/// };
/// #[repr(C)]
/// struct TestInit<const CONSTANT:usize,A,B,C,D,INIT:EmplaceInitializer<Output=[(C,D)]> >{
///     a:A,
///     b:B,
///     c:C,
///     dst:INIT,
/// };
/// ```
#[proc_macro_attribute]
pub fn dst(_attr:TokenStream, input:TokenStream) -> TokenStream{
    let mut item_struct:ItemStruct = syn::parse(input.clone()).unwrap();
    let mut struct_name = item_struct.ident.clone();
    let mut struct_vis = item_struct.vis.clone();
    let mut struct_where_clause = item_struct.generics.where_clause.clone();
    let mut struct_generics_param = item_struct.generics.params.clone();
    let mut struct_where_clause = item_struct.generics.where_clause.clone();
    let mut dst_type = item_struct.fields.iter().last().unwrap().ty.clone();
    let mut field_num = item_struct.fields.iter().len();
    let mut dst_field:Member = item_struct.fields.iter().last().unwrap().ident
        .clone().map_or(Member::Unnamed(Index::from(field_num)),|i|{
        Member::Named(i)
    });

    if !struct_generics_param.trailing_punct()&&!struct_generics_param.is_empty(){
        struct_generics_param.push_punct(Comma::default());
    }

    let mut struct_generics_arg = struct_generics_param
        .iter()
        .fold(Punctuated::<GenericArgument, Comma>::new(),|mut last,i|{
            let push = match i.clone(){
                GenericParam::Type(t)=>{
                    let t = t.ident;
                    GenericArgument::Type(syn::parse(quote!(#t).into()).unwrap())
                },
                GenericParam::Const(t)=>{
                    let t = t.ident;
                    GenericArgument::Const(syn::parse(quote!(#t).into()).unwrap())
                },
                GenericParam::Lifetime(t)=>GenericArgument::Lifetime(t.lifetime),
            };
            last.push(push);
            last.push_punct(Comma::default());
            last
        });

    let mut new_struct = quote!(#[repr(C)]);
    new_struct.extend(item_struct.into_token_stream());
    let mut new_struct:ItemStruct = syn::parse(new_struct.into()).unwrap();

    let mut fst_struct = new_struct.clone();
    let fst_ident = format_ident!("{}Fst",struct_name);
    fst_struct.ident = fst_ident.clone();
    fst_struct.fields.iter_mut()
        .last().unwrap().ty = syn::parse(quote!(core::marker::PhantomData< #dst_type >).into()).unwrap();

    let mut init_struct = new_struct.clone();
    let init_ident = format_ident!("{}Init",struct_name.to_string());
    init_struct.ident = init_ident.clone();
    init_struct.generics.params
        .push(GenericParam::Type(syn::parse(quote!(INIT:EmplaceInitializer<Output=#dst_type>).into()).unwrap()));
    init_struct.fields.iter_mut()
        .last().unwrap().ty = syn::parse(quote!(INIT).into()).unwrap();

    let init_ident = init_struct.ident.clone();
    let impl_emplace:ItemImpl = syn::parse(quote!(
        impl<#struct_generics_param INIT:EmplaceInitializer<Output=#dst_type>> EmplaceInitializer for #init_ident<#struct_generics_arg INIT>
            #struct_where_clause
        {
            type Output = #struct_name<#struct_generics_arg>;
            #[inline(always)]
            fn layout(&mut self) -> Layout{
                let layout = Layout::new::<#fst_ident<#struct_generics_arg>>();
                layout
                    .extend(self.#dst_field.layout())
                    .unwrap()
                    .0
                    .pad_to_align()
            }
            #[inline(always)]
            fn emplace(mut self, ptr: NonNull<u8>) -> NonNull<Self::Output>{unsafe{
                use core::mem;
                use core::mem::size_of;
                use std::ptr;
                let fst_layout = Layout::new::<#fst_ident<#struct_generics_arg>>();
                let dst_layout = self.#dst_field.layout();
                let dst = ptr
                    .as_ptr()
                    .add(size_of::<#fst_ident<#struct_generics_arg>>())
                    .add(fst_layout.padding_needed_for(dst_layout.align()));
                let fst = ptr::read(&self as *const Self as *const _);
                let dst_init = ptr::read(&self.#dst_field as *const INIT);
                std::mem::forget(self);
                ptr.as_ptr().cast::<#fst_ident<#struct_generics_arg>>().write(fst);
                let (_, meta) = dst_init
                    .emplace(NonNull::new(dst.cast()).unwrap())
                    .to_raw_parts();
                mem::transmute(NonNull::<#dst_type>::from_raw_parts(ptr.cast(), meta))
            }}
        }
    ).into()).unwrap();

    let impl_init:ItemImpl = syn::parse(quote!(
        impl<#struct_generics_param DstInit:EmplaceInitializer<Output=#dst_type>> Initializer<DstInit> for #struct_name<#struct_generics_arg>
            #struct_where_clause
        {
            type Init = #init_ident<#struct_generics_arg DstInit>;
        }
    ).into()).unwrap();

    let mut output = new_struct.into_token_stream();
    output.extend(fst_struct.into_token_stream());
    output.extend(init_struct.into_token_stream());
    output.extend(impl_emplace.into_token_stream());
    output.extend(impl_init.into_token_stream());

    output.into()
}
