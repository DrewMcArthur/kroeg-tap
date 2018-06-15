//! This module contains a few structs that make handling JSON-LD in Rust way easier.
//!
//! They define a set of structs that translate the most important subset of JSON-LD
//! in a way that is easier to process, by removing lots of JSON boilerplate.

use serde_json::Map as JMap;
use serde_json::Value as JValue;

use std::collections::HashMap;
use std::ops::{Index, IndexMut};
use std::{error, fmt};

#[derive(Debug)]
/// Errors that can occur when translating JSON-LD into a `StoreItem`.
pub enum EntityError {
    /// The value passed into the method is not an object.
    NotObject,

    /// Entity does not contain an `@id`.
    MissingId,

    /// The `@index` value exists, but is not a string.
    IndexInvalid,

    /// The value for a specific ID is not an array.
    ValueNotArray(String),

    /// The contents of the array for a specific ID is not an object.
    ArrayContentsNotObject(String),

    /// The `@value` object has an invalid set of properties.
    ValueInvalid(String),
}

impl fmt::Display for EntityError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            EntityError::NotObject => write!(f, "passed value is not an object"),
            EntityError::MissingId => write!(f, "object is missing an @id"),
            EntityError::IndexInvalid => write!(f, "@index is not a string"),
            EntityError::ValueNotArray(ref val) => write!(f, "`{}` is not an array", val),
            EntityError::ArrayContentsNotObject(ref val) => {
                write!(f, "`{}` array does not contain object", val)
            }
            EntityError::ValueInvalid(ref val) => write!(f, "`{}` value object is invalid", val),
        }
    }
}

impl error::Error for EntityError {
    fn cause(&self) -> Option<&error::Error> {
        None
    }
}

#[derive(PartialEq, Clone, Debug)]
/// A pointer to either a value, list of pointers, or another JSON-LD object.
pub enum Pointer {
    /// Points to another ID object. if blank node (_:), the object is
    /// stored in the local `StoreItem`. else, it refers to the main object
    /// of any StoreItem, local or not local.
    Id(String),

    /// A value, with `@type` or `@language`, and `@value`.
    Value(Value),

    /// A list of pointers. This list may not contain other lists.
    List(Vec<Pointer>),
}

impl Pointer {
    /// Translates this `Pointer` to the JSON-LD this was generated from.
    pub fn to_json(self) -> JValue {
        let mut map = JMap::new();
        match self {
            Pointer::Id(id) => {
                map.insert("@id".to_owned(), JValue::String(id));
            }
            Pointer::List(list) => {
                map.insert(
                    "@list".to_owned(),
                    JValue::Array(list.into_iter().map(|f| f.to_json()).collect()),
                );
            }
            Pointer::Value(val) => {
                map.insert("@value".to_owned(), val.value);
                if let Some(tid) = val.type_id {
                    map.insert("@type".to_owned(), JValue::String(tid));
                }

                if let Some(lang) = val.language {
                    map.insert("@language".to_owned(), JValue::String(lang));
                }
            }
        };

        JValue::Object(map)
    }

    /// Parse a single JSON-LD `@value`, `@id`, or `@list` value into a `Pointer`.
    pub fn parse(entity: JValue, key: &str) -> Result<Pointer, EntityError> {
        if let JValue::Object(mut entity) = entity {
            let result = match (
                entity.remove("@id"),
                entity.remove("@value"),
                entity.remove("@type"),
                entity.remove("@language"),
                entity.remove("@list"),
            ) {
                (Some(JValue::String(id)), None, None, None, None) => Pointer::Id(id),
                (None, Some(value), None, None, None) => Pointer::Value(Value {
                    value: value,
                    type_id: None,
                    language: None,
                }),
                (None, Some(value), Some(JValue::String(typ)), None, None) => {
                    Pointer::Value(Value {
                        value: value,
                        type_id: Some(typ),
                        language: None,
                    })
                }
                (None, Some(value), None, Some(JValue::String(lang)), None) => {
                    Pointer::Value(Value {
                        value: value,
                        type_id: None,
                        language: Some(lang),
                    })
                }
                (None, None, None, None, Some(JValue::Array(mut arr))) => {
                    let mut result = Vec::new();
                    for item in arr {
                        result.push(Pointer::parse(item, key)?);
                    }
                    Pointer::List(result)
                }
                _ => {
                    return Err(EntityError::ValueInvalid(key.to_owned()));
                }
            };

            Ok(result)
        } else {
            Err(EntityError::ArrayContentsNotObject(key.to_owned()))
        }
    }
}

#[derive(PartialEq, Debug, Clone)]
/// The equivalent to a JSON-LD `@value` object
pub struct Value {
    /// The value contained within this JSON-LD value object. If `type_id` is
    /// `None`, the interpretations are trivial, else the contents are String
    /// and `type_id` explains how to interpret it.
    pub value: JValue,

    /// The (optional) type ID of this value object. `type_id` and `language`
    /// cannot be `Some` at the same time. If this is None, the `value` field
    /// may be any other JSON primitive.
    pub type_id: Option<String>,

    /// The (optional) language of the value object. If `Some`, the value is
    /// always a language string, and should be interpreted as such.
    pub language: Option<String>,
}

