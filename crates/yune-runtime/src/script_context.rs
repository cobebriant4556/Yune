use std::{cell::RefCell, collections::{HashMap, HashSet}, future::poll_fn, rc::{Rc, Weak}, task::{Poll, Waker}};
use lune_roblox::instance::{Instance, instance_to_lua};
use mlua::prelude::*;
use mlua_luau_scheduler::{LuaSchedulerExt, LuaSpawnExt};
use rbx_dom_weak::types::Variant;

struct ScriptRecord {
    thread: LuaThread,
    run_context: u32,
}

#[derive(Default)]
struct ModuleJob {
    result: RefCell<Option<LuaResult<LuaValue>>>,
    waiters: RefCell<Vec<Waker>>,
}

impl ModuleJob {
    fn finish(&self, result: LuaResult<LuaValue>) {
        *self.result.borrow_mut() = Some(result);
        let waiters = std::mem::take(&mut *self.waiters.borrow_mut());
        for waker in waiters { waker.wake(); }
    }

    async fn wait(&self) -> LuaResult<LuaValue> {
        poll_fn(|cx| {
            if let Some(result) = self.result.borrow().as_ref() { return Poll::Ready(result.clone()); }
            let mut waiters = self.waiters.borrow_mut();
            if !waiters.iter().any(|waker| waker.will_wake(cx.waker())) { waiters.push(cx.waker().clone()); }
            Poll::Pending
        }).await
    }
}

#[derive(Default)]
pub struct ScriptContext {
    scripts: HashMap<String, ScriptRecord>,
    modules: HashMap<String, Rc<ModuleJob>>,
    dependencies: HashMap<String, HashSet<String>>,
    original_require: Option<LuaFunction>,
}

impl ScriptContext {
    pub fn install(lua: &Lua, context: Rc<RefCell<Self>>) -> LuaResult<()> {
        context.borrow_mut().original_require = Some(lua.globals().get("require")?);
        lua.globals().set("require", make_require(lua, context, None)?)
    }

    pub fn discover(&mut self, lua: &Lua, game: Instance) -> LuaResult<usize> {
        let candidates = game.get_descendants_preorder().into_iter()
            .filter(|script| matches!(script.get_class_name(), "Script" | "LocalScript"))
            .collect::<Vec<_>>();
        let live = candidates.iter().map(|script| (instance_key(script), *script)).collect::<HashMap<_, _>>();
        let removed = self.scripts.iter().filter_map(|(key, record)| {
            let keep = live.get(key).is_some_and(|script| script_enabled(script) && run_context(script) == record.run_context);
            (!keep).then_some(key.clone())
        }).collect::<Vec<_>>();
        let task: LuaTable = lua.globals().get("task")?;
        let cancel: LuaFunction = task.get("cancel")?;
        for key in removed {
            if let Some(record) = self.scripts.remove(&key) {
                if record.thread.status() == LuaThreadStatus::Resumable { cancel.call::<()>(record.thread)?; }
            }
        }

        let mut scheduled = 0;
        for script in candidates {
            let key = instance_key(&script);
            if self.scripts.contains_key(&key) || !script_enabled(&script) { continue; }
            let Some(source) = script_source(&script) else { continue };
            let environment = script_environment(lua, script)?;
            let function = lua.load(source).set_name(format!("@{}", script.get_full_name()))
                .set_environment(environment).into_function()?;
            let thread = lua.create_thread(function)?;
            lua.push_thread_back(thread.clone(), ())?;
            self.scripts.insert(key, ScriptRecord { thread, run_context: run_context(&script) });
            scheduled += 1;
        }
        Ok(scheduled)
    }

    pub fn reset(&mut self) {
        self.scripts.clear();
        self.modules.clear();
        self.dependencies.clear();
    }
}

fn make_require(lua: &Lua, context: Rc<RefCell<ScriptContext>>, caller: Option<String>) -> LuaResult<LuaFunction> {
    let original = context.borrow().original_require.clone()
        .ok_or_else(|| LuaError::runtime("ScriptContext require is not installed"))?;
    lua.create_async_function(move |lua, value: LuaValue| {
        let original = original.clone();
        let context = context.clone();
        let caller = caller.clone();
        async move {
            let instance = match &value {
                LuaValue::UserData(userdata) => userdata.borrow::<Instance>().ok().map(|instance| *instance),
                _ => None,
            };
            let Some(instance) = instance else { return original.call_async::<LuaValue>(value).await; };
            if instance.get_class_name() != "ModuleScript" {
                return Err(LuaError::runtime("require expects a ModuleScript Instance"));
            }
            require_module(lua, context, instance, caller).await
        }
    })
}

struct DependencyGuard {
    context: Weak<RefCell<ScriptContext>>,
    caller: String,
    target: String,
}

