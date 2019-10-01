use serde_json::json;
use std::error::Error;
use std::fmt;

use kroeg_tap::{as2, assign_id, Context, MessageHandler, StoreItem};

pub struct AutomaticCreateHandler;

#[derive(Debug)]
pub enum AutomaticCreateError {
    ImproperActivity,
}

impl fmt::Display for AutomaticCreateError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            AutomaticCreateError::ImproperActivity => {
                write!(f, "Improper activity, did you forget the as:actor?")
            }
        }
    }
}

impl Error for AutomaticCreateError {}

const DEFAULT_ACTIVITIES: &'static [&'static str] = &[
    as2!(Accept),
    as2!(Add),
    as2!(Announce),
    as2!(Arrive),
    as2!(Block),
    as2!(Create),
    as2!(Delete),
    as2!(Dislike),
    as2!(Flag),
    as2!(Follow),
    as2!(Ignore),
    as2!(Invite),
    as2!(Join),
    as2!(Leave),
    as2!(Like),
    as2!(Listen),
    as2!(Move),
    as2!(Offer),
    as2!(Question),
    as2!(Reject),
    as2!(Read),
    as2!(Remove),
    as2!(TentativeReject),
    as2!(TentativeAccept),
    as2!(Travel),
    as2!(Undo),
    as2!(Update),
    as2!(View),
];

enum ObjectType {
    Activity,
    ImproperActivity,
    Object,
}

fn object_type(entity: &StoreItem) -> ObjectType {
    if entity.main()[as2!(actor)].len() > 0 {
        ObjectType::Activity
    } else {
        for typ in entity.main().types.iter() {
            if DEFAULT_ACTIVITIES.contains(&&**typ) {
                return ObjectType::ImproperActivity;
            }
        }

        ObjectType::Object
    }
}

const TO_CLONE: &'static [&'static str] =
    &[as2!(to), as2!(cc), as2!(bto), as2!(bcc), as2!(audience)];

#[async_trait::async_trait]
impl MessageHandler for AutomaticCreateHandler {
    async fn handle(
        &self,
        context: &mut Context<'_, '_>,
        _inbox: &mut String,
        elem: &mut String,
    ) -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
        let item = match context.entity_store.get(elem.to_owned(), false).await? {
            Some(item) => item,
            None => return Ok(()),
        };

        match object_type(&item) {
            ObjectType::Activity => return Ok(()),
            ObjectType::ImproperActivity => {
                return Err(AutomaticCreateError::ImproperActivity.into())
            }
            ObjectType::Object => (),
        }

        let assigned_id = assign_id(
            context,
            Some("activity".to_owned()),
            Some(elem.to_owned()),
            1,
        )
        .await?;

        let mut activity = StoreItem::parse(
            &assigned_id,
            &json!({
                "@id": &assigned_id,
                "@type": [as2!(Create)],
                as2!(object): [{"@id": item.id()}],
                as2!(actor): [{"@id": &context.user.subject}]
            }),
        )
        .unwrap();

        for predicate in TO_CLONE {
            activity
                .main_mut()
                .get_mut(predicate)
                .extend_from_slice(&item.main()[predicate]);
        }

        context.entity_store.put(assigned_id, &mut activity).await?;
        *elem = activity.id().to_owned();

        Ok(())
    }
}
