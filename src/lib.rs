//! A library for rust to provide ways to emplace dynamic sized type
//! ```rust
//! #![feature(alloc_layout_extra)]
//! #![feature(ptr_metadata)]
//!
//! use dst_init_macros::dst;
//! use dst_init::{BoxExt, Slice, SliceExt};
//! #[dst]
//! #[derive(Debug)]
//! struct Test<A, B, C, D> {
//!     a: A,
//!     b: B,
//!     c: C,
//!     dst: [(C, D)],
//! }
//!
//! #[dst]
//! #[derive(Debug)]
//! struct Test1<A, B, C, D> {
//!     a: usize,
//!     t: Test<A, B, C, D>,
//! }
//!
//! let t = TestInit {
//!     a: 1usize,
//!     b: 1u8,
//!     c: 1u8,
//!     dst: Slice::iter_init(3, (0..).map(|i| (i as u8, i as usize))),
//! };
//! let u = Test1Init { a: 1usize, t };
//! let a = Box::emplace(u);
//! assert_eq!(a.a,1usize);
//! assert_eq!(a.t.a,1);
//! assert_eq!(a.t.b,1);
//! assert_eq!(a.t.c,1);
//! assert_eq!(a.t.dst,[(0,0),(1,1),(2,2)]);
//!
//! ```
#![feature(ptr_metadata)]
#![feature(unsize)]
#![feature(alloc_layout_extra)]
#![feature(allocator_api)]

pub mod alloc;

pub use dst_init_macros as macros;
pub use macros::dst;
use std::alloc::Layout;
use std::marker::{PhantomData, Unsize};
use std::ptr::{null, NonNull, Pointee};
use std::{mem, ptr};
use std::rc::Rc;
use std::sync::Arc;

type Metadata<T> = <T as Pointee>::Metadata;

#[inline(always)]
const fn metadata_of<T: Unsize<Dyn>, Dyn: ?Sized>() -> Metadata<Dyn> {
    let null: *const T = null();
    let dyn_null = null as *const Dyn;
    ptr::metadata(dyn_null)
}

pub trait Initializer<DstInit: EmplaceInitializer> {
    type Init;
}

pub type Init<T, DstInit> = <T as Initializer<DstInit>>::Init;

/// An abstract interface for all emplace initializer
pub trait EmplaceInitializer {
    type Output: ?Sized;
    /// Layout of the type
    fn layout(&mut self) -> Layout;
    /// Emplace the type in given memory
    fn emplace(self, ptr: NonNull<u8>) -> NonNull<Self::Output>;
}

/// An Emplace Initializer for Slice, created by iterator and member number.
pub struct SliceIterInitializer<Iter: Iterator> {
    size: usize,
    iter: Iter,
}

impl<Iter: Iterator> SliceIterInitializer<Iter> {
    /// Create a SliceIterInitializer by iterator and member number. iterator::next will be called
    /// for given member number times
    ///
    /// # Panics
    /// would panic if iterator has less item than given member number
    #[inline(always)]
    pub fn new(size: usize, iter: Iter) -> Self {
        Self { size, iter }
    }
}

impl<Iter: Iterator> EmplaceInitializer for SliceIterInitializer<Iter> {
    type Output = [Iter::Item];

    #[inline(always)]
    fn layout(&mut self) -> Layout {
        Layout::array::<Iter::Item>(self.size).unwrap()
    }

    #[inline(always)]
    fn emplace(mut self, ptr: NonNull<u8>) -> NonNull<Self::Output> {
        unsafe {
            let mut p: *mut Iter::Item = ptr.as_ptr().cast();
            for _ in 0..self.size {
                let item = self.iter.next().unwrap();
                p.write(item);
                p = p.add(1);
            }
            mem::transmute(NonNull::slice_from_raw_parts(
                ptr.cast::<Iter::Item>(),
                self.size,
            ))
        }
    }
}

/// An Emplace Initializer for Slice, created by closure and member number.
pub struct SliceFnInitializer<Item, F: FnMut() -> Item> {
    size: usize,
    f: F,
}

