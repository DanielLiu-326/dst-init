#![feature(proc_macro_diagnostic)]
#![feature(const_trait_impl)]

use proc_macro::{TokenStream};
use std::alloc::Layout;
use std::any::Any;
use std::collections::HashMap;
use std::ptr::NonNull;
use convert_case::{Case, Casing};
use proc_macro2::{Ident, Punct, Span};
use quote::ToTokens;
use syn::{Token, GenericParam, ItemEnum, ItemStruct, Path, Type, TypeParam, ImplItem, ImplItemType, GenericArgument, ExprField, Member, Index, Attribute, Fields};
use syn::ext::IdentExt;
use syn::spanned::Spanned;
use quote::{quote};
use syn::punctuated::Punctuated;
use syn::token::Comma;
use syn::parse_quote::ParseQuote;

// pub trait EmplaceInitializer {
//     type Output: ?Sized;
//     fn layout(&mut self) -> Layout;
//     fn emplace(self, ptr: NonNull<u8>) -> NonNull<Self::Output>;
// }

// #[dst]

#[proc_macro_attribute]
pub fn dst(_attr:TokenStream, input:TokenStream) -> TokenStream{
    let mut item_struct:ItemStruct = syn::parse(input.clone()).unwrap();
    let mut struct_name = item_struct.ident.clone();
    let mut struct_vis = item_struct.vis.clone();
    let mut struct_where_clause = item_struct.generics.where_clause.clone();
    let mut origin_generics = item_struct.generics.clone();
    let mut dst_type = item_struct.fields.iter().last().unwrap().ty.clone();
    let mut field_num = item_struct.fields.iter().len();
    let mut dst_field:Member = item_struct.fields.iter().last().unwrap().ident
        .clone().map_or(Member::Unnamed(Index::from(field_num)),|i|{
        Member::Named(i)
    });

    // revise old struct defination
    let mut new_struct = item_struct.clone();
    new_struct.ident=Ident::new(&(struct_name.to_string()+"Dst"),Span::mixed_site());
    new_struct.fields.iter_mut().last().unwrap().ty = syn::parse(quote!(DST).into()).unwrap();
    // if last field is generic type
    // do nothing with new struct defination,
    // else:
    // add new generic
    new_struct
        .generics
        .params
        .push(syn::parse(quote!(DST:?Sized).into()).unwrap());
    let new_struct_name = new_struct.ident.clone();
    let mut output:proc_macro2::TokenStream = quote!(#[repr(C)]);
    output.extend(new_struct.into_token_stream());

    let mut origin_generic_param = origin_generics.params.clone();
    let mut origin_generic_args = origin_generics
        .params
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
        last
    });
    let comma = if origin_generic_param.empty_or_trailing(){
        if origin_generic_param.trailing_punct(){
            origin_generic_args.push_punct(Comma::default());
        }
        quote!()
    }else{
        quote!(,)
    };

    output.extend(quote!(
        #[allow(type_alias_bounds)]
        #struct_vis type #struct_name <#origin_generic_param> = #new_struct_name <#origin_generic_args #comma #dst_type>;
    ));

    let init_ident:Ident = Ident::new(&(struct_name.to_string()+"Init"),Span::mixed_site());
    let fst_ident:Ident = Ident::new(&(struct_name.to_string()+"Fst"),Span::mixed_site());
    output.extend(quote!(
        #[allow(type_alias_bounds)]
        #struct_vis type #fst_ident <#origin_generic_param> = #new_struct_name <#origin_generic_args #comma ()>;

        #[allow(type_alias_bounds)]
        #struct_vis type #init_ident <#origin_generic_param #comma INIT:EmplaceInitializer<Output=#dst_type>>= #new_struct_name <#origin_generic_args #comma INIT>;

        impl<#origin_generic_param> #struct_name <#origin_generic_args>
            #struct_where_clause
        {
            #[inline(always)]
            fn alloc<Init:EmplaceInitializer<Output=Self>>(mut init:Init)->Box<Self>{unsafe{
                let layout = init.layout();
                let mem = std::alloc::alloc(layout);
                let t = init.emplace(NonNull::new(mem).unwrap());
                Box::from_raw(t.as_ptr())
            }}
        }

        impl<#origin_generic_param #comma INIT:EmplaceInitializer<Output=#dst_type>> EmplaceInitializer for #init_ident <#origin_generic_args #comma INIT>
            #struct_where_clause
        {
            type Output = #struct_name<#origin_generic_args>;
            #[inline(always)]
            fn layout(&mut self) -> Layout{
                let layout = Layout::new::<#fst_ident<#origin_generic_args>>();
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
                let fst_layout = Layout::new::<#fst_ident<#origin_generic_args>>();
                let dst_layout = self.#dst_field.layout();
                let dst = ptr
                    .as_ptr()
                    .add(size_of::<#fst_ident<#origin_generic_args>>())
                    .add(fst_layout.padding_needed_for(dst_layout.align()));
                let fst = ptr::read(&self as *const Self as *const _);
                let dst_init = ptr::read(&self.#dst_field as *const INIT);
                std::mem::forget(self);
                ptr.as_ptr().cast::<#fst_ident<#origin_generic_args>>().write(fst);
                let (_, meta) = dst_init
                    .emplace(NonNull::new(dst.cast()).unwrap())
                    .to_raw_parts();
                mem::transmute(NonNull::<#dst_type>::from_raw_parts(ptr.cast(), meta))
            }}
        }
    ));

    println!("{}",output);
    output.into()
}