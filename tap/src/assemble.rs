use entity::{Entity, Pointer, StoreItem};

use std::collections::{HashMap, HashSet};

use serde_json::Map as JMap;
use serde_json::Value as JValue;

use entitystore::EntityStore;
use futures::prelude::*;

#[async(boxed_send)]
/// Assemble a single [`Pointer`], avoiding cycles and repeating objects.
fn _assemble_val<T: EntityStore>(
    value: Pointer,
    depth: u32,
    mut items: HashMap<String, Entity>,
    mut store: Option<T>,
    mut seen: HashSet<String>,
) -> Result<(Option<T>, HashMap<String, Entity>, HashSet<String>, JValue), T::Error> {
    match value {
        Pointer::Id(id) => {
            if seen.contains(&id) {
                let mut hash = JMap::new();
                hash.insert("@id".to_owned(), JValue::String(id));
                return Ok((store, items, seen, JValue::Object(hash)));
            } else if items.contains_key(&id) {
                let item = items.remove(&id).unwrap();
                return await!(_assemble(item, depth + 1, items, store, seen));
            }
            if depth < 3 {
                if let Some(mut store) = store {
                    let item = await!(store.get(id.to_owned()))?;
                    if let Some(item) = item {
                        seen.insert(id);
                        let (s, t, o) = await!(assemble(item, depth + 1, Some(store), seen))?;
                        return Ok((t, items, s, o));
                    } else {
                        let mut hash = JMap::new();
                        hash.insert("@id".to_owned(), JValue::String(id));
                        return Ok((Some(store), items, seen, JValue::Object(hash)));
                    }
                }
            }

            {
                let mut hash = JMap::new();
                hash.insert("@id".to_owned(), JValue::String(id));
                Ok((store, items, seen, JValue::Object(hash)))
            }
        }

        Pointer::Value(val) => Ok((store, items, seen, Pointer::Value(val).to_json())),

        Pointer::List(list) => {
            let mut vals = Vec::new();
            for item in list {
                let (nstore, nitems, nseen, res) =
                    await!(_assemble_val(item, depth, items, store, seen))?;
                seen = nseen;
                store = nstore;
                items = nitems;
                vals.push(res);
            }

            let mut map = JMap::new();
            map.insert("@list".to_owned(), JValue::Array(vals));
            Ok((store, items, seen, JValue::Object(map)))
        }
    }
}

#[async(boxed_send)]
fn _assemble<T: EntityStore>(
    item: Entity,
    depth: u32,
    mut items: HashMap<String, Entity>,
    mut store: Option<T>,
    mut seen: HashSet<String>,
) -> Result<(Option<T>, HashMap<String, Entity>, HashSet<String>, JValue), T::Error> {
    let mut map = JMap::new();
    map.insert("@id".to_owned(), JValue::String(item.id));

    if let Some(index) = item.index {
        map.insert("@index".to_owned(), JValue::String(index));
    }

    for (key, values) in item.data {
        let mut out = Vec::new();

        for value in values {
            let (nstore, nitems, nseen, res) =
                await!(_assemble_val(value, depth, items, store, seen))?;
            store = nstore;
            items = nitems;
            seen = nseen;
            out.push(res);
        }

        map.insert(key, JValue::Array(out));
    }

    Ok((store, items, seen, JValue::Object(map)))
}

#[async(boxed_send)]
/// Assembles a `StoreItem`, ensuring that no cycles happen.
///
/// If this code were to infinitely recurse when assembling a `StoreItem`,
/// this would easily allow a remote server to DoS this server.
///
/// Also, due to limitations in `Future`s, this function takes ownership of
/// the `EntityStore` passed into it, and returns it in a tuple when the
/// future is fullfilled.
///
/// Currently, if the future fails, the `EntityStore` is completely consumed.
/// This may change in the future.
pub fn assemble<T: EntityStore>(
    mut item: StoreItem,
    depth: u32,
    store: Option<T>,
    seen: HashSet<String>,
) -> Result<(HashSet<String>, Option<T>, JValue), T::Error> {
    let main = item.data.remove(&item.id).unwrap();

    let (nstore, _, nseen, val) = await!(_assemble(main, depth, item.data, store, seen))?;

    Ok((nseen, nstore, val))
}
