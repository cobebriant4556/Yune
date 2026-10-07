mod editable_image;
mod editable_mesh;
mod framebuffer;
mod gui;
mod props;
mod raster;

use std::{collections::HashMap, path::Path, sync::{Arc, Mutex}};

use glam::{Vec2, Vec3};
use image::ImageReader;
use lune_roblox::{
    datatypes::types::{Color3, Vector2, Vector3},
    instance::{Instance, instance_to_lua, registry::InstanceRegistry},
};
use mlua::prelude::*;
use rbx_dom_weak::types::{Color3 as DomColor3, Variant};

pub use editable_image::EditableImage;
pub use editable_mesh::EditableMesh;
use framebuffer::Framebuffer;
use gui::render_gui;
use props::{color_prop, instance_key};
use raster::{Lighting, render_world};

#[derive(Clone)]
pub enum ImageBinding {
    Pixels { width: u32, height: u32, pixels: Arc<Vec<u8>> },
    Editable(EditableImage),
}

#[derive(Clone, Default)]
pub struct RenderState {
    meshes: Arc<Mutex<HashMap<String, EditableMesh>>>,
    images: Arc<Mutex<HashMap<String, ImageBinding>>>,
}

impl RenderState {
    fn bind_mesh(&self, key: String, mesh: EditableMesh) {
        self.meshes.lock().expect("mesh binding lock poisoned").insert(key, mesh);
    }

    pub(crate) fn mesh_binding(&self, key: &str) -> Option<EditableMesh> {
        self.meshes.lock().expect("mesh binding lock poisoned").get(key).cloned()
    }

    fn bind_image(&self, key: String, image: ImageBinding) {
        self.images.lock().expect("image binding lock poisoned").insert(key, image);
    }

    pub(crate) fn image_binding(&self, key: &str) -> Option<ImageBinding> {
        self.images.lock().expect("image binding lock poisoned").get(key).cloned()
    }
}

pub fn install(lua: &Lua, state: RenderState) -> LuaResult<LuaValue> {
    inject_roblox_globals(lua)?;
    let (game, workspace) = create_data_model(lua)?;
    install_into(lua, state, game, workspace)
}

