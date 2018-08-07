use futures::prelude::{await, *};

use serde_json::Value as JValue;

use super::entity::StoreItem;
use super::entitystore::EntityStore;
use super::user::Context;
use jsonld::nodemap::{Entity, Pointer, Value};

use rand::{thread_rng, Rng};
use std::collections::{HashMap, HashSet};
use std::fmt::Write;

pub fn get_suggestion() -> String {
    let mut result = String::new();
    let mut data: [u8; 5] = [0; 5];

    thread_rng().fill(&mut data);
    for byte in data.iter() {
        write!(&mut result, "{:x}", byte).unwrap();
    }

    result
}

const NAMES: [&'static str; 3] = [
    as2!(preferredUsername),
    as2!(name),
    as2!(summary),
    //    as2!(content),
];

fn translate_name(predicate: &str, nam: &str) -> String {
    let mut result = String::new();

    if predicate == as2!(preferredUsername) {
        result += "~";
    }

    for ch in nam.chars().take(15) {
        if ch.is_alphanumeric() {
            result += &ch.to_lowercase().collect::<String>();
        } else {
            result += "-";
        }
    }

    result
}

/// Generates a suggestion for a short name in the URL of an entity.
pub fn shortname_suggestion(main: &Entity) -> Option<String> {
    for name in NAMES.iter() {
        if main[name].len() > 0 {
            let first = &main[name][0];
            if let Pointer::Value(ref val) = first {
                if let JValue::String(ref string) = val.value {
                    return Some(translate_name(name, string));
                }
            }
        }
    }

    if main.types.len() > 0 && main[as2!(actor)].len() == 0 {
        return Some(translate_name(
            "@type",
            main.types[0].split('#').last().unwrap(),
        ));
    }

    None
}

fn _remap_arr(
    context: &Context,
    arr: &mut Vec<Pointer>,
    data: &HashMap<String, String>,
    found: &mut HashSet<String>,
) {
    for val in arr {
        match val {
            Pointer::Id(ref mut id) => {
                if !id.starts_with(&context.server_base) && !id.starts_with("_:") {
                    continue;
                }

                found.insert(id.to_owned());

                if data.contains_key(id) {
                    *id = data[id].to_owned();
                }
            }

            Pointer::List(ref mut list) => _remap_arr(context, list, data, found),

            _ => {}
        }
    }
}

fn _remap(
    context: &Context,
    entity: &mut Entity,
    data: &HashMap<String, String>,
    found: &mut HashSet<String>,
) {
    for (_, mut value) in entity.iter_mut() {
        _remap_arr(context, value, data, found);
    }
}

#[async]
pub fn assign_ids<T: EntityStore>(
    mut context: Context,
    mut store: T,
    parent: Option<String>,
    data: HashMap<String, StoreItem>,
) -> Result<(Context, T, Vec<String>, HashMap<String, StoreItem>), T::Error> {
    let mut out = HashMap::new();
    let mut remap = HashMap::new();
    let roots: Vec<_> = data.keys().map(|f| f.to_owned()).collect();
    for (_, mut value) in data {
        let mut to_run = HashSet::new();
        let mut to_do: Vec<(Option<String>, String)> = Vec::new();
        to_do.push((parent.to_owned(), value.id.to_owned()));

        while to_do.len() > 0 {
            let (parent, id) = to_do.remove(0);
            if let Some(mut newitem) = value.remove(&id) {
                if newitem.iter().next().is_some() {
                    let suggestion = shortname_suggestion(&newitem);
                    let (c, s, r) = await!(assign_id(context, store, suggestion, parent))?;
                    context = c;
                    store = s;

                    let mut found = HashSet::new();
                    remap.insert(newitem.id.to_owned(), r.to_owned());
                    _remap(&context, &mut newitem, &HashMap::new(), &mut found);

                    newitem.id = r.to_owned();

                    for kv in found {
                        if !to_run.contains(&kv) {
                            to_run.insert(kv.to_owned());
                            to_do.push((Some(r.to_owned()), kv));
                        }
                    }

                    let mut minimap = HashMap::new();
                    minimap.insert(r.to_owned(), newitem);
                    let mut item = StoreItem::new(r.to_owned(), minimap);
                    item.meta()
                        .get_mut(kroeg!(instance))
                        .push(Pointer::Value(Value {
                            value: JValue::Number(context.instance_id.into()),
                            type_id: None,
                            language: None,
                        }));
                    out.insert(r, item);
                }
            }
        }
    }

    for (_, ref mut value) in out.iter_mut() {
        _remap(&context, value.main_mut(), &remap, &mut HashSet::new());
    }

    Ok((
        context,
        store,
        roots.into_iter().map(|f| remap[&f].to_owned()).collect(),
        out,
    ))
}

#[async]
pub fn assign_id<T: EntityStore>(
    context: Context,
    store: T,
    suggestion: Option<String>,
    parent: Option<String>,
) -> Result<(Context, T, String), T::Error> {
    let parent = parent.unwrap_or(context.server_base.to_owned());
    let suggestion = suggestion.unwrap_or(get_suggestion());

    let preliminary = format!(
        "{}{}{}",
        parent,
        if parent.ends_with("/") { "" } else { "/" },
        suggestion
    );
    let test = await!(store.get(preliminary.to_owned()))?;
    if test.is_none() {
        return Ok((context, store, preliminary));
    }

    for _ in 1isize..3isize {
        let suggestion = get_suggestion();
        let preliminary = format!(
            "{}{}{}",
            parent,
            if parent.ends_with("/") { "" } else { "/" },
            suggestion
        );
        let test = await!(store.get(preliminary.to_owned()))?;
        if test.is_none() {
            return Ok((context, store, preliminary));
        }
    }

    panic!("TODO: better handling of ID assignment");
}
