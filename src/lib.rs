use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

#[derive(Debug)]
pub struct Rx<T> {
    value: T,
    dependents: RefCell<Vec<(u64, Weak<Dependent>)>>,
}

impl<T: Clone> Clone for Rx<T> {
    fn clone(&self) -> Self {
        Rx {
            value: self.value.clone(),
            dependents: RefCell::new(Vec::new()),
        }
    }
}

impl<T: Clone> Rx<T> {
    pub fn new(value: T) -> Self {
        Rx {
            value,
            dependents: RefCell::new(Vec::new()),
        }
    }

    pub fn get(&self, ctx: &RxCtx) -> &T {
        let mut dependents = self.dependents.borrow_mut();

        let mut push = true;

        dependents.retain_mut(|(gen, d)| {
            let Some(dependent) = d.upgrade() else {
                // filter out dependents that no longer exist
                return false;
            };

            if Rc::ptr_eq(&dependent, ctx.dependent) {
                *gen = ctx.dependent.generation.get();
                push = false;
            }

            true
        });

        if push {
            dependents.push((ctx.dependent.generation.get(), Rc::downgrade(ctx.dependent)));
        }

        &self.value
    }

    pub fn get_untracked(&self) -> &T {
        &self.value
    }

    pub fn get_mut(&mut self) -> &mut T {
        fn mark_dirty(dependents: &RefCell<Vec<(u64, Weak<Dependent>)>>) {
            dependents.borrow_mut().retain(|(gen, d)| {
                let Some(dependent) = d.upgrade() else {
                    return false;
                };

                // filter out things that are no longer dependent
                if dependent.generation.get() > *gen {
                    return false;
                }

                dependent.dirty.set(true);

                mark_dirty(&dependent.dependents);

                true
            });
        }

        mark_dirty(&self.dependents);

        &mut self.value
    }
}

#[derive(Debug)]
pub struct RxFn<I: PartialEq, O> {
    last_input: Option<I>,
    result: Option<O>,
    this: Rc<Dependent>,
}

impl<I: PartialEq, O> Default for RxFn<I, O> {
    fn default() -> Self {
        RxFn::new()
    }
}

// TODO: Add a test and comment that explains the reasoning for this.
impl<I: PartialEq, O> Clone for RxFn<I, O> {
    fn clone(&self) -> Self {
        Self::new()
    }
}

impl<I: PartialEq, O> RxFn<I, O> {
    pub fn new() -> Self {
        RxFn {
            last_input: None,
            result: None,
            this: Rc::new(Dependent {
                generation: Cell::new(0),
                dirty: Cell::new(true),
                dependents: RefCell::new(Vec::new()),
            }),
        }
    }

    pub fn call(&mut self, ctx: &RxCtx, params: I, mut closure: impl FnMut(&RxCtx, &I) -> O) -> &O {
        let mut dependents = self.this.dependents.borrow_mut();

        let mut push = true;

        dependents.retain_mut(|(gen, d)| {
            let Some(dependent) = d.upgrade() else {
                // filter out dependents that no longer exist
                return false;
            };

            if Rc::ptr_eq(&dependent, ctx.dependent) {
                *gen = ctx.dependent.generation.get();
                push = false;
            }

            true
        });

        if push {
            dependents.push((ctx.dependent.generation.get(), Rc::downgrade(ctx.dependent)));
        }

        // Maybe != is not quite right here because we don't want trigger a re-run every time a NaN
        // gets passed.
        // The unwrap works because the whole thing starts out dirty and after that there's always
        // something in the option.
        if self.this.dirty.get() || self.last_input.as_ref().unwrap() != &params {
            let params: &I = self.last_input.insert(params);
            self.this.dirty.set(false);
            self.this.generation.set(self.this.generation.get() + 1);

            let result = self.result.insert(closure(
                &RxCtx {
                    dependent: &self.this,
                },
                params,
            ));

            result
        } else {
            self.result.as_ref().unwrap()
        }
    }
}

#[derive(Debug)]
pub struct Effect {
    this: Rc<Dependent>,
}

impl Effect {
    pub fn new() -> Self {
        Effect {
            this: Rc::new(Dependent {
                generation: Cell::new(0),
                dirty: Cell::new(true),
                dependents: RefCell::new(Vec::new()),
            }),
        }
    }

