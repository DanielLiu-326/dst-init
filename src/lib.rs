#![feature(ptr_metadata)]
#![feature(unsize)]
#![feature(alloc_layout_extra)]

use std::alloc::Layout;
use std::marker::{PhantomData, Unsize};
use std::{mem, ptr};
use std::ops::{Deref, DerefMut};
use std::ptr::{NonNull, null, Pointee};
pub use macros::dst;

type Metadata<T> = <T as Pointee>::Metadata;

#[inline(always)]
const fn metadata_of<T: Unsize<Dyn>, Dyn: ?Sized>() -> Metadata<Dyn> {
    let null: *const T = null();
    let dyn_null = null as *const Dyn;
    ptr::metadata(dyn_null)
}


pub trait EmplaceInitializer {
    type Output: ?Sized;
    fn layout(&mut self) -> Layout;
    fn emplace(self, ptr: NonNull<u8>) -> NonNull<Self::Output>;
}


pub struct SliceIterInitializer<Iter: Iterator> {
    size: usize,
    iter: Iter,
}

impl<Iter: Iterator> SliceIterInitializer<Iter> {
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

pub struct SliceFnInit<Item, F: FnMut() -> Item> {
    size: usize,
    f: F,
}

impl<Item, F: FnMut() -> Item> SliceFnInit<Item, F> {
    #[inline(always)]
    pub fn new(size: usize, f: F) -> Self {
        Self { size, f }
    }
}

impl<Item, F: FnMut() -> Item> EmplaceInitializer for SliceFnInit<Item, F> {
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
            NonNull::from_raw_parts(ptr.cast(), meta)
        }
    }
}

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


#[repr(C)]
pub struct Dst<FST, DST: ?Sized> {
    fst: FST,
    dst: DST,
}

impl<FST, DST> Dst<FST, DST> {
    #[inline(always)]
    fn new(header: FST, body: DST) -> Self {
        Self {
            fst: header,
            dst: body,
        }
    }
}

impl<FST, DST: ?Sized> Dst<FST, DST> {
    #[inline(always)]
    pub fn header(&self) -> &FST {
        &self.fst
    }
    #[inline(always)]
    pub fn header_mut(&mut self) -> &mut FST {
        &mut self.fst
    }
    #[inline(always)]
    pub fn body(&self) -> &DST {
        &self.dst
    }
    #[inline(always)]
    pub fn body_mut(&mut self) -> &mut DST {
        &mut self.dst
    }
}

impl<FST, DST: ?Sized> Deref for Dst<FST, DST> {
    type Target = FST;

    fn deref(&self) -> &Self::Target {
        self.header()
    }
}
impl<FST, DST: ?Sized> DerefMut for Dst<FST, DST> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.header_mut()
    }
}

pub struct DstInitializer<FST, DST: ?Sized, INIT: EmplaceInitializer<Output = DST>> {
    dst_init: INIT,
    fst: FST,
}

impl<FST, DST: ?Sized, INIT: EmplaceInitializer<Output = DST>> DstInitializer<FST, DST, INIT> {
    #[inline(always)]
    pub fn new(fst: FST, dst_init: INIT) -> Self {
        Self { dst_init, fst }
    }
    #[inline(always)]
    pub fn fallback(self) -> (FST, INIT) {
        (self.fst, self.dst_init)
    }
}

impl<FST, DST: ?Sized, INIT: EmplaceInitializer<Output = DST>> EmplaceInitializer
for DstInitializer<FST, DST, INIT>
{
    type Output = Dst<FST, DST>;

    #[inline(always)]
    fn layout(&mut self) -> Layout {
        let layout = Layout::new::<FST>();
        layout
            .extend(self.dst_init.layout())
            .unwrap()
            .0
            .pad_to_align()
    }

    #[inline(always)]
    fn emplace(mut self, ptr: NonNull<u8>) -> NonNull<Self::Output> {
        unsafe {
            let fst_layout = Layout::new::<FST>();
            let dst_layout = self.dst_init.layout();
            let dst = ptr
                .as_ptr()
                .add(mem::size_of::<FST>())
                .add(fst_layout.padding_needed_for(dst_layout.align()));
            let DstInitializer { dst_init, fst } = self;
            ptr.as_ptr().cast::<FST>().write(fst);
            let (_, meta) = dst_init
                .emplace(NonNull::new(dst.cast()).unwrap())
                .to_raw_parts();
            mem::transmute(NonNull::<DST>::from_raw_parts(ptr.cast(), meta))
        }
    }
}


#[cfg(test)]
pub mod test {
    use crate::{CoercionInitializer, DirectInitializer, Dst, DstInitializer, EmplaceInitializer, SliceFnInit, SliceIterInitializer};
    use std::alloc::Layout;
    use std::fmt::{Debug, DebugStruct, Formatter};
    use std::ptr::NonNull;
    use std::{alloc, mem};

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
        let init = SliceFnInit::new(10065, || {
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
    fn test_dst_initializer() {
        let mut init = DstInitializer::new(100u8, SliceIterInitializer::new(100, 0..100usize));
        assert_eq!(init.layout(), Layout::new::<Dst<u8, [usize; 100]>>());
        let data = alloc(init);
        let mut i = 0;
        for x in &data.dst {
            assert_eq!(i, *x);
            i += 1;
        }
    }
}
