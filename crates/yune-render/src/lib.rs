mod asset_io;
mod editable_image;
mod editable_mesh;
mod framebuffer;
mod gui;
mod mesh_asset;
mod props;
mod raster;
mod world_gui;

use std::{collections::{BTreeMap, HashMap}, io::Cursor, path::Path, sync::{Arc, Mutex}};
use asset_io::{AssetPaths, asset_key, read_limited};
use fontdue::{Font, FontSettings};
use glam::Vec3;
use image::ImageReader;
use lune_roblox::{datatypes::types::{Color3, Vector2, Vector3}, instance::{Instance, instance_to_lua, registry::InstanceRegistry}};
use mlua::prelude::*;
use rbx_dom_weak::types::{Color3 as DomColor3, Variant};
pub use editable_image::EditableImage;
pub use editable_mesh::EditableMesh;
use framebuffer::Framebuffer;
use gui::render_gui;
use mesh_asset::StaticMesh;
use props::{color_prop, instance_key};
use raster::{Lighting, render_world};
use world_gui::render_world_guis;

#[derive(Clone)]
pub enum ImageBinding {
    Pixels { width: u32, height: u32, pixels: Arc<Vec<u8>> },
    Editable(EditableImage),
}

#[derive(Clone, Default)]
pub struct RenderState {
    meshes: Arc<Mutex<HashMap<String, EditableMesh>>>,
    mesh_assets: Arc<Mutex<HashMap<String, Arc<StaticMesh>>>>,
    images: Arc<Mutex<HashMap<String, ImageBinding>>>,
    image_assets: Arc<Mutex<HashMap<String, ImageBinding>>>,
    fonts: Arc<Mutex<HashMap<String, Arc<Font>>>>,
    paths: Arc<Mutex<AssetPaths>>,
    errors: Arc<Mutex<BTreeMap<String, String>>>,
}

impl RenderState {
    fn bind_mesh(&self, key: String, mesh: EditableMesh) { self.meshes.lock().unwrap().insert(key, mesh); }
    pub(crate) fn mesh_binding(&self, key: &str) -> Option<EditableMesh> { self.meshes.lock().unwrap().get(key).cloned() }
    fn register_mesh_asset(&self, id: String, mesh: StaticMesh) { self.mesh_assets.lock().unwrap().insert(asset_key(&id), Arc::new(mesh)); }
    pub(crate) fn mesh_asset(&self, id: &str) -> Option<Arc<StaticMesh>> {
        if id.trim().is_empty() { return None; }
        let key = asset_key(id);
        if let Some(mesh) = self.mesh_assets.lock().unwrap().get(&key).cloned() { return Some(mesh); }
        if self.errors.lock().unwrap().contains_key(&format!("mesh:{key}")) { return None; }
        let path = self.paths.lock().unwrap().resolve(id);
        match path.and_then(|path| read_mesh(&path)) {
            Ok(mesh) => {
                let mesh = Arc::new(mesh);
                self.mesh_assets.lock().unwrap().insert(key, mesh.clone());
                Some(mesh)
            }
            Err(error) => { self.errors.lock().unwrap().insert(format!("mesh:{key}"), error); None }
        }
    }
    fn bind_image(&self, key: String, image: ImageBinding) { self.images.lock().unwrap().insert(key, image); }
    pub(crate) fn image_binding(&self, key: &str) -> Option<ImageBinding> { self.images.lock().unwrap().get(key).cloned() }
    fn register_image_asset(&self, id: String, image: ImageBinding) { self.image_assets.lock().unwrap().insert(asset_key(&id), image); }
    pub(crate) fn image_asset(&self, id: &str) -> Option<ImageBinding> {
        if id.trim().is_empty() { return None; }
        let key = asset_key(id);
        if let Some(image) = self.image_assets.lock().unwrap().get(&key).cloned() { return Some(image); }
        if self.errors.lock().unwrap().contains_key(&format!("image:{key}")) { return None; }
        let path = self.paths.lock().unwrap().resolve(id);
        match path.and_then(|path| read_image(&path)) {
            Ok(image) => { self.image_assets.lock().unwrap().insert(key, image.clone()); Some(image) }
            Err(error) => { self.errors.lock().unwrap().insert(format!("image:{key}"), error); None }
        }
    }
    fn register_font(&self, family: String, font: Font) {
        let font = Arc::new(font);
        let mut fonts = self.fonts.lock().unwrap();
        fonts.entry("@default".to_string()).or_insert_with(|| font.clone());
        fonts.insert(family, font);
    }
    pub(crate) fn font_asset(&self, family: &str) -> Option<Arc<Font>> {
        let fonts = self.fonts.lock().unwrap();
        fonts.get(family).or_else(|| fonts.get("@default")).cloned()
    }
}

