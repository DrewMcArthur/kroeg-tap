use jsonld::nodemap::Pointer;

use kroeg_tap::{box_store_error, Context, EntityStore, MessageHandler};

use std::error::Error;
use std::fmt;

use futures::prelude::{await, *};

#[derive(Debug)]
pub enum ClientUndoError {
    DifferingActor,
    MissingRequired(String),
}

impl fmt::Display for ClientUndoError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ClientUndoError::DifferingActor => write!(f, "as:actor on Undo and object differ!"),

            ClientUndoError::MissingRequired(ref val) => write!(
                f,
                "The {} predicate is missing or occurs more than once",
                val
            ),
        }
    }
}

impl Error for ClientUndoError {
    fn cause(&self) -> Option<&Error> {
        None
    }
}

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

pub struct ClientUndoHandler;

impl<T: EntityStore + 'static> MessageHandler<T> for ClientUndoHandler {
    #[async(boxed_send)]
    fn handle(
        &self,
        context: Context,
        store: T,
        _inbox: String,
        elem: String,
    ) -> Result<(Context, T, String), (Box<Error + Send + Sync + 'static>, T)> {
        let subject = context.user.subject.to_owned();
        let root = elem.to_owned();

        let (relem, store) = await!(store.get(elem, false)).map_err(box_store_error)?;
        let relem = relem.expect("Missing the entity being handled, shouldn't happen");

        if !relem.main().types.contains(&as2!(Undo).to_owned()) {
            return Ok((context, store, root));
        }

        let elem = if relem.main()[as2!(object)].len() == 1 {
            relem.main()[as2!(object)][0].to_owned()
        } else {
            return Err((
                Box::new(ClientUndoError::MissingRequired(as2!(object).to_owned())),
                store,
            ));
        };
        let elem = if let Pointer::Id(id) = elem {
            id
        } else {
            return Err((
                Box::new(ClientUndoError::MissingRequired(as2!(object).to_owned())),
                store,
            ));
        };

        let (elem, store) = await!(store.get(elem.to_owned(), false)).map_err(box_store_error)?;
        let elem = elem.unwrap();

        if !equals_any_order(&relem.main()[as2!(actor)], &elem.main()[as2!(actor)]) {
            return Err((Box::new(ClientUndoError::DifferingActor), store));
        }

        let (subj, mut store) = await!(store.get(subject, false)).map_err(box_store_error)?;
        let subj = subj.unwrap();

        if elem.main().types.contains(&as2!(Like).to_owned()) {
            if let Some(Pointer::Id(liked)) = subj.main()[as2!(liked)].iter().next().cloned() {
                store = await!(store.remove_collection(liked.to_owned(), elem.id().to_owned()))
                    .map_err(box_store_error)?;
            }
        }

        if elem.main().types.contains(&as2!(Follow).to_owned()) {
            if let Some(Pointer::Id(following)) =
                subj.main()[as2!(following)].iter().next().cloned()
            {
                store = await!(store.remove_collection(following.to_owned(), elem.id().to_owned()))
                    .map_err(box_store_error)?;
            }
        }

        Ok((context, store, root))
    }
}
