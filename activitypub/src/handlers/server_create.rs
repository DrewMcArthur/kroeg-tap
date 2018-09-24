use jsonld::nodemap::{Entity, Pointer};

use kroeg_tap::{Context, EntityStore, MessageHandler};

use std::error::Error;
use std::fmt;

use futures::prelude::{await, *};

#[derive(Debug)]
pub enum ServerCreateError {
    FailedToRetrieve,
    ExistingPredicate(String),
    MissingRequired(String),
}

impl fmt::Display for ServerCreateError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ServerCreateError::MissingRequired(ref val) => write!(
                f,
                "The {} predicate is missing or occurs more than once",
                val
            ),
            ServerCreateError::FailedToRetrieve => write!(f, "Failed to retrieve object. Timeout?"),
            ServerCreateError::ExistingPredicate(ref val) => {
                write!(f, "The {} predicate should not have been passed", val)
            }
        }
    }
}

impl Error for ServerCreateError {
    fn cause(&self) -> Option<&Error> {
        None
    }
}

pub struct ServerCreateHandler;

fn _ensure(entity: &Entity, name: &str) -> Result<Pointer, Box<Error + Send + Sync + 'static>> {
    if entity[name].len() == 1 {
        Ok(entity[name][0].to_owned())
    } else {
        Err(Box::new(ServerCreateError::MissingRequired(
            name.to_owned(),
        )))
    }
}
impl<T: EntityStore + 'static> MessageHandler<T> for ServerCreateHandler {
    #[async(boxed_send)]
    fn handle(
        &self,
        context: Context,
        mut store: T,
        _inbox: String,
        elem: String,
    ) -> Result<(Context, T, String), Box<Error + Send + Sync + 'static>> {
        let root = elem.to_owned();

        let mut elem = match await!(store.get(elem, false)).map_err(Box::new)? {
            Some(val) => val,
            None => return Err(Box::new(ServerCreateError::FailedToRetrieve)),
        };

        if !elem.main().types.contains(&as2!(Create).to_owned()) {
            return Ok((context, store, root));
        }

        let elem = _ensure(elem.main(), as2!(object))?;
        let elem = if let Pointer::Id(id) = elem {
            id
        } else {
            return Err(Box::new(ServerCreateError::MissingRequired(
                as2!(object).to_owned(),
            )));
        };

        let mut elem = await!(store.get(elem, false)).map_err(Box::new)?.unwrap();

        for pointer in elem.main()[as2!(inReplyTo)].clone().into_iter() {
            if let Pointer::Id(id) = pointer {
                let item = await!(store.get(id, true)).map_err(Box::new)?;

                if let Some(item) = item {
                    if item.is_owned(&context) {
                        if let Some(Pointer::Id(replies)) =
                            item.main()[as2!(replies)].iter().next().cloned()
                        {
                            await!(store.insert_collection(replies, elem.id().to_owned()))
                                .map_err(Box::new)?;
                        }
                    }
                }
            }
        }

        Ok((context, store, root))
    }
}