fn read_mesh(path: &Path) -> Result<StaticMesh, String> {
    let bytes = read_limited(path)?;
    let mesh = StaticMesh::from_bytes(&bytes).map_err(|error| error.to_string())?;
    if mesh.vertices.is_empty() || mesh.triangles.is_empty() { return Err("mesh contains no triangles".to_string()); }
    if mesh.vertices.iter().any(|vertex| !vertex.is_finite())
        || mesh.normals.iter().any(|normal| !normal.is_finite())
        || mesh.uvs.iter().any(|uv| !uv.is_finite())
        || mesh.triangles.iter().flatten().any(|index| *index as usize >= mesh.vertices.len()) {
        return Err("mesh has invalid coordinates or triangle indices".to_string());
    }
    Ok(mesh)
}

fn read_image(path: &Path) -> Result<ImageBinding, String> {
    let bytes = read_limited(path)?;
    let mut reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format().map_err(|error| error.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode().map_err(|error| error.to_string())?.to_rgba8();
    Ok(ImageBinding::Pixels { width: image.width(), height: image.height(), pixels: Arc::new(image.into_raw()) })
}

pub fn install(lua: &Lua, state: RenderState) -> LuaResult<LuaValue> {
    inject_roblox_globals(lua)?;
    let (game, workspace) = create_data_model(lua)?;
    install_into(lua, state, game, workspace)
}

pub fn install_into(lua: &Lua, state: RenderState, game: Instance, workspace: Instance) -> LuaResult<LuaValue> {
    install_asset_service(lua)?;
    let module = lua.create_table()?;
    let binding = state.clone();
    module.set("bindEditableMesh", lua.create_function(move |_, (instance, mesh): (LuaUserDataRef<Instance>, LuaUserDataRef<EditableMesh>)| {
        binding.bind_mesh(instance_key(&instance), mesh.clone()); Ok(())
    })?)?;
    let binding = state.clone();
    module.set("bindEditableImage", lua.create_function(move |_, (instance, image): (LuaUserDataRef<Instance>, LuaUserDataRef<EditableImage>)| {
        binding.bind_image(instance_key(&instance), ImageBinding::Editable(image.clone())); Ok(())
    })?)?;
    let assets = state.clone();
    module.set("registerMesh", lua.create_function(move |_, (id, path): (String, String)| {
        assets.register_mesh_asset(id, read_mesh(Path::new(&path)).map_err(LuaError::runtime)?); Ok(())
    })?)?;
    let assets = state.clone();
    module.set("registerImage", lua.create_function(move |_, (id, path): (String, String)| {
        assets.register_image_asset(id, read_image(Path::new(&path)).map_err(LuaError::runtime)?); Ok(())
    })?)?;
    let assets = state.clone();
    module.set("registerAsset", lua.create_function(move |_, (id, path): (String, String)| {
        assets.paths.lock().unwrap().register(&id, Path::new(&path)).map_err(LuaError::runtime)?;
        let key = asset_key(&id);
        assets.mesh_assets.lock().unwrap().remove(&key);
        assets.image_assets.lock().unwrap().remove(&key);
        assets.errors.lock().unwrap().clear();
        Ok(())
    })?)?;
    let assets = state.clone();
    module.set("setAssetRoot", lua.create_function(move |_, path: String| {
        assets.paths.lock().unwrap().set_root(Path::new(&path)).map_err(LuaError::runtime)?;
        assets.mesh_assets.lock().unwrap().clear();
        assets.image_assets.lock().unwrap().clear();
        assets.errors.lock().unwrap().clear();
        Ok(())
    })?)?;
    let binding = state.clone();
    module.set("bindImage", lua.create_function(move |_, (instance, path): (LuaUserDataRef<Instance>, String)| {
        binding.bind_image(instance_key(&instance), read_image(Path::new(&path)).map_err(LuaError::runtime)?); Ok(())
    })?)?;
    let fonts = state.clone();
    module.set("registerFont", lua.create_function(move |_, (family, path): (String, String)| {
        let bytes = read_limited(Path::new(&path)).map_err(LuaError::runtime)?;
        let font = Font::from_bytes(bytes, FontSettings::default()).map_err(LuaError::runtime)?;
        fonts.register_font(family, font); Ok(())
    })?)?;
    let capture_state = state.clone();
    module.set("capture", lua.create_function(move |lua, options: LuaTable| {
        capture(lua, &capture_state, game, workspace, options)
    })?)?;
    module.set("version", "0.3.1")?;
    lua.globals().set("YuneRender", module.clone())?;
    Ok(LuaValue::Table(module))
}

fn inject_roblox_globals(lua: &Lua) -> LuaResult<()> {
    let roblox = lune_roblox::module(lua.clone())?;
    for pair in roblox.pairs::<LuaValue, LuaValue>() {
        let (key, value) = pair?;
        if let LuaValue::String(key) = key { lua.globals().set(key.to_str()?, value)?; }
    }
    Ok(())
}

fn create_data_model(lua: &Lua) -> LuaResult<(Instance, Instance)> {
    let game = Instance::new_orphaned("DataModel");
    game.set_name("Game");
    let workspace = Instance::new_in_dom(game.dom_id, "Workspace");
    workspace.set_parent(Some(game));
    let lighting = Instance::new_in_dom(game.dom_id, "Lighting");
    lighting.set_parent(Some(game));
    let camera = Instance::new_in_dom(game.dom_id, "Camera");
    camera.set_parent(Some(workspace));
    workspace.set_property("CurrentCamera", Variant::Ref(camera.dom_ref));
    lua.globals().set("game", instance_to_lua(lua, game)?)?;
    lua.globals().set("workspace", instance_to_lua(lua, workspace)?)?;
    Ok((game, workspace))
}

fn install_asset_service(lua: &Lua) -> LuaResult<()> {
    let create_mesh = lua.create_function(|_, (_service, _options): (LuaUserDataRef<Instance>, Option<LuaTable>)| Ok(EditableMesh::default()))?;
    InstanceRegistry::insert_method(lua, "AssetService", "CreateEditableMesh", create_mesh).map_err(LuaError::external)?;
    let create_image = lua.create_function(|_, (_service, options): (LuaUserDataRef<Instance>, Option<LuaTable>)| {
        let mut dimensions = (512, 512);
        if let Some(options) = options {
            if let Some(size) = options.get::<Option<LuaUserDataRef<Vector2>>>("Size")? {
                if !size.0.is_finite() || size.0.x < 1.0 || size.0.y < 1.0 || size.0.x > 4096.0 || size.0.y > 4096.0 {
                    return Err(LuaError::runtime("EditableImage Size must be finite and within 1..4096"));
                }
                dimensions = (size.0.x.round() as u32, size.0.y.round() as u32);
            }
        }
        Ok(EditableImage::new(dimensions.0, dimensions.1))
    })?;
    InstanceRegistry::insert_method(lua, "AssetService", "CreateEditableImage", create_image).map_err(LuaError::external)?;
    Ok(())
}

fn capture(lua: &Lua, state: &RenderState, game: Instance, default_workspace: Instance, options: LuaTable) -> LuaResult<LuaTable> {
    let world = table_instance(&options, "world")?.unwrap_or(default_workspace);
    let camera = table_instance(&options, "camera")?
        .or_else(|| props::ref_prop(&default_workspace, "CurrentCamera"))
        .or_else(|| world.get_descendants_preorder().into_iter().find(|instance| instance.get_class_name() == "Camera"))
        .ok_or_else(|| LuaError::runtime("Yune capture requires a Camera"))?;
    let width = options.get::<Option<u32>>("width")?.unwrap_or(1280);
    let height = options.get::<Option<u32>>("height")?.unwrap_or(720);
    if width == 0 || height == 0 || width > 8192 || height > 8192 || width as u64 * height as u64 > 16_777_216 {
        return Err(LuaError::runtime("capture dimensions must be 1..8192, with at most 16,777,216 pixels"));
    }
    let path = options.get::<Option<String>>("path")?;
    let strict = options.get::<Option<bool>>("strictAssets")?.unwrap_or(true);
    state.errors.lock().unwrap().clear();
    let clear = match options.get::<LuaValue>("clearColor")? {
        LuaValue::UserData(value) => {
            let color = *value.borrow::<Color3>()?;
            let color: DomColor3 = color.into();
            Vec3::new(color.r, color.g, color.b)
        }
        LuaValue::Nil => Vec3::new(0.55, 0.72, 0.9),
        _ => return Err(LuaError::runtime("clearColor must be Color3")),
    };
    let mut framebuffer = Framebuffer::new(width, height);
    framebuffer.clear(props::color_to_rgba(clear, 0.0));
    let mut lighting = Lighting::default();
    if let Some(instance) = game.get_children().into_iter().find(|instance| instance.get_class_name() == "Lighting") {
        lighting.ambient = color_prop(&instance, "Ambient", lighting.ambient);
        lighting.outdoor_ambient = color_prop(&instance, "OutdoorAmbient", lighting.outdoor_ambient);
        lighting.brightness = props::f32_prop(&instance, "Brightness", lighting.brightness).max(0.0);
        lighting.exposure = props::f32_prop(&instance, "ExposureCompensation", lighting.exposure);
        lighting.fog_color = color_prop(&instance, "FogColor", lighting.fog_color);
        lighting.fog_start = props::f32_prop(&instance, "FogStart", lighting.fog_start).max(0.0);
        lighting.fog_end = props::f32_prop(&instance, "FogEnd", lighting.fog_end).max(lighting.fog_start);
    }
    if let Some(direction) = options.get::<Option<LuaUserDataRef<Vector3>>>("lightDirection")? {
        lighting.light_direction = direction.0.normalize_or_zero();
    }
    let world_stats = render_world(&mut framebuffer, world, camera, state, lighting);
    let world_gui_stats = render_world_guis(&mut framebuffer, world, camera, state);
    let mut gui_objects = world_gui_stats.objects;
    let mut viewport_parts = world_gui_stats.viewport_parts;
    let mut viewport_triangles = world_gui_stats.viewport_triangles;
    let roots = if options.get::<LuaValue>("gui")?.is_nil() { default_gui_roots(game) } else { gui_roots(&options)? };
    for root in roots {
        let stats = render_gui(&mut framebuffer, root, state);
        gui_objects += stats.objects;
        viewport_parts += stats.viewport_parts;
        viewport_triangles += stats.viewport_triangles;
    }
    let errors = state.errors.lock().unwrap().clone();
    if strict && !errors.is_empty() {
        let messages = errors.iter().map(|(asset, message)| format!("{asset}: {message}")).collect::<Vec<_>>();
        return Err(LuaError::runtime(format!("unresolved render assets:\n{}", messages.join("\n"))));
    }
    if let Some(path) = &path { framebuffer.save_png(path).map_err(LuaError::external)?; }
    let result = lua.create_table()?;
    result.set("width", width)?;
    result.set("height", height)?;
    result.set("parts", world_stats.parts + viewport_parts)?;
    result.set("triangles", world_stats.triangles + viewport_triangles)?;
    result.set("guiObjects", gui_objects)?;
    let diagnostics = lua.create_table()?;
    for (asset, message) in errors {
        let entry = lua.create_table()?;
        entry.set("asset", asset)?;
        entry.set("message", message)?;
        diagnostics.push(entry)?;
    }
    result.set("assetErrors", diagnostics)?;
    if options.get::<Option<bool>>("includePixels")?.unwrap_or(false) {
        result.set("pixels", lua.create_buffer(&framebuffer.pixels)?)?;
    }
    if let Some(path) = path { result.set("path", path)?; }
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
        LuaValue::Table(values) => values.sequence_values::<LuaAnyUserData>()
            .map(|value| value.and_then(|value| Ok(*value.borrow::<Instance>()?))).collect(),
        value => Err(LuaError::runtime(format!("'gui' must be an Instance or array, got {}", value.type_name()))),
    }
}
fn default_gui_roots(game: Instance) -> Vec<Instance> {
    let mut roots = Vec::new();
    for child in game.get_children() {
        if child.get_class_name() == "CoreGui" { roots.push(child); }
        if child.get_class_name() == "Players" {
            if let Some(player) = props::ref_prop(&child, "LocalPlayer") {
                roots.extend(player.get_children().into_iter().filter(|instance| instance.get_class_name() == "PlayerGui"));
            }
        }
    }
    roots
}
