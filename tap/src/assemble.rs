use entity::StoreItem;

use std::collections::{HashMap, HashSet};

use serde_json::Map as JMap;
use serde_json::Value as JValue;

use entitystore::EntityStore;
use futures::prelude::*;

use jsonld::nodemap::{generate_node_map, DefaultNodeGenerator, Entity, NodeMapError, Pointer};

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
    seen.insert(item.id.to_owned());

    let mut map = JMap::new();
    map.insert("@id".to_owned(), JValue::String(item.id.to_owned()));

    if let Some(index) = item.index.to_owned() {
        map.insert("@index".to_owned(), JValue::String(index));
    }

    map.insert(
        "@type".to_owned(),
        JValue::Array(
            item.types
                .iter()
                .map(|f| JValue::String(f.to_owned()))
                .collect(),
        ),
    );

    for (key, values) in item.into_data() {
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

fn _untangle_vec(data: &Vec<Pointer>, tangles: &mut Vec<String>) {
    for value in data {
        match value {
            Pointer::Id(ref id) => tangles.push(id.to_owned()),
            Pointer::List(ref list) => _untangle_vec(list, tangles),
            _ => {}
        };
    }
}

/// Untangles a JSON-LD object, returning all the objects split up into their respective
/// `StoreItem`s. May not return the expected value in some cases.
pub fn untangle(data: JValue) -> Result<HashMap<String, StoreItem>, NodeMapError> {
    // the tangle map stores a list of node -> node mappings
    let mut tangle_map: HashMap<String, Vec<String>> = HashMap::new();

    let mut flattened = generate_node_map(data, &mut DefaultNodeGenerator::new())?
        .remove("@default")
        .unwrap();
    flattened.retain(|_, v| v.iter().next().is_some());

    let mut free: HashSet<_> = flattened.keys().map(|f| f.to_owned()).collect();
    for (key, item) in flattened.iter() {
        let mut tangles = Vec::new();
        for (_, ivalues) in item.iter() {
            _untangle_vec(ivalues, &mut tangles);
        }

        for tangle in tangles.iter() {
            free.remove(tangle);
        }

        if item.iter().next().is_some() {
            tangle_map.insert(key.to_owned(), tangles);
        } else {
            free.remove(key);
        }
    }

    let mut untangled = HashSet::new();
    let mut roots: Vec<_> = tangle_map
        .keys()
        .filter(|a| !a.starts_with("_:"))
        .map(|a| a.to_owned())
        .collect();

    // no roots, so we can skip all this magic
    if roots.len() == 0 {
        let k = free.iter().next().or(tangle_map.keys().next()).unwrap();
        let mut map = HashMap::new();
        map.insert(k.to_owned(), StoreItem::new(k.to_owned(), flattened));
        return Ok(map);
    }

    let mut result = HashMap::new();
    for root in roots {
        let mut to_untangle = tangle_map.remove(&root).unwrap();
        let mut items = HashMap::new();
        items.insert(root.to_owned(), flattened.remove(&root).unwrap());
        while to_untangle.len() > 0 {
            let item = to_untangle.pop().unwrap();
            if !item.starts_with("_:") || !tangle_map.contains_key(&item) {
                continue;
            }

            if untangled.contains(&item) {
                panic!("too tangled")
            }

            to_untangle.append(&mut tangle_map.remove(&item).unwrap());
            items.insert(item.to_owned(), flattened.remove(&item).unwrap());
            untangled.insert(item);
        }

        let storeitem = StoreItem::new(root.to_owned(), items);
        result.insert(root, storeitem);
    }

    Ok(result)
}
