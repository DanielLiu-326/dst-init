# dst-init
A library for rust to provide ways to emplace dynamic sized type

Example:

```rust
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
```

