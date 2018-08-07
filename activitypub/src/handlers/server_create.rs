use jsonld::nodemap::{Entity, Pointer};

use kroeg_tap::{Context, EntityStore, MessageHandler};

use std::error::Error;
use std::fmt;

use futures::prelude::{*, await};

#[derive(Debug)]
pub enum ServerCreateError<T>
where
    T: EntityStore,
{
    ExistingPredicate(String),
    MissingRequired(String),
    EntityStoreError(T::Error),
}

impl<T> fmt::Display for ServerCreateError<T>
where
    T: EntityStore,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ServerCreateError::MissingRequired(ref val) => write!(
                f,
                "The {} predicate is missing or occurs more than once",
                val
            ),
            ServerCreateError::ExistingPredicate(ref val) => {
                write!(f, "The {} predicate should not have been passed", val)
            }
            ServerCreateError::EntityStoreError(ref err) => {
                write!(f, "failed to get value from the entity store: {}", err)
            }
        }
    }
}

impl<T> Error for ServerCreateError<T>
where
    T: EntityStore,
{
    fn cause(&self) -> Option<&Error> {
        None
    }
}

pub struct ServerCreateHandler;

fn _ensure<T: EntityStore + 'static>(
    entity: &Entity,
    name: &str,
) -> Result<Pointer, ServerCreateError<T>> {
    if entity[name].len() == 1 {
        Ok(entity[name][0].to_owned())
    } else {
        Err(ServerCreateError::MissingRequired(name.to_owned()))
    }
}
impl<T: EntityStore + 'static> MessageHandler<T> for ServerCreateHandler {
    type Error = ServerCreateError<T>;
    type Future = Box<Future<Item = (Context, T, String), Error = ServerCreateError<T>> + Send>;

    #[async(boxed_send)]
    fn handle(
        self,
        context: Context,
        mut store: T,
        _inbox: String,
        elem: String,
    ) -> Result<(Context, T, String), ServerCreateError<T>> {
        let root = elem.to_owned();

        let mut elem = await!(store.get(elem))
            .map_err(|e| ServerCreateError::EntityStoreError(e))?
            .expect("Missing the entity being handled, shouldn't happen");

        if !elem.main().types.contains(&as2!(Create).to_owned()) {
            return Ok((context, store, root));
        }

        let elem = _ensure(elem.main(), as2!(object))?;
        let elem = if let Pointer::Id(id) = elem {
            id
        } else {
            return Err(ServerCreateError::MissingRequired(as2!(object).to_owned()));
        };

        let mut elem = await!(store.get(elem))
            .map_err(ServerCreateError::EntityStoreError)?
            .unwrap();

        for pointer in elem.main()[as2!(inReplyTo)].clone().into_iter() {
            if let Pointer::Id(id) = pointer {
                let item = await!(store.get(id)).map_err(ServerCreateError::EntityStoreError)?;

                if let Some(item) = item {
                    if item.is_owned(&context) {
                        if let Some(Pointer::Id(replies)) =
                            item.main()[as2!(replies)].iter().next().cloned()
                        {
                            await!(store.insert_collection(replies, elem.id().to_owned()))
                                .map_err(ServerCreateError::EntityStoreError)?;
                        }
                    }
                }
            }
        }

        Ok((context, store, root))
    }
}
