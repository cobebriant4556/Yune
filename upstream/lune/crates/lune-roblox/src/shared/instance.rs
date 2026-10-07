#![allow(dead_code)]

use std::{borrow::Cow, collections::HashMap, sync::{LazyLock, RwLock}};
use rbx_dom_weak::types::{Variant as DomValue, VariantType as DomType};
use rbx_reflection::{ClassTag, DataType};
use thiserror::Error;

/** A class defined at runtime that the reflection database does not know about. */
#[derive(Debug, Clone)]
struct CustomClass {
    superclass: String,
    is_service: bool,
}

fn custom_classes() -> &'static RwLock<HashMap<String, CustomClass>> {
    static CUSTOM_CLASSES: LazyLock<RwLock<HashMap<String, CustomClass>>> = LazyLock::new(|| RwLock::new(HashMap::new()));
    &CUSTOM_CLASSES
}

/** Error that may occur when registering a custom class. */
#[derive(Debug, Clone, Error)]
pub enum CustomClassError {
    #[error("class '{0}' already exists and cannot be redefined")]
    AlreadyExists(String),
    #[error("superclass '{0}' is not a valid class name")]
    InvalidSuperclass(String),
}

/**
    Registers a class not present in the built-in reflection database.
    The superclass must already exist. Built-in and duplicate class names are rejected.
*/
pub fn register_custom_class(class_name: &str, superclass: &str, is_service: bool) -> Result<(), CustomClassError> {
    if class_exists(class_name) { return Err(CustomClassError::AlreadyExists(class_name.to_string())); }
    if !class_exists(superclass) { return Err(CustomClassError::InvalidSuperclass(superclass.to_string())); }
    custom_classes().write().unwrap().insert(class_name.to_string(), CustomClass { superclass: superclass.to_string(), is_service });
    Ok(())
}

fn superclass_of(class_name: &str) -> Option<String> {
    let db = rbx_reflection_database::get().unwrap();
    if let Some(class) = db.classes.get(class_name) { return class.superclass.as_ref().map(ToString::to_string); }
    custom_classes().read().unwrap().get(class_name).map(|class| class.superclass.clone())
}

#[derive(Debug, Clone, Default)]
pub(crate) struct PropertyInfo {
    pub enum_name: Option<Cow<'static, str>>,
    pub enum_default: Option<u32>,
    pub value_type: Option<DomType>,
    pub value_default: Option<&'static DomValue>,
}

/** Finds a property's definition and inherited default without inventing unknown properties. */
pub(crate) fn find_property_info(instance_class: impl AsRef<str>, property_name: impl AsRef<str>) -> Option<PropertyInfo> {
    let db = rbx_reflection_database::get().unwrap();
    let instance_class = instance_class.as_ref();
    let property_name = property_name.as_ref();
    if matches!(property_name, "Attributes" | "Tags") { return None; }

    let mut class_info = None;
    let mut current = Some(instance_class.to_string());
    while let Some(class_name) = current {
        if let Some(class) = db.classes.get(class_name.as_str()) {
            if let Some(definition) = class.properties.get(property_name) {
                class_info = Some(match &definition.data_type {
                    DataType::Enum(name) => PropertyInfo { enum_name: Some(Cow::Borrowed(name)), ..Default::default() },
                    DataType::Value(value_type) => PropertyInfo { value_type: Some(*value_type), ..Default::default() },
                    _ => PropertyInfo::default(),
                });
                break;
            }
        }
        current = superclass_of(&class_name);
    }

    if let Some(info) = class_info.as_mut() {
        let mut current = Some(instance_class.to_string());
        while let Some(class_name) = current {
            if let Some(class) = db.classes.get(class_name.as_str()) {
                if let Some(default) = class.default_properties.get(property_name) {
                    if info.enum_name.is_some() {
                        info.enum_default = match default { DomValue::Enum(value) => Some(value.to_u32()), _ => None };
                    } else if info.value_type.is_some() { info.value_default = Some(default); }
                    break;
                }
            }
            current = superclass_of(&class_name);
        }

        // These non-serialized pose properties can be absent from a generated
        // reflection-default snapshot even though new engine instances start at identity.
        if info.value_default.is_none() && info.value_type == Some(DomType::CFrame) && property_name == "Transform"
            && (class_is_a(instance_class, "Motor6D") == Some(true) || class_is_a(instance_class, "Bone") == Some(true)) {
            static IDENTITY: LazyLock<DomValue> = LazyLock::new(|| DomValue::CFrame(crate::datatypes::types::CFrame::IDENTITY.into()));
            info.value_default = Some(&*IDENTITY);
        }
    }
    class_info
}

pub fn class_exists(class_name: impl AsRef<str>) -> bool {
    let class_name = class_name.as_ref();
    let db = rbx_reflection_database::get().unwrap();
    db.classes.contains_key(class_name) || custom_classes().read().unwrap().contains_key(class_name)
}

#[must_use]
pub fn class_name_chain(class_name: &str) -> Vec<String> {
    let mut list = vec![class_name.to_string()];
    let mut current = class_name.to_string();
    while let Some(superclass) = superclass_of(&current) {
        list.push(superclass.clone());
        current = superclass;
    }
    list
}

