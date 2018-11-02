use jsonld::nodemap::{Entity, Pointer};

use kroeg_tap::{assign_id, box_store_error, Context, EntityStore, MessageHandler, StoreItem};

use std::error::Error;
use std::fmt;

use futures::prelude::{await, *};

#[derive(Debug)]
pub enum ClientCreateError {
    ExistingPredicate(String),
    MissingRequired(String),
}

impl fmt::Display for ClientCreateError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ClientCreateError::MissingRequired(ref val) => write!(
                f,
                "The {} predicate is missing or occurs more than once",
                val
            ),
            ClientCreateError::ExistingPredicate(ref val) => {
                write!(f, "The {} predicate should not have been passed", val)
            }
        }
    }
}

impl Error for ClientCreateError {
    fn cause(&self) -> Option<&Error> {
        None
    }
}

pub struct ClientCreateHandler;

fn _ensure<T: EntityStore + 'static>(
    store: T,
    entity: &Entity,
    name: &str,
) -> Result<(Pointer, T), (Box<Error + Send + Sync + 'static>, T)> {
    if entity[name].len() == 1 {
        Ok((entity[name][0].to_owned(), store))
    } else {
        Err((
            Box::new(ClientCreateError::MissingRequired(name.to_owned())),
            store,
        ))
    }
}

fn _set<T: EntityStore + 'static>(
    store: T,
    entity: &mut Entity,
    name: &str,
    val: Pointer,
) -> Result<T, (Box<Error + Send + Sync + 'static>, T)> {
    if entity[name].len() != 0 {
        Err((
            Box::new(ClientCreateError::ExistingPredicate(name.to_owned())),
            store,
        ))
    } else {
        entity.get_mut(name).push(val);
        Ok(store)
    }
}

impl<T: EntityStore + 'static> MessageHandler<T> for ClientCreateHandler {
    #[async(boxed_send)]
    fn handle(
        &self,
        mut context: Context,
        store: T,
        _inbox: String,
        elem: String,
    ) -> Result<(Context, T, String), (Box<Error + Send + Sync + 'static>, T)> {
        let root = elem.to_owned();

        let (elem, store) = await!(store.get(elem, false)).map_err(box_store_error)?;

        let mut elem = elem.expect("Missing the entity being handled, shouldn't happen");

        if !elem.main().types.contains(&as2!(Create).to_owned()) {
            return Ok((context, store, root));
        }

        let (elem, store) = _ensure(store, elem.main(), as2!(object))?;
        let elem = if let Pointer::Id(id) = elem {
            id
        } else {
            return Err((
                Box::new(ClientCreateError::MissingRequired(as2!(object).to_owned())),
                store,
            ));
        };

        let (elem, mut store) = await!(store.get(elem, false)).map_err(box_store_error)?;
        let mut elem = elem.unwrap();

        for itemname in &["likes", "shares", "replies"] {
            let (_context, _store, id) = await!(assign_id(
                context,
                store,
                Some(itemname.to_string()),
                Some(elem.id().to_owned()),
                1
            ))
            .map_err(box_store_error)?;

            context = _context;
            store = _store;

            let item = StoreItem::parse(
                &id,
                json!({
                "@id": id,
                "@type": [as2!(OrderedCollection)],
                as2!(partOf): [{"@id": elem.id()}]
            }),
            )
            .unwrap();

            let (_, _store) = await!(store.put(id.to_owned(), item)).map_err(box_store_error)?;
            store = _store;

            store = _set(
                store,
                elem.main_mut(),
                &format!("https://www.w3.org/ns/activitystreams#{}", itemname),
                Pointer::Id(id),
            )?;
        }

        let (_, store) = await!(store.put(elem.id().to_owned(), elem)).map_err(box_store_error)?;

        Ok((context, store, root))
    }
}
