use entity::StoreItem;

use std::collections::{HashMap, HashSet};

use serde_json::Map as JMap;
use serde_json::Value as JValue;

use entitystore::EntityStore;
use futures::prelude::{await, *};

use auth::Authorizer;

use jsonld::nodemap::{generate_node_map, DefaultNodeGenerator, Entity, NodeMapError, Pointer};

#[async]
fn _get_collectionified<T: EntityStore>(
    store: T,
    id: String,
) -> Result<(Option<StoreItem>, T), (T::Error, T)> {
    let without_query = id.split('&').next().unwrap().to_string();
    if without_query == id {
        await!(store.get(id.to_owned(), true))
    } else {
        let (val, store) = await!(store.get(without_query.to_owned(), true))?;
        if let Some(val) = val {
            if !val
                .main()
                .types
                .contains(&as2!(OrderedCollection).to_string())
            {
                return Ok((None, store));
            }

            let (data, store) = await!(store.read_collection(without_query.to_owned(), None, None))?;

            Ok((
                Some(
                    StoreItem::parse(
                        &id,
                        json!({
                            "@id": id,
                            "@type": [as2!(OrderedCollectionPage)],
                            as2!(partOf): [{"@id": without_query}],
                            "orderedItems": [{"@list": data.items}]
                        }),
                    ).expect("static input cannot fail"),
                ),
                store,
            ))
        } else {
            Ok((None, store))
        }
    }
}

const AVOID_ASSEMBLE: [&'static str; 12] = [
    as2!(url),
    ldp!(inbox),
    as2!(outbox),
    as2!(sharedInbox),
    as2!(href),
    as2!(followers),
    as2!(following),
    as2!(to),
    as2!(cc),
    as2!(bto),
    as2!(bcc),
    "http://ostatus.org/#conversation",
];

#[async(boxed_send)]
/// Assemble a single [`Pointer`], avoiding cycles and repeating objects.
fn _assemble_val<T: EntityStore, R: Authorizer<T>>(
    value: Pointer,
    depth: u32,
    mut items: HashMap<String, Entity>,
    mut store: Option<T>,
    mut authorizer: R,
    mut seen: HashSet<String>,
) -> Result<
    (
        Option<T>,
        R,
        HashMap<String, Entity>,
        HashSet<String>,
        JValue,
    ),
    (T::Error, T),
> {
    match value {
        Pointer::Id(id) => {
            if seen.contains(&id) && !id.starts_with("_:") {
                let mut hash = JMap::new();
                hash.insert("@id".to_owned(), JValue::String(id));
                return Ok((store, authorizer, items, seen, JValue::Object(hash)));
            } else if items.contains_key(&id) {
                let item = items.remove(&id).unwrap();
                return await!(_assemble(item, depth + 1, items, store, authorizer, seen));
            }
            if depth < 5 && !id.starts_with("_:") {
                // todo: properly deserialize graphs
                store = if let Some(store) = store {
                    let (item, store) = await!(_get_collectionified(store, id.to_owned()))?;
                    if let Some(item) = item {
                        let (mut store, can_show) = await!(authorizer.can_show(store, &item))?;
                        if !can_show {
                            let mut hash = JMap::new();
                            hash.insert("@id".to_owned(), JValue::String(id));
                            return Ok((Some(store), authorizer, items, seen, JValue::Object(hash)));
                        }
                        seen.insert(id.to_owned());
                        if !item
                            .main()
                            .types
                            .contains(&as2!(OrderedCollection).to_string())
                        {
                            let (s, t, auth, o) =
                                await!(assemble(item, depth + 1, Some(store), authorizer, seen))?;
                            store = t.unwrap();
                            authorizer = auth;
                            seen = s;
                            return Ok((Some(store), authorizer, items, seen, o));
                        }

                        Some(store)
                    } else {
                        let mut hash = JMap::new();
                        hash.insert("@id".to_owned(), JValue::String(id));
                        return Ok((Some(store), authorizer, items, seen, JValue::Object(hash)));
                    }
                } else {
                    None
                }
            }

            {
                let mut hash = JMap::new();
                hash.insert("@id".to_owned(), JValue::String(id));
                Ok((store, authorizer, items, seen, JValue::Object(hash)))
            }
        }

        Pointer::Value(val) => Ok((
            store,
            authorizer,
            items,
            seen,
            Pointer::Value(val).to_json(),
        )),

        Pointer::List(list) => {
            let mut vals = Vec::new();
            for item in list {
                let (nstore, nauth, nitems, nseen, res) =
                    await!(_assemble_val(item, depth, items, store, authorizer, seen))?;
                seen = nseen;
                store = nstore;
                authorizer = nauth;
                items = nitems;
                vals.push(res);
            }

            let mut map = JMap::new();
            map.insert("@list".to_owned(), JValue::Array(vals));
            Ok((store, authorizer, items, seen, JValue::Object(map)))
        }
    }
}

#[async(boxed_send)]
fn _assemble<T: EntityStore, R: Authorizer<T>>(
    item: Entity,
    depth: u32,
    mut items: HashMap<String, Entity>,
    mut store: Option<T>,
    mut authorizer: R,
    mut seen: HashSet<String>,
) -> Result<
    (
        Option<T>,
        R,
        HashMap<String, Entity>,
        HashSet<String>,
        JValue,
    ),
    (T::Error, T),
> {
    let mut map = JMap::new();
    if !item.id.starts_with("_:") {
        seen.insert(item.id.to_owned());
        map.insert("@id".to_owned(), JValue::String(item.id.to_owned()));
    }

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
            let (nstore, nauth, nitems, nseen, res) = await!(_assemble_val(
                value,
                if AVOID_ASSEMBLE.contains(&(&key as &str)) {
                    999
                } else {
                    depth
                },
                items,
                store,
                authorizer,
                seen
            ))?;
            store = nstore;
            authorizer = nauth;
            items = nitems;
            seen = nseen;
            out.push(res);
        }

        map.insert(key, JValue::Array(out));
    }

    Ok((store, authorizer, items, seen, JValue::Object(map)))
}

#[async(boxed_send)]
/// Assembles a `StoreItem`, ensuring that no cycles happen.
pub fn assemble<T: EntityStore, R: Authorizer<T>>(
    mut item: StoreItem,
    depth: u32,
    store: Option<T>,
    authorizer: R,
    seen: HashSet<String>,
) -> Result<(HashSet<String>, Option<T>, R, JValue), (T::Error, T)> {
    let main = item.data.remove(&item.id).unwrap();

    let (nstore, authorizer, _, nseen, val) =
        await!(_assemble(main, depth, item.data, store, authorizer, seen))?;

    Ok((nseen, nstore, authorizer, val))
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
    let roots: Vec<_> = tangle_map
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
