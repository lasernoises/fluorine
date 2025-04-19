use std::{
    cell::{Cell, RefCell},
    marker::PhantomData,
    rc::{Rc, Weak},
};

// https://cliffle.com/blog/not-thread-safe/
#[derive(Debug)]
pub struct Id(u64, PhantomData<*const u8>);

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct IdRef(u64, PhantomData<*const u8>);

impl Id {
    fn new() -> Self {
        thread_local! {
            static NEXT_ID: Cell<u64> = Cell::new(0);
        }

        Id(NEXT_ID.replace(NEXT_ID.get() + 1), PhantomData)
    }
}

impl From<&Id> for IdRef {
    fn from(value: &Id) -> Self {
        IdRef(value.0, PhantomData)
    }
}

impl PartialEq<Id> for IdRef {
    fn eq(&self, other: &Id) -> bool {
        self.0 == other.0
    }
}

#[derive(Debug, Default)]
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

impl<T> Rx<T> {
    pub fn new(value: T) -> Self {
        Rx {
            value,
            dependents: RefCell::new(Vec::new()),
        }
    }

    pub fn get(&self, ctx: &mut RxCtx) -> &T {
        track(ctx, &self.dependents);

        &self.value
    }

    pub fn get_untracked(&self) -> &T {
        &self.value
    }

    pub fn get_mut(&mut self) -> &mut T {
        mark_dirty(&self.dependents);

        &mut self.value
    }
}

#[derive(Debug)]
pub struct RxVec<T> {
    id: Id,
    content: Vec<RxVecValue<T>>,
    dependents: RefCell<Vec<(u64, Weak<Dependent>)>>,
}

#[derive(Debug)]
pub struct RxVecValue<T> {
    id: Id,
    pub value: T,
}

impl<T> RxVecValue<T> {
    pub fn id(&self) -> IdRef {
        (&self.id).into()
    }
}

impl<T: Clone> Clone for RxVec<T> {
    fn clone(&self) -> Self {
        RxVec {
            id: Id::new(),
            content: self
                .content
                .iter()
                .map(|v| RxVecValue {
                    id: Id::new(),
                    value: v.value.clone(),
                })
                .collect(),
            dependents: RefCell::new(Vec::new()),
        }
    }
}

impl<T> Default for RxVec<T> {
    fn default() -> Self {
        RxVec::new()
    }
}

impl<T> RxVec<T> {
    pub fn new() -> Self {
        RxVec {
            id: Id::new(),
            content: Vec::new(),
            dependents: RefCell::new(Vec::new()),
        }
    }

    pub fn id(&self) -> IdRef {
        (&self.id).into()
    }

    pub fn push(&mut self, value: T) {
        mark_dirty(&self.dependents);

        self.content.push(RxVecValue {
            id: Id::new(),
            value,
        });
    }

    pub fn as_slice(&self, ctx: &mut RxCtx) -> &[RxVecValue<T>] {
        track(ctx, &self.dependents);

        &self.content
    }

    pub fn get(&self, ctx: &mut RxCtx, index: usize) -> Option<&T> {
        track(ctx, &self.dependents);

        self.content.get(index).map(|v| &v.value)
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        self.content.get_mut(index).map(|v| &mut v.value)
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

    pub fn call(
        &mut self,
        ctx: &mut RxCtx,
        params: I,
        mut closure: impl FnMut(&mut RxCtx, &I) -> O,
    ) -> &O {
        track(ctx, &self.this.dependents);

        // Maybe != is not quite right here because we don't want trigger a re-run every time a NaN
        // gets passed.
        // The unwrap works because the whole thing starts out dirty and after that there's always
        // something in the option.
        if self.this.dirty.get() || self.last_input.as_ref().unwrap() != &params {
            let params: &I = self.last_input.insert(params);
            self.this.dirty.set(false);
            self.this.generation.set(self.this.generation.get() + 1);

            let result = self.result.insert(closure(
                &mut RxCtx {
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

    pub fn call(&mut self, ctx: &mut RxCtx, mut closure: impl FnMut(&mut RxCtx)) {
        track(ctx, &self.this.dependents);

        if self.this.dirty.get() {
            self.this.dirty.set(false);
            self.this.generation.set(self.this.generation.get() + 1);

            closure(&mut RxCtx {
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

/// Recursively mark all dependents and dependents of dependens as dirty.
fn mark_dirty(dependents: &RefCell<Vec<(u64, Weak<Dependent>)>>) {
    dependents.borrow_mut().retain(|(generation, d)| {
        let Some(dependent) = d.upgrade() else {
            return false;
        };

        // filter out things that are no longer dependent
        if dependent.generation.get() > *generation {
            return false;
        }

        dependent.dirty.set(true);

        mark_dirty(&dependent.dependents);

        true
    });
}

/// Adds the `dependent` of the `ctx` to `dependents`.
fn track(ctx: &mut RxCtx, dependents: &RefCell<Vec<(u64, Weak<Dependent>)>>) {
    let mut dependents = dependents.borrow_mut();

    let mut push = true;

    dependents.retain_mut(|(generation, d)| {
        let Some(dependent) = d.upgrade() else {
            // filter out dependents that no longer exist
            return false;
        };

        if Rc::ptr_eq(&dependent, ctx.dependent) {
            *generation = ctx.dependent.generation.get();
            push = false;
        }

        true
    });

    if push {
        dependents.push((ctx.dependent.generation.get(), Rc::downgrade(ctx.dependent)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_readme_example() {
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
    }

    #[test]
    fn test() {
        struct MyState {
            something: Rx<f64>,
            layout: RxFn<f64, f64>,
        }

        fn layout(ctx: &mut RxCtx, state: &mut MyState, width: f64) -> f64 {
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
        let ctx = &mut dependent.ctx();

        assert_eq!(layout(ctx, &mut state, 2.), 64.);

        // let ctx = RxCtx {}; // TODO: figure out where that comes from or if it's needed at all
        *state.something.get_mut() = 64.;

        assert_eq!(layout(ctx, &mut state, 2.), 32.);
    }

    #[test]
    fn test_last_input_storage() {
        let times_called = Cell::new(0);

        let mut f = RxFn::new();
        let mut something = |ctx: &mut RxCtx, num: u32| -> bool {
            *f.call(ctx, num, |_ctx, num| {
                times_called.set(times_called.get() + 1);
                num & 1 == 0
            })
        };

        let dependent = Dependent::toplevel();
        let ctx = &mut dependent.ctx();

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
        let mut something = |ctx: &mut RxCtx, a: &mut Rx<bool>, b: &mut Rx<u32>| -> bool {
            *f.call(ctx, (), |ctx, ()| *a.get(ctx) || *b.get(ctx) > 3)
        };

        let dependent = Dependent::toplevel();
        let ctx = &mut dependent.ctx();

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

        fn inner_layout(ctx: &mut RxCtx, state: &mut Inner, width: f64) -> f64 {
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

        fn layout(ctx: &mut RxCtx, state: &mut MyState, width: f64) -> f64 {
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
        let ctx = &mut dependent.ctx();

        assert_eq!(layout(ctx, &mut state, 2.), 84.);

        *state.inner.a.get_mut() = false;

        assert_eq!(layout(ctx, &mut state, 2.), 94.);
    }
}
