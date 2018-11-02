use jsonld::nodemap::Pointer;

use kroeg_tap::{box_store_error, Context, EntityStore, MessageHandler};

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

impl<T: EntityStore + 'static> MessageHandler<T> for ServerCreateHandler {
    #[async(boxed_send)]
    fn handle(
        &self,
        context: Context,
        store: T,
        _inbox: String,
        elem: String,
    ) -> Result<(Context, T, String), (Box<Error + Send + Sync + 'static>, T)> {
        let root = elem.to_owned();

        let (elem, store) = match await!(store.get(elem, false)).map_err(box_store_error)? {
            (Some(val), store) => (val, store),
            (None, store) => return Err((Box::new(ServerCreateError::FailedToRetrieve), store)),
        };

        if !elem.main().types.contains(&as2!(Create).to_owned()) {
            return Ok((context, store, root));
        }

        let elem = if elem.main()[as2!(object)].len() == 1 {
            elem.main()[as2!(object)][0].to_owned()
        } else {
            return Err((
                Box::new(ServerCreateError::MissingRequired(as2!(object).to_owned())),
                store,
            ));
        };

        let elem = if let Pointer::Id(id) = elem {
            id
        } else {
            return Err((
                Box::new(ServerCreateError::MissingRequired(as2!(object).to_owned())),
                store,
            ));
        };

        let (elem, mut store) = await!(store.get(elem, false)).map_err(box_store_error)?;
        let elem = elem.unwrap();

        for pointer in elem.main()[as2!(inReplyTo)].clone().into_iter() {
            if let Pointer::Id(id) = pointer {
                let (item, _store) = await!(store.get(id, true)).map_err(box_store_error)?;
                store = _store;

                if let Some(item) = item {
                    if item.is_owned(&context) {
                        if let Some(Pointer::Id(replies)) =
                            item.main()[as2!(replies)].iter().next().cloned()
                        {
                            store = await!(store.insert_collection(replies, elem.id().to_owned()))
                                .map_err(box_store_error)?;
                        }
                    }
                }
            }
        }

        Ok((context, store, root))
    }
}
