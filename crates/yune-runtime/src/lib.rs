mod animation;
mod joints;
mod script_context;
mod signal;

use std::{
    cell::{Cell, RefCell},
    fs,
    rc::Rc,
};

use lune_roblox::{
    datatypes::types::Vector3,
    document::{Document, DocumentKind},
    instance::{Instance, instance_to_lua, registry::InstanceRegistry},
};
use mlua::prelude::*;
use rbx_dom_weak::types::{Color3 as DomColor3, Variant};

use animation::AnimationSystem;
use joints::JointSystem;
use script_context::ScriptContext;
use signal::Signal;
use yune_physics::PhysicsWorld;

#[derive(Clone, Default)]
struct FrameSignals {
    stepped: Signal,
    pre_animation: Signal,
    pre_simulation: Signal,
    post_simulation: Signal,
    heartbeat: Signal,
    pre_render: Signal,
    render_stepped: Signal,
}

#[derive(Clone)]
struct RuntimeState {
    game: Instance,
    workspace: Instance,
    run_service: Instance,
    signals: FrameSignals,
    time: Rc<Cell<f64>>,
    frame: Rc<Cell<u64>>,
    physics: Rc<RefCell<PhysicsWorld>>,
    joints: Rc<RefCell<JointSystem>>,
    animations: Rc<RefCell<AnimationSystem>>,
    scripts: Rc<RefCell<ScriptContext>>,
}

pub fn install(lua: &Lua) -> LuaResult<LuaValue> {
    inject_roblox_globals(lua)?;
    lua.globals()
        .set("task", lune_std_task::module(lua.clone())?)?;

    let render_state = yune_render::RenderState::default();
    let scripts = Rc::new(RefCell::new(ScriptContext::default()));
    ScriptContext::install(lua, scripts.clone())?;
    let state = bootstrap(lua, scripts)?;
    install_run_service(lua, &state)?;
    install_physics_api(lua, state.physics.clone())?;
    AnimationSystem::install(lua, state.animations.clone())?;

    let render_module =
        yune_render::install_into(lua, render_state, state.game, state.workspace)?;
    lua.register_module("@yune/render", render_module.clone())?;

    let module = lua.create_table()?;

    let step_state = state.clone();
    module.set(
        "step",
        lua.create_function(move |lua, dt: Option<f64>| {
            step_runtime(lua, &step_state, dt.unwrap_or(1.0 / 60.0))
        })?,
    )?;

    let run_state = state.clone();
    module.set(
        "runFrames",
        lua.create_function(move |lua, (count, dt): (u64, Option<f64>)| {
            let dt = dt.unwrap_or(1.0 / 60.0);
            for _ in 0..count {
                step_runtime(lua, &run_state, dt)?;
            }
            Ok(())
        })?,
    )?;

    let load_place_state = state.clone();
    module.set(
        "loadPlace",
        lua.create_function(move |lua, path: String| {
            load_place_into(load_place_state.game, &path)?;
            {
                let mut scripts = load_place_state.scripts.borrow_mut();
                scripts.reset();
                scripts.discover(lua, load_place_state.game)?;
            }
            Ok(())
        })?,
    )?;

    module.set(
        "executeSource",
        lua.create_function(|lua, (source, chunk_name): (String, Option<String>)| {
            let name = chunk_name.unwrap_or_else(|| "YuneScript".to_string());
            lua.load(source).set_name(name).exec()
        })?,
    )?;

    let time_state = state.clone();
    module.set(
        "getTime",
        lua.create_function(move |_, ()| Ok(time_state.time.get()))?,
    )?;

    let frame_state = state.clone();
    module.set(
        "getFrame",
        lua.create_function(move |_, ()| Ok(frame_state.frame.get()))?,
    )?;

    module.set("game", instance_to_lua(lua, state.game)?)?;
    module.set("workspace", instance_to_lua(lua, state.workspace)?)?;
    module.set("render", render_module)?;
    module.set("version", "0.3.0")?;

    lua.globals().set("Yune", module.clone())?;
    Ok(LuaValue::Table(module))
}

