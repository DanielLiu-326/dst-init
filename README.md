# dst-init
A library for rust to provide ways to emplace dynamic sized type
```rust
#![feature(alloc_layout_extra)]
#![feature(ptr_metadata)]

use dst_init_macros::dst;
use dst_init::{BoxExt, Slice, SliceExt};
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

let t = TestInit {
    a: 1usize,
    b: 1u8,
    c: 1u8,
    dst: Slice::iter_init(3, (0..).map(|i| (i as u8, i as usize))),
};
let u = Test1Init { a: 1usize, t };
let a = Box::emplace(u);
assert_eq!(a.a,1usize);
assert_eq!(a.t.a,1);
assert_eq!(a.t.b,1);
assert_eq!(a.t.c,1);
assert_eq!(a.t.dst,[(0,0),(1,1),(2,2)]);

```