    pub fn call(&mut self, ctx: &RxCtx, mut closure: impl FnMut(&RxCtx)) {
        let mut dependents = self.this.dependents.borrow_mut();

        let mut push = true;

        dependents.retain_mut(|(gen, d)| {
            let Some(dependent) = d.upgrade() else {
                // filter out dependents that no longer exist
                return false;
            };

            if Rc::ptr_eq(&dependent, ctx.dependent) {
                *gen = ctx.dependent.generation.get();
                push = false;
            }

            true
        });

        if push {
            dependents.push((ctx.dependent.generation.get(), Rc::downgrade(ctx.dependent)));
        }

        if self.this.dirty.get() {
            self.this.dirty.set(false);
            self.this.generation.set(self.this.generation.get() + 1);

            closure(&RxCtx {
                dependent: &self.this,
            });
        }
    }
}

pub struct RxCtx<'a> {
    dependent: &'a Rc<Dependent>,
}

#[derive(Debug)]
pub struct Dependent {
    generation: Cell<u64>,
    dirty: Cell<bool>,
    dependents: RefCell<Vec<(u64, Weak<Dependent>)>>,
}

impl Dependent {
    pub fn toplevel() -> Rc<Self> {
        Rc::new(Dependent {
            generation: Cell::new(0),
            dirty: Cell::new(true),
            dependents: RefCell::new(Vec::new()),
        })
    }

    pub fn ctx<'a>(self: &'a Rc<Self>) -> RxCtx<'a> {
        RxCtx { dependent: self }
    }

    pub fn dirty(&self) -> bool {
        self.dirty.get()
    }

    pub fn set_clean(&self) {
        self.dirty.set(false);
    }
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

        fn layout(ctx: &RxCtx, state: &mut MyState, width: f64) -> f64 {
            *state.layout.call(ctx, width, |ctx, width| {
                let height = state.something.get(ctx) / width;

                height
            })
        }

        let mut state = MyState {
            something: Rx::new(128.),
            layout: RxFn::new(),
        };

        let dependent = Dependent::toplevel();
        let ctx = &dependent.ctx();

        assert_eq!(layout(ctx, &mut state, 2.), 64.);

        // let ctx = RxCtx {}; // TODO: figure out where that comes from or if it's needed at all
        *state.something.get_mut() = 64.;

        assert_eq!(layout(ctx, &mut state, 2.), 32.);
    }

    #[test]
    fn test_last_input_storage() {
        let times_called = Cell::new(0);

        let mut f = RxFn::new();
        let mut something = |ctx, num: u32| -> bool {
            *f.call(ctx, num, |_ctx, num| {
                times_called.set(times_called.get() + 1);
                num & 1 == 0
            })
        };

        let dependent = Dependent::toplevel();
        let ctx = &dependent.ctx();

        assert!(!something(ctx, 1));
        assert_eq!(times_called.get(), 1);
        assert!(!something(ctx, 1));
        assert_eq!(times_called.get(), 1);
        assert!(something(ctx, 310));
        assert_eq!(times_called.get(), 2);
    }

    #[test]
    fn test_dependency_change() {
        let mut a = Rx::new(true);
        let mut b = Rx::new(2);

        let mut f = RxFn::new();
        let mut something = |ctx, a: &mut Rx<bool>, b: &mut Rx<u32>| -> bool {
            *f.call(ctx, (), |ctx, ()| *a.get(ctx) || *b.get(ctx) > 3)
        };

        let dependent = Dependent::toplevel();
        let ctx = &dependent.ctx();

        assert!(something(ctx, &mut a, &mut b));
        assert_eq!(a.dependents.borrow().len(), 1);
        assert_eq!(b.dependents.borrow().len(), 0);

        *a.get_mut() = false;

        assert!(!something(ctx, &mut a, &mut b));
        assert_eq!(a.dependents.borrow().len(), 1);
        assert_eq!(b.dependents.borrow().len(), 1);

        *a.get_mut() = true;

        assert!(something(ctx, &mut a, &mut b));
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

        fn inner_layout(ctx: &RxCtx, state: &mut Inner, width: f64) -> f64 {
            *state.layout.call(ctx, width, |ctx, width| {
                if *state.a.get(ctx) && *width > 0. {
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

        fn layout(ctx: &RxCtx, state: &mut MyState, width: f64) -> f64 {
            *state.layout.call(ctx, width, |ctx, width| {
                let height = state.something.get(ctx) / width
                    + inner_layout(ctx, &mut state.inner, width - 1.);

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

        let dependent = Dependent::toplevel();
        let ctx = &dependent.ctx();

        assert_eq!(layout(ctx, &mut state, 2.), 84.);

        *state.inner.a.get_mut() = false;

        assert_eq!(layout(ctx, &mut state, 2.), 94.);
    }
}