fn inject_roblox_globals(lua: &Lua) -> LuaResult<()> {
    let roblox = lune_roblox::module(lua.clone())?;
    for pair in roblox.pairs::<LuaValue, LuaValue>() {
        let (key, value) = pair?;
        if let LuaValue::String(key) = key {
            lua.globals().set(key.to_str()?, value)?;
        }
    }
    Ok(())
}

fn bootstrap(lua: &Lua, scripts: Rc<RefCell<ScriptContext>>) -> LuaResult<RuntimeState> {
    let game = Instance::new_orphaned("DataModel");
    game.set_name("Game");
    game.set_property("PlaceId", Variant::Int64(0));
    game.set_property("GameId", Variant::Int64(0));
    game.set_property("JobId", Variant::String("Yune".to_string()));

    let workspace = create_service(game, "Workspace");
    game.set_property("Workspace", Variant::Ref(workspace.dom_ref));

    let lighting = create_service(game, "Lighting");
    lighting.set_property(
        "Ambient",
        Variant::Color3(DomColor3 {
            r: 0.45,
            g: 0.45,
            b: 0.45,
        }),
    );
    lighting.set_property(
        "OutdoorAmbient",
        Variant::Color3(DomColor3 {
            r: 0.5,
            g: 0.5,
            b: 0.5,
        }),
    );
    lighting.set_property("Brightness", Variant::Float32(1.0));
    lighting.set_property("ClockTime", Variant::Float32(14.0));
    lighting.set_property("GlobalShadows", Variant::Bool(true));
    lighting.set_property(
        "FogColor",
        Variant::Color3(DomColor3 {
            r: 0.75,
            g: 0.82,
            b: 0.9,
        }),
    );
    lighting.set_property("FogStart", Variant::Float32(0.0));
    lighting.set_property("FogEnd", Variant::Float32(100000.0));
    lighting.set_property("ExposureCompensation", Variant::Float32(0.0));

    let run_service = create_service(game, "RunService");
    create_service(game, "ReplicatedFirst");
    create_service(game, "ReplicatedStorage");
    create_service(game, "ServerStorage");
    create_service(game, "StarterGui");
    create_service(game, "CoreGui");
    create_service(game, "SoundService");
    create_service(game, "AssetService");
    create_service(game, "CollectionService");
    create_service(game, "PhysicsService");

    let players = create_service(game, "Players");
    let local_player = Instance::new_in_dom(game.dom_id, "Player");
    local_player.set_name("LocalPlayer");
    local_player.set_parent(Some(players));

    let player_gui = Instance::new_in_dom(game.dom_id, "PlayerGui");
    player_gui.set_name("PlayerGui");
    player_gui.set_parent(Some(local_player));
    players.set_property("LocalPlayer", Variant::Ref(local_player.dom_ref));

    let camera = Instance::new_in_dom(game.dom_id, "Camera");
    camera.set_name("Camera");
    camera.set_parent(Some(workspace));
    workspace.set_property("CurrentCamera", Variant::Ref(camera.dom_ref));
    workspace.set_property("DistributedGameTime", Variant::Float64(0.0));
    workspace.set_property("Gravity", Variant::Float32(196.2));

    lua.globals().set("game", instance_to_lua(lua, game)?)?;
    lua.globals()
        .set("workspace", instance_to_lua(lua, workspace)?)?;

    Ok(RuntimeState {
        game,
        workspace,
        run_service,
        signals: FrameSignals::default(),
        time: Rc::new(Cell::new(0.0)),
        frame: Rc::new(Cell::new(0)),
        physics: Rc::new(RefCell::new(PhysicsWorld::default())),
        joints: Rc::new(RefCell::new(JointSystem::default())),
        animations: Rc::new(RefCell::new(AnimationSystem::default())),
        scripts,
    })
}

