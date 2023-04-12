#![feature(proc_macro_diagnostic)]
#![feature(const_trait_impl)]
#![feature(decl_macro)]

use proc_macro::{TokenStream};
use quote::{format_ident, ToTokens};
use syn::{GenericParam, ItemStruct, GenericArgument, Member, Index, ItemImpl};
use quote::{quote};
use syn::punctuated::Punctuated;
use syn::token::{Comma};

/// # Usage:
/// Add `#[dst]` ahead of struct item as below:
/// ```rust
/// #[dst]
/// struct Foo{
///     a:u8,
///     b:[usize],
/// }
/// ```
/// after expansion:
/// ```rust
/// use std::marker::PhantomData;
/// #[repr(C)]
/// struct Foo{
///     a:u8,
///     b:[usize],
/// }
///
/// #[repr(C)]
/// struct FooInit<INIT:EmplaceInitializer<Output=[usize]>>{
///     a:u8,
///     b:INIT,
/// }
///
/// #[repr(C)]
/// struct FooFst{
///     a:u8,
///     b:PhantomData<[usize]>,
/// }
/// ```
/// You can also use it in nestly. With above Foo:
/// ```rust
/// #[dst]
/// struct Bar{
///     c:usize,
///     d:Foo
/// }
/// ```
/// For bar there will be 3 structs `Bar`,`BarInit`,`BarFst` after expansion.
/// The `BarInit` looks like this:
/// ```rust
/// #[repr(C)]
/// struct BarInit<INIT:EmplaceInitializer<Output=Foo>>{
///     c:usize,
///     d:INIT,
/// }
/// ```
///
/// # Use Case:
/// - 1 add simpler api
///
///   we usually provide a function to create the initializer
/// ```rust
/// #[dst]
/// struct SomePacket{
///     src:u32,
///     dst:u32,
///     options:[u8],
/// }
/// impl SomePacket{
///     fn initializer<Init:EmplaceInitializer<Output=[u8]>>(src:u32,dst:u32,init:Init)->SomePacketInit<Init>{
///         SomePacketInit{
///             src,
///             dst,
///             options:init
///         }
///     }
/// }
/// ```
///
#[proc_macro_attribute]
pub fn dst(_attr:TokenStream, input:TokenStream) -> TokenStream{
    let item_struct:ItemStruct = syn::parse(input.clone()).unwrap();
    let struct_name = item_struct.ident.clone();
    let mut struct_generics_param = item_struct.generics.params.clone();
    let struct_where_clause = item_struct.generics.where_clause.clone();
    let dst_type = item_struct.fields.iter().last().unwrap().ty.clone();
    let field_num = item_struct.fields.iter().len();
    let dst_field:Member = item_struct.fields.iter().last().unwrap().ident
        .clone().map_or(Member::Unnamed(Index::from(field_num)),|i|{
        Member::Named(i)
    });

    if !struct_generics_param.trailing_punct()&&!struct_generics_param.is_empty(){
        struct_generics_param.push_punct(Comma::default());
    }

    let struct_generics_arg = struct_generics_param
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
    let new_struct:ItemStruct = syn::parse(new_struct.into()).unwrap();

    let mut fst_struct = new_struct.clone();
    let fst_ident = format_ident!("{}Fst",struct_name);
    fst_struct.ident = fst_ident.clone();
    fst_struct.fields.iter_mut()
        .last().unwrap().ty = syn::parse(quote!(core::marker::PhantomData< #dst_type >).into()).unwrap();

    let mut init_struct = new_struct.clone();
    let init_ident = format_ident!("{}Init",struct_name.to_string());
    init_struct.ident = init_ident.clone();
    init_struct.generics.params
        .push(GenericParam::Type(syn::parse(quote!(INIT:dst_init::EmplaceInitializer<Output=#dst_type>).into()).unwrap()));
    init_struct.fields.iter_mut()
        .last().unwrap().ty = syn::parse(quote!(INIT).into()).unwrap();

    let init_ident = init_struct.ident.clone();
    let impl_emplace:ItemImpl = syn::parse(quote!(
        impl<#struct_generics_param INIT:dst_init::EmplaceInitializer<Output=#dst_type>> dst_init::EmplaceInitializer for #init_ident<#struct_generics_arg INIT>
            #struct_where_clause
        {
            type Output = #struct_name<#struct_generics_arg>;
            #[inline(always)]
            fn layout(&mut self) -> core::alloc::Layout{
                use core::alloc::Layout;
                let layout = Layout::new::<#fst_ident<#struct_generics_arg>>();
                layout
                    .extend(self.#dst_field.layout())
                    .unwrap()
                    .0
                    .pad_to_align()
            }

            #[inline(always)]
            fn emplace(mut self, ptr: core::ptr::NonNull<u8>) -> core::ptr::NonNull<Self::Output>{unsafe{
                use core::ptr;
                use core::ptr::NonNull;
                use core::alloc::Layout;
                use core::mem;
                use dst_init::EmplaceInitializer;

                let fst_layout = Layout::new::<#fst_ident<#struct_generics_arg>>();
                let dst_layout = self.#dst_field.layout();
                let dst = ptr
                    .as_ptr()
                    .add(mem::size_of::<#fst_ident<#struct_generics_arg>>())
                    .add(fst_layout.padding_needed_for(dst_layout.align()));
                let fst = ptr::read(&self as *const Self as *const _);
                let dst_init = ptr::read(&self.#dst_field as *const INIT);
                mem::forget(self);
                ptr.as_ptr().cast::<#fst_ident<#struct_generics_arg>>().write(fst);
                let (_, meta) = dst_init
                    .emplace(NonNull::new(dst.cast()).unwrap())
                    .to_raw_parts();
                mem::transmute(NonNull::<#dst_type>::from_raw_parts(ptr.cast(), meta))
            }}
        }
    ).into()).unwrap();

    let impl_init:ItemImpl = syn::parse(quote!(
        impl<#struct_generics_param DstInit:dst_init::EmplaceInitializer<Output=#dst_type>> dst_init::Initializer<DstInit> for #struct_name<#struct_generics_arg>
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