pub fn install_into(
    lua: &Lua,
    state: RenderState,
    game: Instance,
    workspace: Instance,
) -> LuaResult<LuaValue> {
    install_asset_service(lua)?;

    let module = lua.create_table()?;

    let bind_mesh_state = state.clone();
    module.set(
        "bindEditableMesh",
        lua.create_function(move |_, (instance, mesh): (LuaUserDataRef<Instance>, LuaUserDataRef<EditableMesh>)| {
            bind_mesh_state.bind_mesh(instance_key(&instance), mesh.clone());
            Ok(())
        })?,
    )?;

    let bind_editable_image_state = state.clone();
    module.set(
        "bindEditableImage",
        lua.create_function(move |_, (instance, image): (LuaUserDataRef<Instance>, LuaUserDataRef<EditableImage>)| {
            bind_editable_image_state.bind_image(instance_key(&instance), ImageBinding::Editable(image.clone()));
            Ok(())
        })?,
    )?;

    let bind_file_state = state.clone();
    module.set(
        "bindImage",
        lua.create_function(move |_, (instance, path): (LuaUserDataRef<Instance>, String)| {
            let image = ImageReader::open(Path::new(&path))
                .map_err(LuaError::external)?
                .decode()
                .map_err(LuaError::external)?
                .to_rgba8();
            let width = image.width();
            let height = image.height();
            bind_file_state.bind_image(
                instance_key(&instance),
                ImageBinding::Pixels { width, height, pixels: Arc::new(image.into_raw()) },
            );
            Ok(())
        })?,
    )?;

    let capture_state = state.clone();
    let default_workspace = workspace;
    let default_game = game;
    module.set(
        "capture",
        lua.create_function(move |lua, options: LuaTable| {
            capture(lua, &capture_state, default_game, default_workspace, options)
        })?,
    )?;

    module.set("version", "0.1.0")?;
    lua.globals().set("YuneRender", module.clone())?;
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

fn create_data_model(lua: &Lua) -> LuaResult<(Instance, Instance)> {
    let game = Instance::new_orphaned("DataModel");
    game.set_name("Game");

    let workspace = Instance::new_in_dom(game.dom_id, "Workspace");
    workspace.set_name("Workspace");
    workspace.set_parent(Some(game));

    let lighting = Instance::new_in_dom(game.dom_id, "Lighting");
    lighting.set_name("Lighting");
    lighting.set_parent(Some(game));

    let camera = Instance::new_in_dom(game.dom_id, "Camera");
    camera.set_name("Camera");
    camera.set_parent(Some(workspace));
    workspace.set_property("CurrentCamera", Variant::Ref(camera.dom_ref));

    lua.globals().set("game", instance_to_lua(lua, game)?)?;
    lua.globals().set("workspace", instance_to_lua(lua, workspace)?)?;
    Ok((game, workspace))
}

fn install_asset_service(lua: &Lua) -> LuaResult<()> {
    let create_mesh = lua.create_function(|_, (_service, _options): (LuaUserDataRef<Instance>, Option<LuaTable>)| {
        Ok(EditableMesh::default())
    })?;
    InstanceRegistry::insert_method(lua, "AssetService", "CreateEditableMesh", create_mesh)
        .map_err(LuaError::external)?;

    let create_image = lua.create_function(|_, (_service, options): (LuaUserDataRef<Instance>, Option<LuaTable>)| {
        let mut width = 512u32;
        let mut height = 512u32;
        if let Some(options) = options
            && let Ok(LuaValue::UserData(size)) = options.get::<LuaValue>("Size")
            && let Ok(size) = size.borrow::<Vector2>()
        {
            width = size.0.x.round().clamp(1.0, 4096.0) as u32;
            height = size.0.y.round().clamp(1.0, 4096.0) as u32;
        }
        Ok(EditableImage::new(width, height))
    })?;
    InstanceRegistry::insert_method(lua, "AssetService", "CreateEditableImage", create_image)
        .map_err(LuaError::external)?;
    Ok(())
}

fn capture(
    lua: &Lua,
    state: &RenderState,
    game: Instance,
    default_workspace: Instance,
    options: LuaTable,
) -> LuaResult<LuaTable> {
    let world = table_instance(&options, "world")?.unwrap_or(default_workspace);
    let camera = table_instance(&options, "camera")?
        .or_else(|| props::ref_prop(&default_workspace, "CurrentCamera"))
        .or_else(|| world.get_descendants_preorder().into_iter().find(|inst| inst.get_class_name() == "Camera"))
        .ok_or_else(|| LuaError::runtime("Yune capture requires a Camera"))?;

    let width = options.get::<Option<u32>>("width")?.unwrap_or(1280).clamp(1, 8192);
    let height = options.get::<Option<u32>>("height")?.unwrap_or(720).clamp(1, 8192);
    let path = options.get::<Option<String>>("path")?;

    let clear = match options.get::<LuaValue>("clearColor")? {
        LuaValue::UserData(value) => match value.borrow::<Color3>() {
            Ok(color) => {
                let color: DomColor3 = (*color).into();
                Vec3::new(color.r, color.g, color.b)
            }
            Err(_) => Vec3::new(0.55, 0.72, 0.9),
        },
        _ => Vec3::new(0.55, 0.72, 0.9),
    };

    let mut framebuffer = Framebuffer::new(width, height);
    framebuffer.clear([
        (clear.x.clamp(0.0, 1.0) * 255.0).round() as u8,
        (clear.y.clamp(0.0, 1.0) * 255.0).round() as u8,
        (clear.z.clamp(0.0, 1.0) * 255.0).round() as u8,
        255,
    ]);

    let lighting_instance = game.get_children().into_iter().find(|inst| inst.get_class_name() == "Lighting");
    let mut lighting = Lighting::default();
    if let Some(lighting_inst) = lighting_instance {
        lighting.ambient = color_prop(&lighting_inst, "Ambient", lighting.ambient);
        lighting.outdoor_ambient =
            color_prop(&lighting_inst, "OutdoorAmbient", lighting.outdoor_ambient);
        lighting.brightness =
            props::f32_prop(&lighting_inst, "Brightness", lighting.brightness).max(0.0);
        lighting.exposure =
            props::f32_prop(&lighting_inst, "ExposureCompensation", lighting.exposure);
        lighting.fog_color = color_prop(&lighting_inst, "FogColor", lighting.fog_color);
        lighting.fog_start =
            props::f32_prop(&lighting_inst, "FogStart", lighting.fog_start).max(0.0);
        lighting.fog_end =
            props::f32_prop(&lighting_inst, "FogEnd", lighting.fog_end).max(lighting.fog_start);
    }
    if let LuaValue::UserData(direction) = options.get::<LuaValue>("lightDirection")?
        && let Ok(direction) = direction.borrow::<Vector3>()
    {
        lighting.light_direction = direction.0.normalize_or_zero();
    }

    let world_stats = render_world(&mut framebuffer, world, camera, state, lighting);

    let mut gui_objects = 0u64;
    let mut viewport_parts = 0u64;
    let mut viewport_triangles = 0u64;
    for gui_root in gui_roots(&options)? {
        let stats = render_gui(&mut framebuffer, gui_root, state);
        gui_objects += stats.objects;
        viewport_parts += stats.viewport_parts;
        viewport_triangles += stats.viewport_triangles;
    }

    if let Some(path) = &path {
        framebuffer.save_png(path).map_err(LuaError::external)?;
    }

    let result = lua.create_table()?;
    result.set("width", width)?;
    result.set("height", height)?;
    result.set("parts", world_stats.parts + viewport_parts)?;
    result.set("triangles", world_stats.triangles + viewport_triangles)?;
    result.set("guiObjects", gui_objects)?;
    if let Some(path) = path {
        result.set("path", path)?;
    }
    Ok(result)
}

fn table_instance(table: &LuaTable, key: &str) -> LuaResult<Option<Instance>> {
    match table.get::<LuaValue>(key)? {
        LuaValue::Nil => Ok(None),
        LuaValue::UserData(value) => Ok(Some(*value.borrow::<Instance>()?)),
        value => Err(LuaError::runtime(format!("'{key}' must be an Instance, got {}", value.type_name()))),
    }
}

fn gui_roots(options: &LuaTable) -> LuaResult<Vec<Instance>> {
    match options.get::<LuaValue>("gui")? {
        LuaValue::Nil => Ok(Vec::new()),
        LuaValue::UserData(value) => Ok(vec![*value.borrow::<Instance>()?]),
        LuaValue::Table(values) => values
            .sequence_values::<LuaAnyUserData>()
            .map(|value| value.and_then(|value| Ok(*value.borrow::<Instance>()?)))
            .collect(),
        value => Err(LuaError::runtime(format!("'gui' must be an Instance or array of Instances, got {}", value.type_name()))),
    }
}
