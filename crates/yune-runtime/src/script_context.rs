use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
};

use lune_roblox::instance::{Instance, instance_to_lua};
use mlua::prelude::*;
use mlua_luau_scheduler::LuaSchedulerExt;
use rbx_dom_weak::types::Variant;

#[derive(Default)]
pub struct ScriptContext {
    started_scripts: HashSet<String>,
    module_cache: HashMap<String, LuaValue>,
    loading_modules: HashSet<String>,
}

impl ScriptContext {
    pub fn install(lua: &Lua, context: Rc<RefCell<Self>>) -> LuaResult<()> {
        let original_require = lua.globals().get::<LuaFunction>("require")?;
        let require_context = context;

        let require = lua.create_async_function(move |lua, value: LuaValue| {
            let original_require = original_require.clone();
            let require_context = require_context.clone();
            async move {
                let LuaValue::UserData(userdata) = &value else {
                    return original_require.call_async::<LuaValue>(value).await;
                };

                let Ok(instance) = userdata.borrow::<Instance>() else {
                    return original_require.call_async::<LuaValue>(value).await;
                };

                if instance.get_class_name() != "ModuleScript" {
                    return Err(LuaError::runtime(format!(
                        "attempt to require non-ModuleScript Instance '{}'",
                        instance.get_full_name()
                    )));
                }

                require_module(lua, require_context, *instance).await
            }
        })?;

        lua.globals().set("require", require)
    }

    pub fn discover(&mut self, lua: &Lua, game: Instance) -> LuaResult<usize> {
        let mut scheduled = 0;

        for script in game.get_descendants_preorder() {
            if !matches!(script.get_class_name(), "Script" | "LocalScript") {
                continue;
            }

            let key = instance_key(&script);
            if self.started_scripts.contains(&key) || !script_enabled(&script) {
                continue;
            }

            let Some(source) = script_source(&script) else {
                continue;
            };
            if source.is_empty() {
                continue;
            }

            let environment = script_environment(lua, script)?;
            let chunk_name = format!("@{}", script.get_full_name());
            let chunk = lua
                .load(source)
                .set_name(chunk_name)
                .set_environment(environment);

            lua.push_thread_back(chunk, ())?;
            self.started_scripts.insert(key);
            scheduled += 1;
        }

        Ok(scheduled)
    }

    pub fn reset(&mut self) {
        self.started_scripts.clear();
        self.module_cache.clear();
        self.loading_modules.clear();
    }
}

async fn require_module(
    lua: Lua,
    context: Rc<RefCell<ScriptContext>>,
    module: Instance,
) -> LuaResult<LuaValue> {
    let key = instance_key(&module);

    if let Some(value) = context.borrow().module_cache.get(&key).cloned() {
        return Ok(value);
    }

    {
        let mut context = context.borrow_mut();
        if !context.loading_modules.insert(key.clone()) {
            return Err(LuaError::runtime(format!(
                "cyclic or concurrent require detected for '{}'",
                module.get_full_name()
            )));
        }
    }

    let result = run_module(&lua, module).await;

    let mut context = context.borrow_mut();
    context.loading_modules.remove(&key);

    match result {
        Ok(value) => {
            context.module_cache.insert(key, value.clone());
            Ok(value)
        }
        Err(error) => Err(error),
    }
}

async fn run_module(lua: &Lua, module: Instance) -> LuaResult<LuaValue> {
    let source = script_source(&module).ok_or_else(|| {
        LuaError::runtime(format!(
            "ModuleScript '{}' has no Source",
            module.get_full_name()
        ))
    })?;

    let environment = script_environment(lua, module)?;
    let chunk_name = format!("@{}", module.get_full_name());
    let function = lua
        .load(source)
        .set_name(chunk_name)
        .set_environment(environment)
        .into_function()?;
    let thread = lua.create_thread(function)?;
    let id = lua.push_thread_front(thread, ())?;

    lua.wait_for_thread(id).await;

    let values = lua
        .get_thread_result(id)
        .ok_or_else(|| LuaError::runtime("ModuleScript thread completed without a result"))??;

    if values.len() != 1 {
        return Err(LuaError::runtime(format!(
            "ModuleScript '{}' must return exactly one value, returned {}",
            module.get_full_name(),
            values.len()
        )));
    }

    Ok(values.into_iter().next().unwrap_or(LuaValue::Nil))
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
    match script.get_property("Source") {
        Some(Variant::String(source)) => Some(source),
        _ => None,
    }
}

fn script_enabled(script: &Instance) -> bool {
    if matches!(script.get_property("Enabled"), Some(Variant::Bool(false))) {
        return false;
    }
    !matches!(script.get_property("Disabled"), Some(Variant::Bool(true)))
}

fn instance_key(instance: &Instance) -> String {
    format!("{}:{}", instance.dom_id, instance.dom_ref)
}