impl<Item, F: FnMut() -> Item> SliceFnInitializer<Item, F> {
    /// Create a SliceIterInitializer by closure and member number. Given closure will be called
    /// for given member number times
    #[inline(always)]
    pub fn new(size: usize, f: F) -> Self {
        Self { size, f }
    }
}

impl<Item, F: FnMut() -> Item> EmplaceInitializer for SliceFnInitializer<Item, F> {
    type Output = [Item];

    #[inline(always)]
    fn layout(&mut self) -> Layout {
        Layout::array::<Item>(self.size).unwrap()
    }

    #[inline(always)]
    fn emplace(mut self, ptr: NonNull<u8>) -> NonNull<Self::Output> {
        unsafe {
            let mut p: *mut Item = ptr.as_ptr().cast();
            for _ in 0..self.size {
                let item = (self.f)();
                p.write(item);
                p = p.add(1);
            }
            NonNull::slice_from_raw_parts(ptr.cast::<Item>(), self.size)
        }
    }
}

/// An Emplace Initializer for `dyn` or `[T]` types, created by concrete type `T` or `[T;N]`.
/// For example `usize` is sized type and implemented `Debug`:
///```rust
/// use std::fmt::Debug;
/// use dst_init::{BoxExt, CoercionInitializer};
///
/// let init:CoercionInitializer<usize,dyn Debug> = CoercionInitializer::new(1usize);
/// let boxed:Box<dyn Debug> = Box::emplace(init);
/// assert_eq!(format!("{:?}",boxed),"1")
///```
pub struct CoercionInitializer<T: Unsize<U>, U: ?Sized> {
    t: T,
    phan: PhantomData<U>,
}

impl<T: Unsize<U>, U: ?Sized> CoercionInitializer<T, U> {
    #[inline(always)]
    pub fn new(t: T) -> Self {
        Self {
            t,
            phan: Default::default(),
        }
    }
    #[inline(always)]
    pub fn fallback(self) -> T {
        self.t
    }
}

impl<T: Unsize<U>, U: ?Sized> EmplaceInitializer for CoercionInitializer<T, U> {
    type Output = U;

    #[inline(always)]
    fn layout(&mut self) -> Layout {
        Layout::new::<T>()
    }

    #[inline(always)]
    fn emplace(self, ptr: NonNull<u8>) -> NonNull<Self::Output> {
        unsafe {
            let meta = metadata_of::<T, U>();
            ptr.as_ptr().cast::<T>().write(self.t);
            NonNull::from_raw_parts(ptr, meta)
        }
    }
}

/// An Emplace Initializer for sized type, created by itself.
pub struct DirectInitializer<T> {
    t: T,
}

impl<T> DirectInitializer<T> {
    #[inline(always)]
    pub fn new(t: T) -> Self {
        Self { t }
    }

    #[inline(always)]
    pub fn fallback(self) -> T {
        self.t
    }
}

impl<T> EmplaceInitializer for DirectInitializer<T> {
    type Output = T;

    #[inline(always)]
    fn layout(&mut self) -> Layout {
        Layout::new::<T>()
    }

    #[inline(always)]
    fn emplace(self, ptr: NonNull<u8>) -> NonNull<Self::Output> {
        unsafe {
            ptr.as_ptr().cast::<T>().write(self.t);
            ptr.cast()
        }
    }
}

/// Abstract for type `Box`,`Rc` and etc to allocate value by EmplaceInitializer types.
pub trait BoxExt: Sized {

    type Output: ?Sized;

    /// Allocate memory by `std::alloc::alloc()` and emplace value in it
    /// Then use Self wrap it.
    fn emplace<Init: EmplaceInitializer<Output = Self::Output>>(init: Init) -> Self;
}

impl<T: ?Sized> BoxExt for Box<T> {

    type Output = T;