fn create_service(game: Instance, class_name: &str) -> Instance {
    if let Some(existing) = game
        .get_children()
        .into_iter()
        .find(|child| child.get_class_name() == class_name)
    {
        return existing;
    }

    let service = Instance::new_in_dom(game.dom_id, class_name);
    service.set_name(class_name);
    service.set_parent(Some(game));
    service
}

fn install_run_service(lua: &Lua, state: &RuntimeState) -> LuaResult<()> {
    install_signal_getter(lua, "Stepped", state.signals.stepped.clone())?;
    install_signal_getter(
        lua,
        "PreAnimation",
        state.signals.pre_animation.clone(),
    )?;
    install_signal_getter(
        lua,
        "PreSimulation",
        state.signals.pre_simulation.clone(),
    )?;
    install_signal_getter(
        lua,
        "PostSimulation",
        state.signals.post_simulation.clone(),
    )?;
    install_signal_getter(lua, "Heartbeat", state.signals.heartbeat.clone())?;
    install_signal_getter(lua, "PreRender", state.signals.pre_render.clone())?;
    install_signal_getter(
        lua,
        "RenderStepped",
        state.signals.render_stepped.clone(),
    )?;

    let is_running =
        lua.create_function(|_, (_service, ()): (LuaUserDataRef<Instance>, ())| Ok(true))?;
    InstanceRegistry::insert_method(lua, "RunService", "IsRunning", is_running)
        .map_err(LuaError::external)?;

    let is_studio =
        lua.create_function(|_, (_service, ()): (LuaUserDataRef<Instance>, ())| Ok(false))?;
    InstanceRegistry::insert_method(lua, "RunService", "IsStudio", is_studio)
        .map_err(LuaError::external)?;

    let is_client =
        lua.create_function(|_, (_service, ()): (LuaUserDataRef<Instance>, ())| Ok(true))?;
    InstanceRegistry::insert_method(lua, "RunService", "IsClient", is_client)
        .map_err(LuaError::external)?;

    let is_server =
        lua.create_function(|_, (_service, ()): (LuaUserDataRef<Instance>, ())| Ok(false))?;
    InstanceRegistry::insert_method(lua, "RunService", "IsServer", is_server)
        .map_err(LuaError::external)?;

    Ok(())
}

fn install_signal_getter(lua: &Lua, name: &str, signal: Signal) -> LuaResult<()> {
    let getter = lua.create_function(move |_, _service: LuaAnyUserData| Ok(signal.clone()))?;
    InstanceRegistry::insert_property_getter(lua, "RunService", name, getter)
        .map_err(LuaError::external)
}


#[derive(Clone, Copy)]
struct LuaRaycastResult {
    instance: Instance,
    position: glam::Vec3,
    normal: glam::Vec3,
    distance: f32,
}

impl LuaUserData for LuaRaycastResult {
    fn add_fields<F: LuaUserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("Instance", |lua, this| {
            instance_to_lua(lua, this.instance)
        });
        fields.add_field_method_get("Position", |_, this| Ok(Vector3(this.position)));
        fields.add_field_method_get("Normal", |_, this| Ok(Vector3(this.normal)));
        fields.add_field_method_get("Distance", |_, this| Ok(this.distance));
    }
}

