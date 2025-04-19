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