    /// Allocate memory by `std::alloc::alloc()` and emplace value in it
    /// Then use `Box` wrap it.
    fn emplace<Init: EmplaceInitializer<Output = Self::Output>>(
        mut init: Init,
    ) -> Box<Self::Output> {
        unsafe {
            let layout = init.layout();
            let mem = std::alloc::alloc(layout);
            let obj = init.emplace(NonNull::new(mem).unwrap());
            Box::from_raw(obj.as_ptr())
        }
    }
}

impl<T: ?Sized> BoxExt for Rc<T> {
    type Output = T;

    /// Allocate memory by `std::alloc::alloc()` and emplace value in it
    /// Then use `Rc` wrap it.
    fn emplace<Init: EmplaceInitializer<Output = Self::Output>>(
        mut init: Init,
    ) -> Rc<Self::Output> {
        unsafe {
            let layout = init.layout();
            let mem = std::alloc::alloc(layout);
            let obj = init.emplace(NonNull::new(mem).unwrap());
            Rc::from_raw(obj.as_ptr())
        }
    }
}

impl<T: ?Sized> BoxExt for Arc<T> {
    type Output = T;

    /// Allocate memory by `std::alloc::alloc()` and emplace value in it
    /// Then use `Arc` wrap it.
    fn emplace<Init: EmplaceInitializer<Output = Self::Output>>(
        mut init: Init,
    ) -> Arc<Self::Output> {
        unsafe {
            let layout = init.layout();
            let mem = std::alloc::alloc(layout);
            let obj = init.emplace(NonNull::new(mem).unwrap());
            Arc::from_raw(obj.as_ptr())
        }
    }
}

/// pub type Slice\<T\> = \[T\];
pub type Slice<T> = [T];

/// Extension for type `[T]` to create EmplaceInitializer.
pub trait SliceExt {
    type Item;

    /// create SliceFnInitializer
    fn fn_init<F>(size: usize, f: F) -> impl EmplaceInitializer<Output = [Self::Item]>
    where
        F: FnMut() -> Self::Item;

    /// create SliceIterInitializer
    fn iter_init<Iter>(size: usize, iter: Iter) -> impl EmplaceInitializer<Output = [Self::Item]>
    where
        Iter: Iterator<Item = Self::Item>;
}

impl<T> SliceExt for Slice<T> {
    type Item = T;

    /// create SliceFnInitializer
    #[inline(always)]
    fn fn_init<F>(size: usize, f: F) -> impl EmplaceInitializer<Output = [Self::Item]>
    where
        F: FnMut() -> Self::Item,
    {
        SliceFnInitializer::new(size, f)
    }

    /// create SliceIterInitializer
    fn iter_init<Iter>(size: usize, iter: Iter) -> impl EmplaceInitializer<Output = [Self::Item]>
    where
        Iter: Iterator<Item = Self::Item>,
    {
        SliceIterInitializer::new(size, iter)
    }
}

pub struct RawInitializer<Output:?Sized, F>{
    layout:Layout,
    emplacer:F,
    phan:PhantomData<Output>,
}

impl<Output, F> RawInitializer<Output,F>
    where Output:?Sized, F:FnOnce(NonNull<u8>)->NonNull<Output>
{
    pub fn new(layout:Layout, f:F)->Self{
        Self{
            layout,
            emplacer:f,
            phan:Default::default()
        }
    }
}

impl<Output, F> EmplaceInitializer for RawInitializer<Output, F>
    where Output:?Sized, F:FnOnce(NonNull<u8>)->NonNull<Output>
{
    type Output = Output;

    fn layout(&mut self) -> Layout {
        self.layout
    }

    fn emplace(self, ptr: NonNull<u8>) -> NonNull<Self::Output> {
        (self.emplacer)(ptr)
    }
}

#[cfg(test)]
pub mod test {
    use crate::{self as dst_init, RawInitializer};
    use crate::{
        CoercionInitializer, DirectInitializer, EmplaceInitializer,
        SliceFnInitializer, SliceIterInitializer,
    };
    use dst_init_macros::dst;
    use std::alloc;
    use std::alloc::Layout;
    use std::fmt::{Debug, Formatter};
    use std::ptr::NonNull;

