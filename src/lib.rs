use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

pub struct Rx<T> {
    value: T,
    dependents: RefCell<Vec<Weak<Cell<bool>>>>,
}

impl<T: Copy> Rx<T> {
    pub fn new(value: T) -> Self {
        Rx {
            value,
            dependents: RefCell::new(Vec::new()),
        }
    }

    pub fn read(&self, ctx: &RxCtx) -> T {
        let mut dependents = self.dependents.borrow_mut();

        let mut weak = Some(Rc::downgrade(ctx.dirty));

        dependents.retain_mut(|d| {
            let Some(dependent) = d.upgrade() else {
                // Filter out old references to closures that have since been re-run and don't
                // depend on this value any more.
                return false;
            };

            if Rc::ptr_eq(&dependent, ctx.old) {
                *d = weak
                    .take()
                    .expect("there should be no duplicate dependents");
            }

            true
        });

        if let Some(weak) = weak {
            dependents.push(weak);
        }

        self.value
    }

    pub fn get_mut(&mut self) -> &mut T {
        self.dependents.borrow_mut().retain(|d| {
            let Some(dependent) = d.upgrade() else {
                return false;
            };

            dependent.set(true);

            true
        });

        &mut self.value
    }
}

pub struct RxFn<I: Copy + PartialEq, O> {
    last_input: Option<I>,
    result: Option<O>,
    dirty: Rc<Cell<bool>>,
}

impl<I: Copy + PartialEq, O> RxFn<I, O> {
    pub fn new() -> Self {
        RxFn {
            last_input: None,
            result: None,
            dirty: Rc::new(Cell::new(true)),
        }
    }

    pub fn call(&mut self, params: I, mut closure: impl FnMut(&RxCtx, I) -> O) -> &O {
        // Maybe != is not quite right here because we don't want trigger a re-run every time a NaN
        // gets passed.
        // The unwrap works because the whole thing starts out dirty and after that there's always
        // something in the option.
        if self.dirty.get() || self.last_input.unwrap() != params {
            self.last_input = Some(params);

            // A generation counter might be a good alternative here that doesn't need to do an
            // allocation whenever it changes.
            let old = std::mem::replace(&mut self.dirty, Rc::new(Cell::new(false)));

            let result = self.result.insert(closure(
                &RxCtx {
                    old: &old,
                    dirty: &self.dirty,
                },
                params,
            ));

            result
        } else {
            self.result.as_ref().unwrap()
        }
    }
}

pub struct RxCtx<'a> {
    old: &'a Rc<Cell<bool>>,
    dirty: &'a Rc<Cell<bool>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test() {
        struct MyState {
            something: Rx<f64>,
            layout: RxFn<f64, f64>,
        }

        fn layout(state: &mut MyState, width: f64) -> f64 {
            *state.layout.call(width, |ctx, width| {
                let height = state.something.read(ctx) / width;

                height
            })
        }

        let mut state = MyState {
            something: Rx::new(128.),
            layout: RxFn::new(),
        };

        assert_eq!(layout(&mut state, 2.), 64.);

        // let ctx = RxCtx {}; // TODO: figure out where that comes from or if it's needed at all
        *state.something.get_mut() = 64.;

        assert_eq!(layout(&mut state, 2.), 32.);
    }

    #[test]
    fn test_last_input_storage() {
        let times_called = Cell::new(0);

        let mut f = RxFn::new();
        let mut something = |num: u32| -> bool {
            *f.call(num, |_ctx, num| {
                times_called.set(times_called.get() + 1);
                num & 1 == 0
            })
        };

        assert!(!something(1));
        assert_eq!(times_called.get(), 1);
        assert!(!something(1));
        assert_eq!(times_called.get(), 1);
        assert!(something(310));
        assert_eq!(times_called.get(), 2);
    }

    #[test]
    fn test_dependency_change() {
        let mut a = Rx::new(true);
        let mut b = Rx::new(2);

        let mut f = RxFn::new();
        let mut something = |a: &mut Rx<bool>, b: &mut Rx<u32>| -> bool {
            *f.call((), |ctx, ()| a.read(ctx) || b.read(ctx) > 3)
        };

        assert!(something(&mut a, &mut b));
        assert_eq!(a.dependents.borrow().len(), 1);
        assert_eq!(b.dependents.borrow().len(), 0);

        *a.get_mut() = false;

        assert!(!something(&mut a, &mut b));
        assert_eq!(a.dependents.borrow().len(), 1);
        assert_eq!(b.dependents.borrow().len(), 1);

        *a.get_mut() = true;

        assert!(something(&mut a, &mut b));
        assert_eq!(a.dependents.borrow().len(), 1);
        assert_eq!(b.dependents.borrow().len(), 1);

        *b.get_mut() = 513;

        // After b has been mutated it should become aware that f is no longer dependent on it.
        assert_eq!(b.dependents.borrow().len(), 0);
    }

    #[test]
    fn test_nested() {
        struct Inner {
            a: Rx<bool>,
            layout: RxFn<f64, f64>,
        }

        fn inner_layout(state: &mut Inner, width: f64) -> f64 {
            *state.layout.call(width, |ctx, width| {
                if state.a.read(ctx) && width > 0. {
                    20.
                } else {
                    30.
                }
            })
        }

        struct MyState {
            // not sure if this should be an Rx
            inner: Inner,
            something: Rx<f64>,
            layout: RxFn<f64, f64>,
        }

        fn layout(state: &mut MyState, width: f64) -> f64 {
            *state.layout.call(width, |ctx, width| {
                let height =
                    state.something.read(ctx) / width + inner_layout(&mut state.inner, width - 1.);

                height
            })
        }

        let mut state = MyState {
            inner: Inner {
                a: Rx::new(true),
                layout: RxFn::new(),
            },
            something: Rx::new(128.),
            layout: RxFn::new(),
        };

        assert_eq!(layout(&mut state, 2.), 84.);
    }
}
