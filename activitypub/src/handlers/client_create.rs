use jsonld::nodemap::{Entity, Pointer};

use kroeg_tap::{assign_id, Context, EntityStore, MessageHandler, StoreItem};

use std::error::Error;
use std::fmt;

use futures::prelude::*;

#[derive(Debug)]
pub enum ClientCreateError<T>
where
    T: EntityStore,
{
    ExistingPredicate(String),
    MissingRequired(String),
    EntityStoreError(T::Error),
}

impl<T> fmt::Display for ClientCreateError<T>
where
    T: EntityStore,
{
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
            ClientCreateError::EntityStoreError(ref err) => {
                write!(f, "failed to get value from the entity store: {}", err)
            }
        }
    }
}

impl<T> Error for ClientCreateError<T>
where
    T: EntityStore,
{
    fn cause(&self) -> Option<&Error> {
        None
    }
}

pub struct ClientCreateHandler;

fn _ensure<T: EntityStore + 'static>(
    entity: &Entity,
    name: &str,
) -> Result<Pointer, ClientCreateError<T>> {
    if entity[name].len() == 1 {
        Ok(entity[name][0].to_owned())
    } else {
        Err(ClientCreateError::MissingRequired(name.to_owned()))
    }
}

fn _set<T: EntityStore + 'static>(
    entity: &mut Entity,
    name: &str,
    val: Pointer,
) -> Result<(), ClientCreateError<T>> {
    if entity[name].len() != 0 {
        Err(ClientCreateError::ExistingPredicate(name.to_owned()))
    } else {
        entity.get_mut(name).push(val);
        Ok(())
    }
}

impl<T: EntityStore + 'static> MessageHandler<T> for ClientCreateHandler {
    type Error = ClientCreateError<T>;
    type Future = Box<Future<Item = (Context, T, String), Error = ClientCreateError<T>> + Send>;

    #[async(boxed_send)]
    fn handle(
        self,
        mut context: Context,
        mut store: T,
        _inbox: String,
        elem: String,
    ) -> Result<(Context, T, String), ClientCreateError<T>> {
        let subject = context.user.subject.to_owned();
        let root = elem.to_owned();

        let mut elem = await!(store.get(elem))
            .map_err(|e| ClientCreateError::EntityStoreError(e))?
            .expect("Missing the entity being handled, shouldn't happen");

        if !elem.main().types.contains(&as2!(Create).to_owned()) {
            return Ok((context, store, root));
        }

        let elem = _ensure(elem.main(), as2!(object))?;
        let elem = if let Pointer::Id(id) = elem {
            id
        } else {
            return Err(ClientCreateError::MissingRequired(as2!(object).to_owned()));
        };

        let mut elem = await!(store.get(elem))
            .map_err(ClientCreateError::EntityStoreError)?
            .unwrap();

        for itemname in &["likes", "shares", "replies"] {
            let (_context, _store, id) = await!(assign_id(
                context,
                store,
                Some(itemname.to_string()),
                Some(elem.id().to_owned())
            )).map_err(ClientCreateError::EntityStoreError)?;

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

            await!(store.put(id.to_owned(), item)).map_err(ClientCreateError::EntityStoreError)?;

            _set(
                elem.main_mut(),
                &format!("https://www.w3.org/ns/activitystreams#{}", itemname),
                Pointer::Id(id),
            )?;
        }

        await!(store.put(elem.id().to_owned(), elem)).map_err(ClientCreateError::EntityStoreError)?;

        Ok((context, store, root))
    }
}
