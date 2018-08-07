use jsonld::nodemap::Pointer;
use kroeg_tap::{Context, EntityStore, MessageHandler};

use std::error::Error;
use std::fmt;

use futures::prelude::{await, *};

#[derive(Debug)]
pub enum RequiredEventsError<T>
where
    T: EntityStore,
{
    FailedToRetrieve,
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
            RequiredEventsError::FailedToRetrieve => {
                write!(f, "Failed to retrieve object. Timeout?")
            }
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

const APPLIES_TO_TYPES: [&'static str; 3] = [as2!(Create), as2!(Update), as2!(Delete)];

impl<T: EntityStore + 'static> MessageHandler<T> for VerifyRequiredEventsHandler {
    type Error = RequiredEventsError<T>;
    type Future = Box<Future<Item = (Context, T, String), Error = RequiredEventsError<T>> + Send>;

    #[async(boxed_send)]
    fn handle(
        self,
        context: Context,
        entitystore: T,
        _inbox: String,
        elem: String,
    ) -> Result<(Context, T, String), RequiredEventsError<T>> {
        let subject = context.user.subject.to_owned();

        let mut elem = match await!(entitystore.get(elem, false))
            .map_err(|e| RequiredEventsError::EntityStoreError(e))?
        {
            Some(val) => val,
            None => return Err(RequiredEventsError::FailedToRetrieve),
        };

        if !elem
            .main()
            .types
            .iter()
            .any(|f| APPLIES_TO_TYPES.contains(&(&*f as &str)))
        {
            return Ok((context, entitystore, elem.id().to_owned()));
        }

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
                if let Some(entity) = await!(entitystore.get(id, false))
                    .map_err(|e| RequiredEventsError::EntityStoreError(e))?
                {
                    if !equals_any_order(
                        &entity.main()[as2!(attributedTo)],
                        &elem.main()[as2!(actor)],
                    )
                        && !elem.main().types.contains(&String::from(as2!(Update)))
                    {
                        Err(RequiredEventsError::ActorAttributedToDoNotMatch)
                    } else {
                        Ok((context, entitystore, elem.id().to_owned()))
                    }
                } else {
                    Err(RequiredEventsError::MissingObject)
                }
            }

            _ => Err(RequiredEventsError::MissingObject),
        }
    }
}
