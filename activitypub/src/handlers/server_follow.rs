use jsonld::nodemap::{Entity, Pointer};

use kroeg_tap::{Context, EntityStore, MessageHandler};

use std::error::Error;
use std::fmt;

use futures::prelude::*;

#[derive(Debug)]
pub enum ServerFollowError<T>
where
    T: EntityStore,
{
    MissingRequired(String),
    EntityStoreError(T::Error),
}

impl<T> fmt::Display for ServerFollowError<T>
where
    T: EntityStore,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ServerFollowError::MissingRequired(ref val) => write!(
                f,
                "The {} predicate is missing or occurs more than once",
                val
            ),
            ServerFollowError::EntityStoreError(ref err) => {
                write!(f, "failed to get value from the entity store: {}", err)
            }
        }
    }
}

impl<T> Error for ServerFollowError<T>
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
) -> Result<Pointer, ServerFollowError<T>> {
    if entity[name].len() == 1 {
        Ok(entity[name][0].to_owned())
    } else {
        Err(ServerFollowError::MissingRequired(name.to_owned()))
    }
}

pub struct ServerFollowHandler;

impl<T: EntityStore + 'static> MessageHandler<T> for ServerFollowHandler {
    type Error = ServerFollowError<T>;
    type Future = Box<Future<Item = (Context, T, String), Error = ServerFollowError<T>> + Send>;

    #[async(boxed_send)]
    fn handle(
        self,
        context: Context,
        mut store: T,
        _inbox: String,
        elem: String,
    ) -> Result<(Context, T, String), ServerFollowError<T>> {
        let root = elem.to_owned();

        let mut relem = await!(store.get(elem))
            .map_err(ServerFollowError::EntityStoreError)?
            .expect("Missing the entity being handled, shouldn't happen");

        let is_accept = relem.main().types.contains(&as2!(Accept).to_owned());
        let is_reject = relem.main().types.contains(&as2!(Reject).to_owned());

        if !is_accept && !is_reject {
            return Ok((context, store, root));
        }

        // for every object that is accepted or rejected,
        for obj in relem.main()[as2!(object)].clone() {
            if let Pointer::Id(id) = obj {
                if let Some(mut elem) =
                    await!(store.get(id.to_owned())).map_err(ServerFollowError::EntityStoreError)?
                {
                    // if it's not one of our follow requests, ignore
                    if !elem.is_owned(&context) {
                        continue;
                    }

                    if elem.main()[as2!(object)] != relem.main()[as2!(actor)] {
                        // the follow request has to be accepted/rejected by the actor.
                        panic!("invalid response TODO proper error");
                    }

                    // if it hasn't been rejected(! stored in meta)
                    if elem.meta()[as2!(Reject)].len() == 0 {
                        let mut changed = false;
                        // if this is a reject
                        if is_reject {
                            // store reject
                            elem.meta()[as2!(Reject)].push(Pointer::Id(root.to_owned()));
                            // get user of the follow request
                            let user = if let Pointer::Id(id) = _ensure(elem.main(), as2!(object))?
                            {
                                id
                            } else {
                                return Err(ServerFollowError::MissingRequired(
                                    as2!(object).to_owned(),
                                ));
                            };

                            let user = await!(store.get(user))
                                .map_err(ServerFollowError::EntityStoreError)?
                                .unwrap();
                            let following =
                                if let Pointer::Id(id) = _ensure(user.main(), as2!(following))? {
                                    id
                                } else {
                                    return Err(ServerFollowError::MissingRequired(
                                        as2!(following).to_owned(),
                                    ));
                                };

                            await!(store.insert_collection(following, root.to_owned()))
                                .map_err(ServerFollowError::EntityStoreError)?;
                            changed = true;
                        }

                        // if this is an unaccepted follow
                        if !changed && elem.meta()[as2!(Accept)].len() == 0 && is_accept {
                            elem.meta()[as2!(Accept)].push(Pointer::Id(root.to_owned()));
                            // get user of the follow request
                            let user = if let Pointer::Id(id) = _ensure(elem.main(), as2!(object))?
                            {
                                id
                            } else {
                                return Err(ServerFollowError::MissingRequired(
                                    as2!(object).to_owned(),
                                ));
                            };

                            let user = await!(store.get(user))
                                .map_err(ServerFollowError::EntityStoreError)?
                                .unwrap();
                            let following =
                                if let Pointer::Id(id) = _ensure(user.main(), as2!(following))? {
                                    id
                                } else {
                                    return Err(ServerFollowError::MissingRequired(
                                        as2!(following).to_owned(),
                                    ));
                                };

                            await!(store.insert_collection(following, root.to_owned()))
                                .map_err(ServerFollowError::EntityStoreError)?;

                            changed = true;
                        }

                        if changed {
                            await!(store.put(id.to_owned(), elem))
                                .map_err(ServerFollowError::EntityStoreError)?;
                        }
                    } else {
                        // already rejected, ignore
                        continue;
                    }
                }
            }
        }

        Ok((context, store, root))
    }
}