    #[dst]
    #[derive(Debug)]
    struct Test<A, B, C, D> {
        a: A,
        b: B,
        c: C,
        dst: [(C, D)],
    }

    #[dst]
    #[derive(Debug)]
    struct Test1<A, B, C, D> {
        a: usize,
        t: Test<A, B, C, D>,
    }

    #[test]
    fn test() {
        let t = TestInit {
            a: 1usize,
            b: 1u8,
            c: 1u8,
            dst: SliceIterInitializer::new(3, (0..).map(|i| (i as u8, i as usize))),
        };
        let u = Test1Init { a: 1usize, t };
        let a = alloc(u);
        println!("{:?}", a)
    }

    fn alloc<O: ?Sized, Init: EmplaceInitializer<Output = O>>(mut init: Init) -> Box<O> {
        unsafe {
            let layout = init.layout();
            let ptr = alloc::alloc(layout);
            if ptr.is_null() {
                panic!("no memory");
            }
            let ptr = init.emplace(NonNull::new(ptr).unwrap());
            Box::from_raw(ptr.as_ptr())
        }
    }

    #[derive(PartialEq, Copy, Clone)]
    pub struct FstStruct {
        a: u8,
        b: usize,
        c: f64,
    }
    impl Debug for FstStruct {
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            write!(f, "{},{},{}", self.a, self.b, self.c)
        }
    }

    #[allow(dead_code)]
    #[derive(Debug)]
    pub struct DstStruct<Data: Debug + ?Sized> {
        a: u8,
        b: usize,
        c: u8,
        d: Data,
    }

    #[test]
    fn test_direct_initializer() {
        #[inline(never)]
        fn test<T: PartialEq + Debug>(a: T, b: T) {
            let mut init = DirectInitializer::new(a);
            let layout = init.layout();
            assert_eq!(layout, Layout::new::<T>());
            let obj = alloc(init);
            assert_eq!(*obj, b);
        }

        test(12345usize, 12345usize);
        test(127u8, 127u8);
        test(456131248u32, 456131248u32);
        test(4123456789u64, 4123456789u64);
        test(1.0f64, 1.0f64);

        let a = FstStruct {
            a: 159,
            b: 47521,
            c: 12345.12345,
        };
        test(a, a);
    }

    #[test]
    fn test_coercion_initializer() {
        let a = FstStruct {
            a: 159,
            b: 47521,
            c: 12345.12345,
        };
        let init = CoercionInitializer::new(a);
        let data: Box<dyn Debug> = alloc(init);
        assert_eq!(format!("{:?}", &*data), "159,47521,12345.12345");

        let create = || DstStruct {
            a: 156,
            b: 157,
            c: 175,
            d: [1usize, 13123usize, 13123usize],
        };
        let init = CoercionInitializer::new(create());
        let data: Box<DstStruct<[usize]>> = alloc(init);
        assert_eq!(format!("{:?}", data), format!("{:?}", create()));
    }

    #[test]
    fn test_slice_fn_initializer() {
        let mut i = 0;
        let init = SliceFnInitializer::new(10065, || {
            i += 1;
            i
        });
        let data = alloc(init);
        i = 1;
        for x in data.iter() {
            assert_eq!(i, *x);
            i += 1;
        }
    }

    #[test]
    fn test_slice_iter_initializer() {
        let init = SliceIterInitializer::new(100, 0..100);
        let data = alloc(init);
        for x in 0..100 {
            assert_eq!(data[x], x)
        }
    }

    #[test]
    fn test_raw_initializer(){
        let init = RawInitializer::new(Layout::new::<[u8;10]>(),|ptr|{unsafe{
            let mut ptr = ptr.as_ptr();
            let tmp = ptr;
            for x in 0..10{
                *ptr = x as u8;
                ptr = ptr.add(1);
            }
            return NonNull::new(std::ptr::slice_from_raw_parts_mut(tmp, 10)).expect("error when creating NonNull");
        }});
        let data = alloc(init);
        for x in 0..10 {
            assert_eq!(data[x], x as u8)
        }
    }
}
