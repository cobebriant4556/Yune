use std::{cell::RefCell, collections::BTreeMap, future::poll_fn, rc::{Rc, Weak}, task::{Poll, Waker}};
use mlua::prelude::*;
use mlua_luau_scheduler::LuaSchedulerExt;

#[derive(Default)]
struct Pending { values: Option<LuaMultiValue>, waker: Option<Waker> }
#[derive(Default)]
struct Inner {
    next_id: u64,
    callbacks: BTreeMap<u64, (LuaFunction, bool)>,
    waiters: Vec<Weak<RefCell<Pending>>>,
}
#[derive(Clone, Default)]
pub struct AudioSignal { inner: Rc<RefCell<Inner>> }

impl AudioSignal {
    pub fn fire(&self, lua: &Lua, args: LuaMultiValue) -> LuaResult<()> {
        let (callbacks, waiters) = {
            let mut inner = self.inner.borrow_mut();
            let callbacks = inner.callbacks.values().cloned().collect::<Vec<_>>();
            inner.callbacks.retain(|_, (_, once)| !*once);
            (callbacks, std::mem::take(&mut inner.waiters))
        };
        for (function, _) in callbacks { lua.push_thread_back(function, args.clone())?; }
        for waiter in waiters.into_iter().filter_map(|waiter| waiter.upgrade()) {
            let mut pending = waiter.borrow_mut();
            pending.values = Some(args.clone());
            if let Some(waker) = pending.waker.take() { waker.wake(); }
        }
        Ok(())
    }
    fn connect(&self, function: LuaFunction, once: bool) -> AudioConnection {
        let mut inner = self.inner.borrow_mut();
        inner.next_id += 1;
        let id = inner.next_id;
        inner.callbacks.insert(id, (function, once));
        AudioConnection { inner: Rc::downgrade(&self.inner), id }
    }
}

impl LuaUserData for AudioSignal {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("Connect", |_, this, callback: LuaFunction| Ok(this.connect(callback, false)));
        methods.add_method("Once", |_, this, callback: LuaFunction| Ok(this.connect(callback, true)));
        methods.add_async_method("Wait", |_, this, (): ()| async move {
            let pending = Rc::new(RefCell::new(Pending::default()));
            this.inner.borrow_mut().waiters.push(Rc::downgrade(&pending));
            poll_fn(move |context| {
                let mut pending = pending.borrow_mut();
                if let Some(values) = pending.values.take() { Poll::Ready(Ok(values)) }
                else { pending.waker = Some(context.waker().clone()); Poll::Pending }
            }).await
        });
    }
}

struct AudioConnection { inner: Weak<RefCell<Inner>>, id: u64 }
impl LuaUserData for AudioConnection {
    fn add_fields<F: LuaUserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("Connected", |_, this| {
            Ok(this.inner.upgrade().is_some_and(|inner| inner.borrow().callbacks.contains_key(&this.id)))
        });
    }
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("Disconnect", |_, this, (): ()| {
            if let Some(inner) = this.inner.upgrade() { inner.borrow_mut().callbacks.remove(&this.id); }
            Ok(())
        });
    }
}
