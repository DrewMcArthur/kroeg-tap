use jsonld::nodemap::Pointer;
use std::error::Error;

use kroeg_tap::{as2, Context, MessageHandler};

pub struct ServerFollowHandler;

#[async_trait::async_trait]
impl MessageHandler for ServerFollowHandler {
    async fn handle(
        &self,
        context: &mut Context<'_, '_>,
        inbox: &mut String,
        elem: &mut String,
    ) -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
        let root = match context.entity_store.get(elem.to_owned(), false).await? {
            Some(root) => root,
            None => return Ok(()),
        };

        let is_follow = root.main().types.iter().any(|f| f == as2!(Follow));
        let is_accept = root.main().types.iter().any(|f| f == as2!(Accept));
        let is_reject = root.main().types.iter().any(|f| f == as2!(Reject));

        if !is_follow && !is_accept && !is_reject {
            return Ok(());
        }

        let inbox = context
            .entity_store
            .get(inbox.to_owned(), true)
            .await?
            .unwrap();
        let attributed_to = &inbox.main()[as2!(attributedTo)];

        if attributed_to.is_empty() {
            return Ok(());
        }

        if is_accept || is_reject {
            // For each object that has been accepted/rejected:
            for pointer in &root.main()[as2!(object)] {
                let id = match pointer {
                    Pointer::Id(id) => id.to_owned(),
                    _ => continue,
                };

                let mut item = match context.entity_store.get(id, true).await? {
                    Some(item) => item,
                    None => continue,
                };

                // Only handle local follows.
                if !item.is_owned(context) || !item.main().types.iter().any(|f| f == as2!(Follow)) {
                    continue;
                }

                // Only handle accept/reject if sent to local user.
                if attributed_to != &item.main()[as2!(actor)] {
                    continue;
                }

                // Validate that object can still be accepted/rejected:
                // An object can only be rejected if it hasn't been rejected before, and it can only be
                //  accepted if it hasn't been accepted nor rejected.
                let accept = is_accept
                    && item.meta()[as2!(Accept)].is_empty()
                    && item.meta()[as2!(Reject)].is_empty();
                let reject = is_reject && item.meta()[as2!(Reject)].is_empty();

                // If this follow has already been accepted or rejected, ignore.
                if !accept && !reject {
                    continue;
                }

                // Follow is not targeting the user that posted this object, ignore.
                if &item.main()[as2!(object)] != &[Pointer::Id(context.user.subject.to_owned())] {
                    continue;
                }

                // For each person that authored the Follow (see above validation),
                for user in &item.main()[as2!(actor)] {
                    let id = match user {
                        Pointer::Id(id) => id,
                        _ => continue,
                    };

                    if let Some(user) = context.entity_store.get(id.to_owned(), true).await? {
                        if !user.is_owned(context) {
                            continue;
                        }

                        // If they have a following collection, add/remove users from following.
                        let following = match &user.main()[as2!(following)] as &[_] {
                            [Pointer::Id(id)] => id,
                            _ => continue,
                        };

                        if reject {
                            let _ = context.entity_store.remove_collection(
                                following.to_owned(),
                                context.user.subject.to_owned(),
                            );
                        } else {
                            let _ = context.entity_store.insert_collection(
                                following.to_owned(),
                                context.user.subject.to_owned(),
                            );
                        }
                    }
                }

                if reject {
                    item.meta()[as2!(Reject)].push(Pointer::Id(elem.to_owned()));
                } else {
                    item.meta()[as2!(Accept)].push(Pointer::Id(elem.to_owned()));
                }

                context
                    .entity_store
                    .put(item.id().to_owned(), &mut item)
                    .await?;
            }
        }

        return Ok(());
    }
}