pub fn class_is_a(instance_class: impl AsRef<str>, class_name: impl AsRef<str>) -> Option<bool> {
    let class_name = class_name.as_ref();
    let mut current = instance_class.as_ref().to_string();
    if class_name == "Instance" || current == class_name { return Some(true); }
    while current != class_name {
        if !class_exists(&current) { return None; }
        match superclass_of(&current) { Some(superclass) => current = superclass, None => return Some(false) }
    }
    Some(true)
}

pub fn class_is_a_service(instance_class: impl AsRef<str>) -> Option<bool> {
    let mut current = instance_class.as_ref().to_string();
    let db = rbx_reflection_database::get().unwrap();
    loop {
        if let Some(custom) = custom_classes().read().unwrap().get(&current).cloned() {
            if custom.is_service { return Some(true); }
            current = custom.superclass;
            continue;
        }
        let class = db.classes.get(current.as_str())?;
        if class.tags.contains(&ClassTag::Service) { return Some(true); }
        if let Some(superclass) = &class.superclass { current = superclass.to_string(); } else { break; }
    }
    Some(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_a_class_valid() {
        assert_eq!(class_is_a("Part", "Part"), Some(true));
        assert_eq!(class_is_a("Part", "BasePart"), Some(true));
        assert_eq!(class_is_a("Part", "PVInstance"), Some(true));
        assert_eq!(class_is_a("Part", "Instance"), Some(true));
        assert_eq!(class_is_a("Workspace", "Workspace"), Some(true));
        assert_eq!(class_is_a("Workspace", "Model"), Some(true));
        assert_eq!(class_is_a("Workspace", "Instance"), Some(true));
    }
    #[test]
    fn is_a_class_invalid() {
        for name in ["part", "Base-Part", "Model", "Paart"] { assert_eq!(class_is_a("Part", name), Some(false)); }
        for name in ["Service", ".", ""] { assert_eq!(class_is_a("Workspace", name), Some(false)); }
    }
    #[test]
    fn is_a_service_valid() {
        for name in ["Workspace", "PhysicsService", "ReplicatedFirst", "CSGDictionaryService"] { assert_eq!(class_is_a_service(name), Some(true)); }
    }
    #[test]
    fn is_a_service_invalid() {
        assert_eq!(class_is_a_service("Camera"), Some(false));
        assert_eq!(class_is_a_service("Terrain"), Some(false));
        assert_eq!(class_is_a_service("Work-space"), None);
        assert_eq!(class_is_a_service("CSG Dictionary Service"), None);
    }
    #[test]
    fn custom_class_register_and_exists() {
        assert!(!class_exists("CustomReg"));
        register_custom_class("CustomReg", "Instance", false).unwrap();
        assert!(class_exists("CustomReg"));
        assert_eq!(class_is_a("CustomReg", "CustomReg"), Some(true));
        assert_eq!(class_is_a("CustomReg", "Instance"), Some(true));
        assert_eq!(class_is_a("CustomReg", "Part"), Some(false));
        assert_eq!(class_is_a_service("CustomReg"), Some(false));
        assert!(find_property_info("CustomReg", "Archivable").is_some());
        assert!(find_property_info("CustomReg", "DefinitelyNotAProperty").is_none());
    }
    #[test]
    fn custom_service_is_service() {
        register_custom_class("CustomSvc", "Instance", true).unwrap();
        assert!(class_exists("CustomSvc"));
        assert_eq!(class_is_a_service("CustomSvc"), Some(true));
        assert_eq!(class_is_a("CustomSvc", "Instance"), Some(true));
    }
    #[test]
    fn custom_class_register_errors() {
        assert!(matches!(register_custom_class("Part", "Instance", false), Err(CustomClassError::AlreadyExists(_))));
        register_custom_class("CustomDup", "Instance", false).unwrap();
        assert!(matches!(register_custom_class("CustomDup", "Instance", false), Err(CustomClassError::AlreadyExists(_))));
        assert!(matches!(register_custom_class("CustomBadSuper", "NotARealClass", false), Err(CustomClassError::InvalidSuperclass(_))));
    }
    #[test]
    fn custom_class_inheritance() {
        register_custom_class("CustomBase", "Instance", false).unwrap();
        register_custom_class("CustomDerived", "CustomBase", false).unwrap();
        assert_eq!(class_is_a("CustomDerived", "CustomBase"), Some(true));
        assert_eq!(class_is_a("CustomDerived", "Instance"), Some(true));
        assert_eq!(class_is_a("CustomDerived", "Part"), Some(false));
        let chain = class_name_chain("CustomDerived");
        assert_eq!(&chain[..3], &["CustomDerived", "CustomBase", "Instance"]);
    }
    #[test]
    fn class_name_chain_unknown_is_singleton() {
        assert_eq!(class_name_chain("TotallyUnknownClass"), vec!["TotallyUnknownClass"]);
    }
    #[test]
    fn animated_transform_defaults_to_identity() {
        let expected = DomValue::CFrame(crate::datatypes::types::CFrame::IDENTITY.into());
        for name in ["Motor6D", "Bone"] {
            assert_eq!(find_property_info(name, "Transform").unwrap().value_default, Some(&expected));
        }
    }
}
