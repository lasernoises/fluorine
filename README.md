# Fluorine

Fluorine is a reactivity library for Rust.

## Example

```rust
let mut a = Rx::new(1.);

let mut f = RxFn::new();

let mut output = |ctx: &mut RxCtx, a: &Rx<f64>, multiplier: f64| -> f64 {
    *f.call(ctx, multiplier, |ctx, multiplier| a.get(ctx) * multiplier)
};

let dependent = Dependent::toplevel();
let ctx = &mut dependent.ctx();

// it gets evaluated here for the first time.
assert_eq!(output(ctx, &a, 2.), 2.);

// this uses the stored result.
assert_eq!(output(ctx, &a, 2.), 2.);

// it gets re-evaluated if the input changes
assert_eq!(output(ctx, &a, 17.), 17.);

// or if a dependency changes
*a.get_mut() = 3.;
assert_eq!(output(ctx, &a, 17.), 51.);
```

## Design

Fluorine is designed to enable the following properties:

**Doesn't stuff all your data into smart pointers.** Only tracking happens through smart pointers.
The `Rx` struct looks like this:

```rust
pub struct Rx<T> {
    value: T,
    dependents: RefCell<Vec<(u64, Weak<Dependent>)>>,
}
```

Since the tracking information always has the same type a future version might put all of the
tracking information into a single `Vec` and use indexes for the references between them.

**Doesn't force you to use interior mutability to mutate your data.** But using interior mutability
is also not a problem either. You can do what makes sense for your app. For an example of how to use
interior mutability see the spreadsheet example.

