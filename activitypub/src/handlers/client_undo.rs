use jsonld::nodemap::Pointer;
use std::error::Error;
use std::fmt;

use kroeg_tap::{as2, Context, MessageHandler};

#[derive(Debug)]
pub enum ClientUndoError {
    DifferingActor,
    MissingRequired(String),
    MissingUndone,
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

            ClientUndoError::MissingUndone => write!(f, "The object to be undone is missing!"),
        }
    }
}

impl Error for ClientUndoError {}

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

#[async_trait::async_trait]
impl MessageHandler for ClientUndoHandler {
    async fn handle(
        &self,
        context: &mut Context<'_, '_>,
        _inbox: &mut String,
        elem: &mut String,
    ) -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
        let elem = context
            .entity_store
            .get(elem.to_owned(), false)
            .await?
            .expect("Cannot find the entity being operated on?!");

        if !elem.main().types.iter().any(|f| f == &as2!(Undo)) {
            return Ok(());
        }

        let undone = if let [Pointer::Id(id)] = &elem.main()[as2!(object)] as &[Pointer] {
            id.clone()
        } else {
            return Err(ClientUndoError::MissingRequired(as2!(object).to_owned()).into());
        };

        let undone = match context.entity_store.get(undone, false).await? {
            Some(undone) => undone,
            None => return Err(ClientUndoError::MissingUndone.into()),
        };

        if !equals_any_order(&elem.main()[as2!(actor)], &undone.main()[as2!(actor)]) {
            return Err(ClientUndoError::DifferingActor.into());
        }

        let subject = match context
            .entity_store
            .get(context.user.subject.clone(), false)
            .await?
        {
            Some(subject) => subject,
            None => return Ok(()),
        };

        if undone.main().types.iter().any(|f| f == &as2!(Like)) {
            if let [Pointer::Id(liked)] = &subject.main()[as2!(liked)] as &[Pointer] {
                context
                    .entity_store
                    .remove_collection(liked.to_owned(), undone.id().to_owned())
                    .await?;
            }
        }

        if undone.main().types.iter().any(|f| f == &as2!(Follow)) {
            if let [Pointer::Id(followed)] = &subject.main()[as2!(followed)] as &[Pointer] {
                context
                    .entity_store
                    .remove_collection(followed.to_owned(), undone.id().to_owned())
                    .await?;
            }
        }

        Ok(())
    }
}
