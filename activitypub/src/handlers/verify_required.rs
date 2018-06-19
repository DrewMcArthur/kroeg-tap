use kroeg_tap::{Context, EntityStore, MessageHandler, Pointer};

use std::error::Error;
use std::fmt;

use futures::prelude::*;

#[derive(Debug)]
pub enum RequiredEventsError<T>
where
    T: EntityStore,
{
    MissingObject,
    MissingActor,

    MayNotPublish,
    NotAllowedtoAct,
    ActorAttributedToDoNotMatch,
    EntityStoreError(T::Error),
}

impl<T> fmt::Display for RequiredEventsError<T>
where
    T: EntityStore,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            RequiredEventsError::MissingObject => {
                write!(f, "as:object predicate is missing or invalid")
            }
            RequiredEventsError::MissingActor => {
                write!(f, "as:actor predicate is missing or invalid")
            }
            RequiredEventsError::MayNotPublish => {
                write!(f, "current authorization token may not publish")
            }
            RequiredEventsError::NotAllowedtoAct => write!(
                f,
                "as:actor and subject in authorization token do not correspond"
            ),
            RequiredEventsError::ActorAttributedToDoNotMatch => {
                write!(f, "as:actor and as:attributedTo in the object do not match")
            }
            RequiredEventsError::EntityStoreError(ref err) => {
                write!(f, "failed to get value from the entity store: {}", err)
            }
        }
    }
}

impl<T> Error for RequiredEventsError<T>
where
    T: EntityStore,
{
    fn cause(&self) -> Option<&Error> {
        None
    }
}

pub struct VerifyRequiredEventsHandler;

fn equals_any_order(a: &Vec<Pointer>, b: &Vec<Pointer>) -> bool {
    if a.len() != b.len() {
        return false;
    }

    for item in a {
        if !b.contains(item) {
            return false;
        }
    }

    true
}

impl<T: EntityStore + 'static> MessageHandler<T> for VerifyRequiredEventsHandler {
    type Error = RequiredEventsError<T>;
    type Future = Box<Future<Item = (Context, T), Error = RequiredEventsError<T>> + Send>;

    #[async(boxed_send)]
    fn handle(
        self,
        context: Context,
        entitystore: T,
        _inbox: String,
        elem: String,
    ) -> Result<(Context, T), RequiredEventsError<T>> {
        let subject = context.user.subject.to_owned();

        let mut elem = await!(entitystore.get(elem))
            .map_err(|e| RequiredEventsError::EntityStoreError(e))?
            .expect("Missing the entity being handled, shouldn't happen");

        let actors = elem.main().get(as2!(actor)).clone();

        if actors.len() != 1 {
            return Err(RequiredEventsError::MissingActor);
        } else {
            match actors[0] {
                Pointer::Id(ref subj) => {
                    if subj != &subject {
                        return Err(RequiredEventsError::NotAllowedtoAct);
                    }
                }

                _ => return Err(RequiredEventsError::NotAllowedtoAct),
            }
        }

        let mut object = elem.main().get(as2!(object)).clone();

        if object.len() != 1 {
            return Err(RequiredEventsError::MissingObject);
        }

        match object.remove(0) {
            Pointer::Id(id) => {
                if let Some(entity) = await!(entitystore.get(id))
                    .map_err(|e| RequiredEventsError::EntityStoreError(e))?
                {
                    if !equals_any_order(
                        &entity.main()[as2!(attributedTo)],
                        &elem.main()[as2!(actor)],
                    ) {
                        Err(RequiredEventsError::ActorAttributedToDoNotMatch)
                    } else {
                        Ok((context, entitystore))
                    }
                } else {
                    Err(RequiredEventsError::MissingObject)
                }
            }

            _ => Err(RequiredEventsError::MissingObject),
        }
    }
}
