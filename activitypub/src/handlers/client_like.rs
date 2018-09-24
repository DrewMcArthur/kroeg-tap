use jsonld::nodemap::{Entity, Pointer};

use kroeg_tap::{Context, EntityStore, MessageHandler};

use std::error::Error;
use std::fmt;

use futures::prelude::{await, *};

#[derive(Debug)]
pub enum ClientLikeError {
    MissingRequired(String),
}

impl fmt::Display for ClientLikeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ClientLikeError::MissingRequired(ref val) => write!(
                f,
                "The {} predicate is missing or occurs more than once",
                val
            ),
        }
    }
}

impl Error for ClientLikeError {
    fn cause(&self) -> Option<&Error> {
        None
    }
}

fn _ensure(entity: &Entity, name: &str) -> Result<Pointer, Box<Error + Send + Sync + 'static>> {
    if entity[name].len() == 1 {
        Ok(entity[name][0].to_owned())
    } else {
        Err(Box::new(ClientLikeError::MissingRequired(name.to_owned())))
    }
}

pub struct ClientLikeHandler;

impl<T: EntityStore + 'static> MessageHandler<T> for ClientLikeHandler {
    #[async(boxed_send)]
    fn handle(
        &self,
        context: Context,
        mut store: T,
        _inbox: String,
        elem: String,
    ) -> Result<(Context, T, String), Box<Error + Send + Sync + 'static>> {
        let subject = context.user.subject.to_owned();
        let root = elem.to_owned();

        let mut elem = await!(store.get(elem, false))
            .map_err(Box::new)?
            .expect("Missing the entity being handled, shouldn't happen");

        if !elem.main().types.contains(&as2!(Like).to_owned()) {
            return Ok((context, store, root));
        }

        let elem = _ensure(elem.main(), as2!(object))?;
        let elem = if let Pointer::Id(id) = elem {
            id
        } else {
            return Err(Box::new(ClientLikeError::MissingRequired(
                as2!(object).to_owned(),
            )));
        };

        let mut elem = await!(store.get(elem.to_owned(), false))
            .map_err(Box::new)?
            .unwrap();

        let mut subj = await!(store.get(subject, false))
            .map_err(Box::new)?
            .unwrap();

        if let Some(Pointer::Id(liked)) = subj.main()[as2!(liked)].iter().next().cloned() {
            await!(store.insert_collection(liked.to_owned(), elem.id().to_owned()))
                .map_err(Box::new)?
        }

        Ok((context, store, root))
    }
}
