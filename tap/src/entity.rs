//! This module contains a few structs that make handling JSON-LD in Rust way easier.
//!
//! They define a set of structs that translate the most important subset of JSON-LD
//! in a way that is easier to process, by removing lots of JSON boilerplate.

use jsonld::nodemap::{generate_node_map, BlankNodeGenerator, Entity, NodeMapError, Pointer};
use serde_json::Value as JValue;
use std::collections::HashMap;

use crate::user::Context;

#[derive(Clone, Debug)]
/// A result from an `EntityStore` response, containing a `main` ID and a map of `sub`
/// items. The `sub` items are all locally namespaced blank nodes.
pub struct StoreItem {
    pub(crate) id: String,
    pub(crate) data: HashMap<String, Entity>,
    i: u32,
}

pub struct StoreItemNodeGenerator {
    i: u32,
    data: HashMap<String, String>,
}

impl StoreItemNodeGenerator {
    pub fn new() -> StoreItemNodeGenerator {
        StoreItemNodeGenerator {
            i: 0,
            data: HashMap::new(),
        }
    }
}

impl BlankNodeGenerator for StoreItemNodeGenerator {
    fn generate_blank_node(&mut self, id: Option<&str>) -> String {
        if let Some(id) = id {
            if id.starts_with("_:http") {
                return id.to_owned();
            }

            if !self.data.contains_key(id) {
                let new_id = format!("_:b{}", self.i);
                self.i += 1;
                self.data.insert(id.to_owned(), new_id);
            }

            self.data[id].to_owned()
        } else {
            self.i += 1;
            format!("_:b{}", self.i - 1)
        }
    }
}

impl StoreItem {
    pub fn id(&self) -> &str {
        &self.id
    }

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

    /// Gets the special meta-entity where parameters are stored.
    pub fn meta(&mut self) -> &mut Entity {
        if !self.data.contains_key(kroeg!(meta)) {
            self.data.insert(
                kroeg!(meta).to_owned(),
                Entity::new(kroeg!(meta).to_owned()),
            );
        }

        self.data.get_mut(kroeg!(meta)).unwrap()
    }

    pub fn remove(&mut self, id: &str) -> Option<Entity> {
        self.data.remove(id)
    }

    /// Translates this `StoreItem` to the JSON-LD this was generated from.
    pub fn to_json(self) -> JValue {
        let mut vec = Vec::new();
        for (_, v) in self.data {
            vec.push(v.into_json());
        }

        JValue::Array(vec)
    }

    /// Returns if the instance ID of this object equals the instance ID in the current context.
    pub fn is_owned(&self, context: &Context) -> bool {
        if let Some(data) = self.data.get(kroeg!(meta)) {
            let data = &data[kroeg!(instance)];
            if data.len() != 1 {
                false
            } else {
                match &data[0] {
                    Pointer::Value(val) => val.value == JValue::Number(context.instance_id.into()),

                    _ => false,
                }
            }
        } else {
            false
        }
    }
    /// Parse a JSON object containing flattened JSON-LD into a `StoreItem`.
    ///
    /// The `main` property is used to store the main object, it should be
    /// the only non-blank node in the map.
    pub fn parse(main: &str, entity: &JValue) -> Result<StoreItem, NodeMapError> {
        let mut node_map = generate_node_map(entity, &mut StoreItemNodeGenerator::new())?
            .remove("@default")
            .unwrap();
        node_map.retain(|_, v| v.iter().next().is_some());
        if !node_map.contains_key(main) {
            node_map.insert(main.to_owned(), Entity::new(main.to_owned()));
        }
        Ok(StoreItem {
            id: main.to_owned(),
            data: node_map,
            i: 0,
        })
    }

    pub fn new(main: String, mut data: HashMap<String, Entity>) -> StoreItem {
        if !data.contains_key(&main) {
            data.insert(main.to_owned(), Entity::new(main.to_owned()));
        }
        StoreItem {
            id: main,
            data: data,
            i: 0,
        }
    }
}
