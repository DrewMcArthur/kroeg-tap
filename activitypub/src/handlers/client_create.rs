use jsonld::nodemap::Pointer;
use serde_json::json;
use std::error::Error;
use std::fmt;

use kroeg_tap::{as2, assign_id, Context, MessageHandler, StoreItem};

#[derive(Debug)]
pub enum ClientCreateError {
    ExistingPredicate(String),
    MissingRequired(String),
}

impl fmt::Display for ClientCreateError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ClientCreateError::MissingRequired(ref val) => write!(
                f,
                "The {} predicate is missing or occurs more than once",
                val
            ),
            ClientCreateError::ExistingPredicate(ref val) => {
                write!(f, "The {} predicate should not have been passed", val)
            }
        }
    }
}

impl Error for ClientCreateError {}

pub struct ClientCreateHandler;

#[async_trait::async_trait]
impl MessageHandler for ClientCreateHandler {
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
            .expect("Cannot find the entity being handled on");

        if !elem.main().types.contains(&as2!(Create).to_owned()) {
            return Ok(());
        }

        let elem = if let [Pointer::Id(id)] = &elem.main()[as2!(object)] as &[Pointer] {
            id.clone()
        } else {
            return Err(ClientCreateError::MissingRequired(as2!(object).to_owned()).into());
        };

        let mut elem = context
            .entity_store
            .get(elem, false)
            .await?
            .expect("Object pointed at doesn't exist?!");

        for &itemname in &["likes", "shares", "replies"] {
            let id = assign_id(
                context,
                Some(itemname.to_owned()),
                Some(elem.id().to_owned()),
                1,
            )
            .await?;

            let mut item = StoreItem::parse(
                &id,
                &json!({
                    "@id": id,
                    "@type": [as2!(OrderedCollection)],
                    as2!(partOf): [{"@id": elem.id()}]
                }),
            )
            .unwrap();

            context.entity_store.put(id.clone(), &mut item).await?;

            let name = format!("https://www.w3.org/ns/activitystreams#{}", itemname);

            if !elem.main()[&name].is_empty() {
                return Err(ClientCreateError::ExistingPredicate(name).into());
            } else {
                elem.main_mut().get_mut(&name).push(Pointer::Id(id));
            }
        }

        context
            .entity_store
            .put(elem.id().to_owned(), &mut elem)
            .await?;

        Ok(())
    }
}
