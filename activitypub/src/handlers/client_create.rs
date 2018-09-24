use jsonld::nodemap::{Entity, Pointer};

use kroeg_tap::{assign_id, Context, EntityStore, MessageHandler, StoreItem};

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

fn _ensure(entity: &Entity, name: &str) -> Result<Pointer, Box<Error + Send + Sync + 'static>> {
    if entity[name].len() == 1 {
        Ok(entity[name][0].to_owned())
    } else {
        Err(Box::new(ClientCreateError::MissingRequired(
            name.to_owned(),
        )))
    }
}

fn _set(
    entity: &mut Entity,
    name: &str,
    val: Pointer,
) -> Result<(), Box<Error + Send + Sync + 'static>> {
    if entity[name].len() != 0 {
        Err(Box::new(ClientCreateError::ExistingPredicate(
            name.to_owned(),
        )))
    } else {
        entity.get_mut(name).push(val);
        Ok(())
    }
}

impl<T: EntityStore + 'static> MessageHandler<T> for ClientCreateHandler {
    #[async(boxed_send)]
    fn handle(
        &self,
        mut context: Context,
        mut store: T,
        _inbox: String,
        elem: String,
    ) -> Result<(Context, T, String), Box<Error + Send + Sync + 'static>> {
        let root = elem.to_owned();

        let mut elem = await!(store.get(elem, false))
            .map_err(Box::new)?
            .expect("Missing the entity being handled, shouldn't happen");

        if !elem.main().types.contains(&as2!(Create).to_owned()) {
            return Ok((context, store, root));
        }

        let elem = _ensure(elem.main(), as2!(object))?;
        let elem = if let Pointer::Id(id) = elem {
            id
        } else {
            return Err(Box::new(ClientCreateError::MissingRequired(
                as2!(object).to_owned(),
            )));
        };

        let mut elem = await!(store.get(elem, false)).map_err(Box::new)?.unwrap();

        for itemname in &["likes", "shares", "replies"] {
            let (_context, _store, id) = await!(assign_id(
                context,
                store,
                Some(itemname.to_string()),
                Some(elem.id().to_owned())
            )).map_err(Box::new)?;

            context = _context;
            store = _store;

            let item = StoreItem::parse(
                &id,
                json!({
                "@id": id,
                "@type": [as2!(OrderedCollection)],
                as2!(partOf): [{"@id": elem.id()}]
            }),
            ).unwrap();

            await!(store.put(id.to_owned(), item)).map_err(Box::new)?;

            _set(
                elem.main_mut(),
                &format!("https://www.w3.org/ns/activitystreams#{}", itemname),
                Pointer::Id(id),
            )?;
        }

        await!(store.put(elem.id().to_owned(), elem)).map_err(Box::new)?;

        Ok((context, store, root))
    }
}