fn install_physics_api(lua: &Lua, physics: Rc<RefCell<PhysicsWorld>>) -> LuaResult<()> {
    let impulse_physics = physics.clone();
    let apply_impulse = lua.create_function(
        move |_, (part, impulse): (LuaUserDataRef<Instance>, LuaUserDataRef<Vector3>)| {
            impulse_physics
                .borrow_mut()
                .apply_impulse(*part, impulse.0);
            Ok(())
        },
    )?;
    InstanceRegistry::insert_method(lua, "BasePart", "ApplyImpulse", apply_impulse)
        .map_err(LuaError::external)?;

    let angular_physics = physics.clone();
    let apply_angular = lua.create_function(
        move |_, (part, impulse): (LuaUserDataRef<Instance>, LuaUserDataRef<Vector3>)| {
            angular_physics
                .borrow_mut()
                .apply_angular_impulse(*part, impulse.0);
            Ok(())
        },
    )?;
    InstanceRegistry::insert_method(
        lua,
        "BasePart",
        "ApplyAngularImpulse",
        apply_angular,
    )
    .map_err(LuaError::external)?;

    let mass_physics = physics.clone();
    let get_mass = lua.create_function(move |_, part: LuaUserDataRef<Instance>| {
        Ok(mass_physics.borrow_mut().mass(*part))
    })?;
    InstanceRegistry::insert_method(lua, "BasePart", "GetMass", get_mass)
        .map_err(LuaError::external)?;

    let ray_physics = physics;
    let raycast = lua.create_function(
        move |_, (_workspace, origin, direction, _params): (
            LuaUserDataRef<Instance>,
            LuaUserDataRef<Vector3>,
            LuaUserDataRef<Vector3>,
            Option<LuaValue>,
        )| {
            let max_distance = direction.0.length();
            Ok(ray_physics
                .borrow()
                .raycast(origin.0, direction.0, max_distance)
                .map(|hit| LuaRaycastResult {
                    instance: hit.instance,
                    position: hit.position,
                    normal: hit.normal,
                    distance: hit.distance,
                }))
        },
    )?;
    InstanceRegistry::insert_method(lua, "Workspace", "Raycast", raycast)
        .map_err(LuaError::external)?;

    Ok(())
}

fn step_runtime(lua: &Lua, state: &RuntimeState, dt: f64) -> LuaResult<()> {
    let dt = dt.max(0.0);
    state.scripts.borrow_mut().discover(lua, state.game)?;
    let old_time = state.time.get();
    let new_time = old_time + dt;
    let new_frame = state.frame.get().saturating_add(1);

    state.time.set(new_time);
    state.frame.set(new_frame);
    state
        .workspace
        .set_property("DistributedGameTime", Variant::Float64(new_time));
    state
        .run_service
        .set_property("FrameNumber", Variant::Int64(new_frame as i64));

    state.signals.stepped.fire2(old_time, dt)?;
    state.signals.pre_animation.fire1(dt)?;
    state.animations.borrow_mut().step(state.workspace, dt);
    state.signals.pre_simulation.fire1(dt)?;

    state.joints.borrow_mut().solve(state.workspace);
    state.physics.borrow_mut().step(state.workspace, dt);
    state.joints.borrow_mut().solve(state.workspace);

    state.signals.post_simulation.fire1(dt)?;
    state.signals.heartbeat.fire1(dt)?;
    state.signals.pre_render.fire1(dt)?;
    state.signals.render_stepped.fire1(dt)?;

    Ok(())
}


fn load_place_into(game: Instance, path: &str) -> LuaResult<()> {
    let bytes = fs::read(path).map_err(LuaError::external)?;
    let source_game = Document::from_bytes(bytes, DocumentKind::Place)
        .map_err(LuaError::external)?
        .into_data_model_instance()
        .map_err(LuaError::external)?;

    let source_services = source_game.get_children();
    for source_service in source_services {
        let class_name = source_service.get_class_name().to_string();
        if let Some(target_service) = game
            .get_children()
            .into_iter()
            .find(|child| child.get_class_name() == class_name)
        {
            let properties = source_service.with_dom(|dom| {
                dom.get_by_ref(source_service.dom_ref)
                    .map(|instance| instance.properties.clone())
                    .unwrap_or_default()
            });

            for child in source_service.get_children() {
                child.set_parent(Some(target_service));
            }

            target_service.with_dom_mut(|dom| {
                if let Some(instance) = dom.get_by_ref_mut(target_service.dom_ref) {
                    for (name, value) in properties {
                        instance.properties.insert(name, value);
                    }
                }
            });
        } else {
            source_service.set_parent(Some(game));
        }
    }

    Ok(())
}
