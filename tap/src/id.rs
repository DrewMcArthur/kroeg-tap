use futures::prelude::*;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use super::entity::StoreItem;
use jsonld::nodemap::{Entity, Pointer};
use super::entitystore::EntityStore;
use super::user::Context;

use std::collections::{HashMap, HashSet};

pub fn get_timestamp() -> String {
    let duration = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();

    format!(
        "{}",
        duration.as_secs() * 1000 + (duration.subsec_nanos() / 1000) as u64
    )
}

const NAMES: [&'static str; 3] = [as2!(preferredUsername), as2!(name), as2!(summary)];

fn translate_name(nam: &str) -> String {
    let mut result = String::new();

    for ch in nam.chars() {
        if ch.is_alphanumeric() {
            result += &ch.to_lowercase().collect::<String>();
        } else {
            result += "-";
        }
    }

    result
}

/// Generates a suggestion for a short name in the URL of an entity.
pub fn shortname_suggestion(object: &StoreItem) -> Option<String> {
    let main = object.main();

    for name in NAMES.iter() {
        if main[name].len() > 0 {
            let first = &main[name][0];
            if let Pointer::Value(ref val) = first {
                if let Value::String(ref string) = val.value {
                    return Some(translate_name(string));
                }
            }
        }
    }

    None
}

fn _remap_arr(arr: &mut Vec<Pointer>, data: &HashMap<String, String>, found: &mut HashSet<String>) {
    for val in arr {
        match val {
            Pointer::Id(ref mut id) => {
                found.insert(id.to_owned());

                if data.contains_key(id) {
                    *id = data[id].to_owned();
                }
            },

            Pointer::List(ref mut list) => _remap_arr(list, data, found),

            _ => {}
        }
    }
}

fn _remap(entity: &mut Entity, data: &HashMap<String, String>, found: &mut HashSet<String>) {
    for (_, mut value) in entity.iter_mut() {
        _remap_arr(value, data, found);
    }
}

#[async]
pub fn assign_ids<T: EntityStore>(
    mut context: Context,
    mut store: T,
    parent: Option<String>,
    data: HashMap<String, StoreItem>
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
            
            let mut newitem = value.remove(&id).unwrap();
            let (c, s, r) = await!(assign_id(context, store, None, parent))?;
            let mut found = HashSet::new();
            remap.insert(newitem.id.to_owned(), r.to_owned());
            _remap(&mut newitem, &HashMap::new(), &mut found);

            context = c;
            store = s;
            newitem.id = r.to_owned();

            for kv in found {
                if !to_run.contains(&kv) {
                    to_run.insert(kv.to_owned());
                    to_do.push((Some(r.to_owned()), kv));
                }
            }

            let mut minimap = HashMap::new();
            minimap.insert(r.to_owned(), newitem);
            let id = r.to_owned();
            out.insert(id, StoreItem::new(r.to_owned(), minimap));
        }
    }

    for (_, ref mut value) in out.iter_mut() {
        _remap(value.main_mut(), &remap, &mut HashSet::new());
    }

    Ok((context, store, roots.into_iter().map(|f| remap[&f].to_owned()).collect(), out))
}

#[async]
pub fn assign_id<T: EntityStore>(
    context: Context,
    store: T,
    suggestion: Option<String>,
    parent: Option<String>,
) -> Result<(Context, T, String), T::Error> {
    let parent = parent.unwrap_or(context.server_base.to_owned());
    let suggestion = suggestion.unwrap_or(get_timestamp());

    let preliminary = format!("{}/{}", parent, suggestion);
    let test = await!(store.get(preliminary.to_owned()))?;
    if test.is_none() {
        return Ok((context, store, preliminary));
    }

    for _ in 1isize..3isize {
        let suggestion = get_timestamp();
        let preliminary = format!("{}/{}", parent, suggestion);
        let test = await!(store.get(preliminary.to_owned()))?;
        if test.is_none() {
            return Ok((context, store, preliminary));
        }
    }

    panic!("TODO: better handling of ID assignment");
}
