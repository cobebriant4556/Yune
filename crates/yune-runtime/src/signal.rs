use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::{Rc, Weak},
};

use mlua::prelude::*;

#[derive(Clone)]
pub struct Signal {
    inner: Rc<RefCell<SignalInner>>,
}

struct SignalInner {
    next_id: u64,
    callbacks: BTreeMap<u64, SignalCallback>,
}

struct SignalCallback {
    function: LuaFunction,
    once: bool,
}

impl Default for Signal {
    fn default() -> Self {
        Self {
            inner: Rc::new(RefCell::new(SignalInner {
                next_id: 1,
                callbacks: BTreeMap::new(),
            })),
        }
    }
}

impl Signal {
    pub fn fire1<T>(&self, arg: T) -> LuaResult<()>
    where
        T: IntoLua + Clone,
    {
        let callbacks = {
            let inner = self.inner.borrow();
            inner
                .callbacks
                .iter()
                .map(|(id, callback)| (*id, callback.function.clone(), callback.once))
                .collect::<Vec<_>>()
        };
        let mut remove = Vec::new();
        for (id, function, once) in callbacks {
            function.call::<()>(arg.clone())?;
            if once {
                remove.push(id);
            }
        }
        if !remove.is_empty() {
            let mut inner = self.inner.borrow_mut();
            for id in remove {
                inner.callbacks.remove(&id);
            }
        }
        Ok(())
    }

    pub fn fire2<A, B>(&self, a: A, b: B) -> LuaResult<()>
    where
        A: IntoLua + Clone,
        B: IntoLua + Clone,
    {
        let callbacks = {
            let inner = self.inner.borrow();
            inner
                .callbacks
                .iter()
                .map(|(id, callback)| (*id, callback.function.clone(), callback.once))
                .collect::<Vec<_>>()
        };
        let mut remove = Vec::new();
        for (id, function, once) in callbacks {
            function.call::<()>((a.clone(), b.clone()))?;
            if once {
                remove.push(id);
            }
        }
        if !remove.is_empty() {
            let mut inner = self.inner.borrow_mut();
            for id in remove {
                inner.callbacks.remove(&id);
            }
        }
        Ok(())
    }
}

impl LuaUserData for Signal {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("Connect", |_, this, function: LuaFunction| {
            let mut inner = this.inner.borrow_mut();
            let id = inner.next_id;
            inner.next_id += 1;
            inner.callbacks.insert(id, SignalCallback { function, once: false });
            Ok(Connection {
                signal: Rc::downgrade(&this.inner),
                id,
                connected: Rc::new(Cell::new(true)),
            })
        });

        methods.add_method("Once", |_, this, function: LuaFunction| {
            let mut inner = this.inner.borrow_mut();
            let id = inner.next_id;
            inner.next_id += 1;
            inner.callbacks.insert(id, SignalCallback { function, once: true });
            Ok(Connection {
                signal: Rc::downgrade(&this.inner),
                id,
                connected: Rc::new(Cell::new(true)),
            })
        });
    }
}

pub struct Connection {
    signal: Weak<RefCell<SignalInner>>,
    id: u64,
    connected: Rc<Cell<bool>>,
}

impl LuaUserData for Connection {
    fn add_fields<F: LuaUserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("Connected", |_, this| {
            if !this.connected.get() {
                return Ok(false);
            }
            let Some(signal) = this.signal.upgrade() else {
                return Ok(false);
            };
            Ok(signal.borrow().callbacks.contains_key(&this.id))
        });
    }

    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_method_mut("Disconnect", |_, this, ()| {
            if let Some(signal) = this.signal.upgrade() {
                signal.borrow_mut().callbacks.remove(&this.id);
            }
            this.connected.set(false);
            Ok(())
        });
    }
}