impl Drop for DependencyGuard {
    fn drop(&mut self) {
        if let Some(context) = self.context.upgrade() {
            if let Some(edges) = context.borrow_mut().dependencies.get_mut(&self.caller) { edges.remove(&self.target); }
        }
    }
}

fn reaches(edges: &HashMap<String, HashSet<String>>, start: &str, target: &str) -> bool {
    let mut queue = vec![start];
    let mut seen = HashSet::new();
    while let Some(node) = queue.pop() {
        if node == target { return true; }
        if seen.insert(node) {
            if let Some(next) = edges.get(node) { queue.extend(next.iter().map(String::as_str)); }
        }
    }
    false
}

async fn require_module(lua: Lua, context: Rc<RefCell<ScriptContext>>, module: Instance, caller: Option<String>) -> LuaResult<LuaValue> {
    let key = instance_key(&module);
    let existing = context.borrow().modules.get(&key).cloned();
    if let Some(job) = &existing {
        if let Some(result) = job.result.borrow().as_ref() { return result.clone(); }
    }
    let _guard = if let Some(caller) = caller {
        let mut state = context.borrow_mut();
        if reaches(&state.dependencies, &key, &caller) {
            return Err(LuaError::runtime(format!("Yune cyclic ModuleScript dependency: {}", module.get_full_name())));
        }
        state.dependencies.entry(caller.clone()).or_default().insert(key.clone());
        Some(DependencyGuard { context: Rc::downgrade(&context), caller, target: key.clone() })
    } else { None };
    let job = if let Some(job) = existing { job } else {
        let job = Rc::new(ModuleJob::default());
        context.borrow_mut().modules.insert(key.clone(), job.clone());
        if let Err(error) = start_module(&lua, context.clone(), module, &key, job.clone()) { job.finish(Err(error)); }
        job
    };
    job.wait().await
}

fn start_module(lua: &Lua, context: Rc<RefCell<ScriptContext>>, module: Instance, key: &str, job: Rc<ModuleJob>) -> LuaResult<()> {
    let source = script_source(&module).ok_or_else(|| LuaError::runtime("ModuleScript has no Source"))?;
    let environment = script_environment(lua, module)?;
    environment.set("require", make_require(lua, context, Some(key.to_string()))?)?;
    let name = module.get_full_name();
    let function = lua.load(source).set_name(format!("@{name}"))
        .set_environment(environment).into_function()?;
    let boundary = lua.load("local fn = ...\nreturn pcall(fn)")
        .set_name("=YuneModuleBoundary").into_function()?;
    let thread = lua.create_thread(boundary)?;
    let id = lua.push_thread_front(thread, (function,))?;
    lua.track_thread(id);
    let worker_lua = lua.clone();
    lua.spawn_local(async move {
        worker_lua.wait_for_thread(id).await;
        let result = worker_lua.get_thread_result(id)
            .unwrap_or_else(|| Err(LuaError::runtime("ModuleScript completed without a tracked result")))
            .and_then(|values| {
                let mut values = values.into_iter();
                if !matches!(values.next(), Some(LuaValue::Boolean(true))) {
                    let message = match values.next() {
                        Some(LuaValue::String(message)) => message.to_string_lossy(),
                        Some(value) => format!("{value:?}"),
                        None => "unknown module error".to_string(),
                    };
                    return Err(LuaError::runtime(message));
                }
                let values = values.collect::<Vec<_>>();
                if values.len() != 1 {
                    Err(LuaError::runtime(format!("ModuleScript '{name}' must return exactly one value, returned {}", values.len())))
                } else { Ok(values.into_iter().next().unwrap_or(LuaValue::Nil)) }
            });
        job.finish(result);
    });
    Ok(())
}

fn script_environment(lua: &Lua, script: Instance) -> LuaResult<LuaTable> {
    let environment = lua.create_table()?;
    environment.set("script", instance_to_lua(lua, script)?)?;
    let metatable = lua.create_table()?;
    metatable.set("__index", lua.globals())?;
    environment.set_metatable(Some(metatable))?;
    Ok(environment)
}
fn script_source(script: &Instance) -> Option<String> {
    match script.get_property("Source") { Some(Variant::String(source)) => Some(source), _ => None }
}
fn script_enabled(script: &Instance) -> bool {
    !matches!(script.get_property("Enabled"), Some(Variant::Bool(false)))
        && !matches!(script.get_property("Disabled"), Some(Variant::Bool(true)))
}
fn run_context(script: &Instance) -> u32 {
    match script.get_property("RunContext") {
        Some(Variant::Enum(value)) => value.to_u32(),
        Some(Variant::EnumItem(value)) => value.value,
        _ => 0,
    }
}
fn instance_key(instance: &Instance) -> String { format!("{}:{}", instance.dom_id, instance.dom_ref) }
