use futures::prelude::{await, *};

use serde_json::Value as JValue;

use super::entity::StoreItem;
use super::entitystore::EntityStore;
use super::user::Context;
use jsonld::nodemap::{Entity, Pointer, Value};

use rand::{thread_rng, Rng};
use std::collections::{BTreeMap, HashMap, HashSet};

const ALPHABET: [char; 32] = [
    'y', 'b', 'n', 'd', 'r', 'f', 'g', '8', 'e', 'j', 'k', 'm', 'c', 'p', 'q', 'x', 'o', 't', '1',
    'u', 'w', 'i', 's', 'z', 'a', '3', '4', '5', 'h', '7', '6', '9',
];

pub fn get_suggestion(depth: u32) -> String {
    let mut data: [u8; 8] = [0; 8];

    thread_rng().fill(&mut data);
    let data: String = data
        .iter()
        .map(|f| ALPHABET[(*f & 0b11111) as usize])
        .collect();

    if depth == 0 {
        format!("{}-{}", &data[..4], &data[4..])
    } else {
        format!("{}", &data[..4])
    }
}

const NAMES: [&'static str; 1] = [as2!(preferredUsername)];

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
    outgoing_ids: &mut HashSet<String>,
) {
    for val in arr {
        match val {
            Pointer::Id(ref mut id) => {
                if !id.starts_with(&context.server_base) && !id.starts_with("_:") {
                    continue;
                }

                outgoing_ids.insert(id.to_owned());

                if data.contains_key(id) {
                    *id = data[id].to_owned();
                }
            }

            Pointer::List(ref mut list) => _remap_arr(context, list, data, outgoing_ids),

            _ => {}
        }
    }
}

fn _remap(
    context: &Context,
    entity: &mut Entity,
    data: &HashMap<String, String>,
    outgoing_ids: &mut HashSet<String>,
) {
    for (_, mut value) in entity.iter_mut() {
        _remap_arr(context, value, data, outgoing_ids);
    }
}

#[async]
pub fn assign_ids<T: EntityStore>(
    mut context: Context,
    mut store: T,
    parent: Option<String>,
    data: HashMap<String, StoreItem>,
    root: Option<String>,
) -> Result<(Context, T, Option<String>, HashMap<String, StoreItem>), (T::Error, T)> {
    let mut out = HashMap::new();
    let mut remap = HashMap::new();

    let data = data.into_iter().collect::<BTreeMap<_, _>>(); // use the topologically assigned blank nodes.
    let root = root.or_else(|| data.iter().map(|(v, _)| v.to_owned()).next());

    let mut graph: HashMap<String, (Option<String>, u32)> = HashMap::new();

    for (id, mut value) in data {
        let (parent, depth) = graph.remove(&id).unwrap_or((parent.clone(), 0));
        let mut suggestion = shortname_suggestion(value.main());

        let new_id = loop {
            let (context_val, store_val, new_id) =
                await!(assign_id(context, store, suggestion, parent.clone(), depth))?;
            context = context_val;
            store = store_val;
            suggestion = None;

            // assign_id verifies that IDs do not conflict in the store, but these objects don't exist
            // in the store yet. Manually check them.
            if !out.contains_key(&new_id) {
                break new_id;
            }
        };

        let mut outgoing_ids = HashSet::new();
        _remap(
            &context,
            value.main_mut(),
            &HashMap::new(),
            &mut outgoing_ids,
        );

        for item in outgoing_ids {
            graph.insert(item, (Some(new_id.to_owned()), depth + 1));
        }

        let mut inner = value.data.remove(&value.id).unwrap();
        inner.id = new_id.to_owned();
        value.data.insert(new_id.to_owned(), inner);
        value.id = new_id.to_owned();

        value
            .meta()
            .get_mut(kroeg!(instance))
            .push(Pointer::Value(Value {
                value: JValue::Number(context.instance_id.into()),
                type_id: None,
                language: None,
            }));

        remap.insert(id, new_id.to_owned());
        out.insert(new_id, value);
    }

    for (_, ref mut value) in out.iter_mut() {
        _remap(&context, value.main_mut(), &remap, &mut HashSet::new());
    }

    Ok((
        context,
        store,
        root.and_then(|f| remap.get(&f).cloned()),
        out,
    ))
}

#[async]
pub fn assign_id<T: EntityStore>(
    context: Context,
    store: T,
    suggestion: Option<String>,
    parent: Option<String>,
    depth: u32,
) -> Result<(Context, T, String), (T::Error, T)> {
    let parent = parent.unwrap_or(context.server_base.to_owned());
    let suggestion = suggestion.unwrap_or(get_suggestion(depth));

    let preliminary = format!(
        "{}{}{}",
        parent,
        if parent.ends_with("/") { "" } else { "/" },
        suggestion
    );
    let (test, mut store) = await!(store.get(preliminary.to_owned(), false))?;
    if test.is_none() {
        return Ok((context, store, preliminary));
    }

    for _ in 1isize..3isize {
        let suggestion = get_suggestion(depth);
        let preliminary = format!(
            "{}{}{}",
            parent,
            if parent.ends_with("/") { "" } else { "/" },
            suggestion
        );
        let (test, storeval) = await!(store.get(preliminary.to_owned(), false))?;
        store = storeval;

        if test.is_none() {
            return Ok((context, store, preliminary));
        }
    }

    panic!("TODO: better handling of ID assignment");
}