#[derive(Clone, Debug)]
/// A simplified JSON-LD entity, containing only value objects, list objects,
/// and references to other JSON-LD objects. Usually, this entity is retrieved
/// from a `StoreItem`, which is returned from an `EntityStore`, to give it
/// context.
///
/// This struct contains a dirty flag, which is set when any mutable method is
/// called.
pub struct Entity {
    pub(crate) id: String,
    pub index: Option<String>,
    pub(crate) data: HashMap<String, Vec<Pointer>>,
    empty: Vec<Pointer>,
}

impl Entity {
    fn new(id: String) -> Entity {
        Entity {
            id: id,
            index: None,
            data: HashMap::new(),
            empty: Vec::new(),
        }
    }

    /// Gets the ID of this entity.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Gets a list of values contained in this entity based on predicate.
    ///
    /// This function never fails, if an unknown value is passed in it will
    /// return an empty array.
    pub fn get(&self, val: &str) -> &Vec<Pointer> {
        self.data.get(val).unwrap_or(&self.empty)
    }

    /// Gets a mutable list of values in this element based on the predicate.
    ///
    /// If an unknown value is passed in, it will return a new empty array.
    pub fn get_mut(&mut self, val: &str) -> &mut Vec<Pointer> {
        self.data
            .entry(val.to_owned())
            .or_insert_with(|| Vec::new())
    }

    /// Translates this `Entity` to the JSON-LD this was generated from.
    pub fn to_json(self) -> JValue {
        let mut map = JMap::new();

        map.insert("@id".to_owned(), JValue::String(self.id));
        if let Some(index) = self.index {
            map.insert("@index".to_owned(), JValue::String(index));
        }

        for (k, v) in self.data {
            map.insert(
                k,
                JValue::Array(v.into_iter().map(|f| f.to_json()).collect()),
            );
        }

        JValue::Object(map)
    }

    /// Parses a flattened JSON-LD object into an `Entity`.
    pub fn parse(entity: JValue) -> Result<Entity, EntityError> {
        if let JValue::Object(mut entity) = entity {
            if entity.contains_key("@value") {
                return Err(EntityError::NotObject);
            }

            let mut result = Entity {
                id: if let Some(JValue::String(id)) = entity.remove("@id") {
                    id
                } else {
                    return Err(EntityError::MissingId);
                },
                index: match entity.remove("@index") {
                    Some(JValue::String(id)) => Some(id),
                    Some(_) => return Err(EntityError::IndexInvalid),
                    None => None,
                },
                data: HashMap::new(),
                empty: Vec::new(),
            };

            for (key, value) in entity {
                if let JValue::Array(mut values) = value {
                    let mut rarr = Vec::new();
                    for value in values {
                        rarr.push(Pointer::parse(value, &key)?);
                    }
                    result.data.insert(key, rarr);
                } else {
                    return Err(EntityError::ValueNotArray(key));
                }
            }

            Ok(result)
        } else {
            Err(EntityError::MissingId)
        }
    }
}

impl<'a> Index<&'a str> for Entity {
    type Output = Vec<Pointer>;

    fn index<'b>(&'b self, index: &'a str) -> &'b Vec<Pointer> {
        self.get(index)
    }
}

impl<'a> IndexMut<&'a str> for Entity {
    fn index_mut<'b>(&'b mut self, index: &'a str) -> &'b mut Vec<Pointer> {
        self.get_mut(index)
    }
}

#[derive(Clone, Debug)]
/// A result from an `EntityStore` response, containing a `main` ID and a map of `sub`
/// items. The `sub` items are all locally namespaced blank nodes.
pub struct StoreItem {
    pub(crate) id: String,
    pub(crate) data: HashMap<String, Entity>,
    i: u32,
}

impl StoreItem {
    /// Retrieves the main entity in this store item.
    pub fn main(&self) -> &Entity {
        &self.data[&self.id]
    }

    /// Retrieves the main entity in this store item mutably.
    pub fn main_mut(&mut self) -> &mut Entity {
        self.data.get_mut(&self.id).unwrap()
    }

    /// Retrieves a sub-item with a specific ID.
    pub fn sub(&self, id: &str) -> Option<&Entity> {
        self.data.get(id)
    }

    /// Retrieves a sub-item with a specific ID, mutably.
    pub fn sub_mut(&mut self, id: &str) -> Option<&mut Entity> {
        self.data.get_mut(id)
    }

    /// Creates a new sub-item with a randomly assigned blank node.
    pub fn create(&mut self) -> &mut Entity {
        let id = loop {
            self.i += 1;

            let id = format!("_:nb{}", self.i);
            if !self.data.contains_key(&id) {
                break id;
            }
        };

        self.data.insert(id.to_owned(), Entity::new(id.to_owned()));

        self.data.get_mut(&id).unwrap()
    }

    /// Translates this `StoreItem` to the JSON-LD this was generated from.
    pub fn to_json(self) -> JValue {
        let mut vec = Vec::new();
        for (_, v) in self.data {
            vec.push(v.to_json());
        }

        JValue::Array(vec)
    }

    /// Parse a JSON object containing flattened JSON-LD into a `StoreItem`.
    ///
    /// The `main` property is used to store the main object, it should be
    /// the only non-blank node in the map.
    pub fn parse(main: &str, entity: JValue) -> Result<StoreItem, EntityError> {
        if let JValue::Object(entity) = entity {
            let mut entity_map = HashMap::new();
            for (key, value) in entity {
                entity_map.insert(key, Entity::parse(value)?);
            }

            Ok(StoreItem {
                id: main.to_owned(),
                data: entity_map,
                i: 0,
            })
        } else {
            Err(EntityError::MissingId)
        }
    }
}
