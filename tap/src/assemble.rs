use entity::StoreItem;

use std::collections::{HashMap, HashSet};

use serde_json::Map as JMap;
use serde_json::Value as JValue;

use entitystore::EntityStore;
use futures::prelude::{await, *};

use auth::Authorizer;
use id::get_suggestion;

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

            let (data, store) =
                await!(store.read_collection(without_query.to_owned(), None, None))?;

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
                    )
                    .expect("static input cannot fail"),
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
            if seen.contains(&id) {
                let mut hash = JMap::new();
                hash.insert("@id".to_owned(), JValue::String(id));
                return Ok((store, authorizer, items, seen, JValue::Object(hash)));
            } else if items.contains_key(&id) {
                let item = items.remove(&id).unwrap();
                return await!(_assemble(item, depth + 1, items, store, authorizer, seen));
            }
            if depth < 5 {
                // todo: properly deserialize graphs
                store = if let Some(store) = store {
                    let (item, store) = await!(_get_collectionified(store, id.to_owned()))?;
                    if let Some(item) = item {
                        let (mut store, can_show) = if item.id().starts_with("_:") {
                            (store, true)
                        } else {
                            await!(authorizer.can_show(store, &item))?
                        };

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
                            let is_blank = item.id().starts_with("_:");

                            let (s, t, auth, o) = await!(assemble(
                                item,
                                if is_blank { depth } else { depth + 1 },
                                Some(store),
                                authorizer,
                                seen
                            ))?;
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

fn _rename_vec(map: &HashMap<&String, String>, data: &mut Vec<Pointer>) {
    for value in data {
        match value {
            Pointer::Id(ref mut id) => {
                if map.contains_key(id) {
                    *id = map[id].to_owned();
                }
            }

            Pointer::List(ref mut list) => _rename_vec(map, list),
            _ => {}
        };
    }
}

fn find_non_blank<'a>(
    map: &'a HashMap<String, Vec<String>>,
    item: &'a String,
    edges: &mut HashSet<(&'a String, &'a String)>,
) -> Option<&'a String> {
    for (i, val) in map {
        if edges.contains(&(i, item)) {
            continue;
        }

        edges.insert((i, item));

        if val.contains(item) {
            if !i.starts_with("_:") {
                return Some(i);
            }

            if let Some(item) = find_non_blank(map, i, edges) {
                return Some(item);
            }
        }
    }

    None
}

pub fn untangle(data: JValue) -> Result<HashMap<String, StoreItem>, NodeMapError> {
    // Build a node map, aka a flattened json-ld graph.
    let mut flattened =
        match generate_node_map(data, &mut DefaultNodeGenerator::new())?.remove("@default") {
            Some(val) => val,
            None => return Ok(HashMap::new()),
        };

    // Remove all objects where there's no actual data inside, aka references to other objects.
    flattened.retain(|_, v| v.iter().next().is_some());

    let mut values = Vec::new();

    let mut tangle_map = HashMap::new();
    for (key, item) in flattened.iter() {
        // Build a Vec<String> of all the ID values inside this object
        let mut tangles = Vec::new();
        for (_, values) in item.iter() {
            _untangle_vec(values, &mut tangles);
        }

        for value in &tangles {
            // Note down graph edge for naming.
            values.push((key.to_owned(), value.to_owned()));
        }

        tangle_map.insert(key.to_owned(), tangles);
    }

    let mut rewrite_id = HashMap::new();

    for (id, _) in &tangle_map {
        if !id.starts_with("_:") || rewrite_id.contains_key(&id) {
            continue;
        }

        if let Some(value) = find_non_blank(&tangle_map, id, &mut HashSet::new()) {
            // woo! this object is rooted in another object! record it as _:https://example.com/object:b1

            let i = rewrite_id.len();
            rewrite_id.insert(id, format!("_:{}:b{}", value, i));
        } else {
            // _:unrooted:1234-5678-1234-5678-1234-5678:b0
            // should be random enough??? maybe pass in outside context (e.g. id??)
            // eh whatever.

            let i = rewrite_id.len();
            rewrite_id.insert(
                id,
                format!(
                    "_:unrooted-{}-{}-{}:b{}",
                    get_suggestion(0),
                    get_suggestion(0),
                    get_suggestion(0),
                    i
                ),
            );
        }
    }

    for (key, item) in &mut flattened {
        for (_, values) in item.iter_mut() {
            _rename_vec(&rewrite_id, values);
        }

        if rewrite_id.contains_key(&item.id) {
            item.id = rewrite_id[&item.id].to_owned();
        }
    }

    Ok(flattened
        .into_iter()
        .map(|(k, v)| {
            let mut map = HashMap::new();
            let k = if rewrite_id.contains_key(&k) {
                rewrite_id[&k].to_owned()
            } else {
                k
            };

            map.insert(k.to_owned(), v);
            (k.to_owned(), StoreItem::new(k, map))
        })
        .collect())
}
