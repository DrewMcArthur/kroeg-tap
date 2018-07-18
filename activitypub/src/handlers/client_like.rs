use jsonld::nodemap::{Entity, Pointer};

use kroeg_tap::{Context, EntityStore, MessageHandler};

use std::error::Error;
use std::fmt;

use futures::prelude::*;

#[derive(Debug)]
pub enum ClientLikeError<T>
where
    T: EntityStore,
{
    MissingRequired(String),
    EntityStoreError(T::Error),
}

impl<T> fmt::Display for ClientLikeError<T>
where
    T: EntityStore,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ClientLikeError::MissingRequired(ref val) => write!(
                f,
                "The {} predicate is missing or occurs more than once",
                val
            ),
            ClientLikeError::EntityStoreError(ref err) => {
                write!(f, "failed to get value from the entity store: {}", err)
            }
        }
    }
}

impl<T> Error for ClientLikeError<T>
where
    T: EntityStore,
{
    fn cause(&self) -> Option<&Error> {
        None
    }
}

fn _ensure<T: EntityStore + 'static>(
    entity: &Entity,
    name: &str,
) -> Result<Pointer, ClientLikeError<T>> {
    if entity[name].len() == 1 {
        Ok(entity[name][0].to_owned())
    } else {
        Err(ClientLikeError::MissingRequired(name.to_owned()))
    }
}

pub struct ClientLikeHandler;

impl<T: EntityStore + 'static> MessageHandler<T> for ClientLikeHandler {
    type Error = ClientLikeError<T>;
    type Future = Box<Future<Item = (Context, T, String), Error = ClientLikeError<T>> + Send>;

    #[async(boxed_send)]
    fn handle(
        self,
        context: Context,
        mut store: T,
        _inbox: String,
        elem: String,
    ) -> Result<(Context, T, String), ClientLikeError<T>> {
        let subject = context.user.subject.to_owned();
        let root = elem.to_owned();

        let mut elem = await!(store.get(elem))
            .map_err(|e| ClientLikeError::EntityStoreError(e))?
            .expect("Missing the entity being handled, shouldn't happen");

        if !elem.main().types.contains(&as2!(Like).to_owned()) {
            return Ok((context, store, root));
        }

        let elem = _ensure(elem.main(), as2!(object))?;
        let elem = if let Pointer::Id(id) = elem {
            id
        } else {
            return Err(ClientLikeError::MissingRequired(as2!(object).to_owned()));
        };

        let mut elem = await!(store.get(elem.to_owned()))
            .map_err(ClientLikeError::EntityStoreError)?
            .unwrap();

        let mut subj = await!(store.get(subject))
            .map_err(ClientLikeError::EntityStoreError)?
            .unwrap();

        if let Some(Pointer::Id(liked)) = subj.main()[as2!(liked)].iter().next().cloned() {
            await!(store.insert_collection(liked.to_owned(), elem.id().to_owned()))
                .map_err(ClientLikeError::EntityStoreError)?
        }

        Ok((context, store, root))
    }
}
