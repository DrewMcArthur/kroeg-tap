use jsonld::nodemap::Pointer;
use kroeg_tap::{Context, EntityStore, MessageHandler};

use std::error::Error;
use std::fmt;

use futures::prelude::{await, *};

#[derive(Debug)]
pub enum RequiredEventsError {
    FailedToRetrieve,
    MissingObject,
    MissingActor,

    MayNotPublish,
    NotAllowedtoAct,
    ActorAttributedToDoNotMatch,
}

impl fmt::Display for RequiredEventsError {
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
        }
    }
}

impl Error for RequiredEventsError {
    fn cause(&self) -> Option<&Error> {
        None
    }
}

pub struct VerifyRequiredEventsHandler(pub bool);

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

impl VerifyRequiredEventsHandler {
    #[async(boxed_send)]
    fn _handle<T: EntityStore + 'static>(
        is_local: bool,
        context: Context,
        entitystore: T,
        _inbox: String,
        elem: String,
    ) -> Result<(Context, T, String), Box<Error + Send + Sync + 'static>> {
        let subject = context.user.subject.to_owned();

        let mut elem = match await!(entitystore.get(elem, false)).map_err(Box::new)? {
            Some(val) => val,
            None => return Err(Box::new(RequiredEventsError::FailedToRetrieve)),
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
            return Err(Box::new(RequiredEventsError::MissingActor));
        } else {
            match actors[0] {
                Pointer::Id(ref subj) => {
                    if subj != &subject {
                        return Err(Box::new(RequiredEventsError::NotAllowedtoAct));
                    }
                }

                _ => return Err(Box::new(RequiredEventsError::NotAllowedtoAct)),
            }
        }

        let mut object = elem.main().get(as2!(object)).clone();

        if object.len() != 1 {
            return Err(Box::new(RequiredEventsError::MissingObject));
        }

        match object.remove(0) {
            Pointer::Id(id) => {
                if let Some(entity) = await!(entitystore.get(id, false)).map_err(Box::new)? {
                    if is_local
                        && !equals_any_order(
                            &entity.main()[as2!(attributedTo)],
                            &elem.main()[as2!(actor)],
                        )
                        && !elem.main().types.contains(&String::from(as2!(Update)))
                    {
                        Err(Box::new(RequiredEventsError::ActorAttributedToDoNotMatch))
                    } else {
                        Ok((context, entitystore, elem.id().to_owned()))
                    }
                } else {
                    Err(Box::new(RequiredEventsError::MissingObject))
                }
            }

            _ => Err(Box::new(RequiredEventsError::MissingObject)),
        }
    }
}

impl<T: EntityStore + 'static> MessageHandler<T> for VerifyRequiredEventsHandler {
    fn handle(
        &self,
        context: Context,
        entitystore: T,
        inbox: String,
        elem: String,
    ) -> Box<Future<Item = (Context, T, String), Error = Box<Error + Send + Sync + 'static>> + Send>
    {
        VerifyRequiredEventsHandler::_handle::<T>(self.0, context, entitystore, inbox, elem)
    }
}
