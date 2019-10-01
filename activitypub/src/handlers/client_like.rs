use jsonld::nodemap::Pointer;
use std::error::Error;

use kroeg_tap::{as2, Context, MessageHandler};

pub struct ClientLikeHandler;

#[async_trait::async_trait]
impl MessageHandler for ClientLikeHandler {
    async fn handle(
        &self,
        context: &mut Context<'_, '_>,
        _inbox: &mut String,
        elem: &mut String,
    ) -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
        let elem = match context.entity_store.get(elem.to_owned(), false).await? {
            Some(elem) => elem,
            None => return Ok(()),
        };

        if !elem.main().types.iter().any(|f| f == as2!(Like)) {
            return Ok(());
        }

        let subject = match context
            .entity_store
            .get(context.user.subject.clone(), false)
            .await?
        {
            Some(subject) => subject,
            None => return Ok(()),
        };

        let liked = if let [Pointer::Id(id)] = &subject.main()[as2!(liked)] as &[Pointer] {
            id.clone()
        } else {
            return Ok(());
        };

        for object in &elem.main()[as2!(object)] {
            if let Pointer::Id(id) = object {
                context
                    .entity_store
                    .insert_collection(liked.clone(), id.to_owned())
                    .await?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::ClientLikeHandler;
    use crate::test::TestStore;
    use crate::{handle_object_pair, object_under_test};
    use async_std::task::block_on;
    use kroeg_tap::{as2, MessageHandler};

    fn setup() -> (TestStore, ()) {
        (
            TestStore::new(vec![
                object_under_test!(remote "/like" => {
                    types => [as2!(Like)];
                    as2!(object) => ["/object"];
                }),
                object_under_test!(local "/subject" => {
                    types => [as2!(Person)];
                    as2!(liked) => ["/liked"];
                }),
            ]),
            (),
        )
    }

    #[test]
    fn handles_like() {
        let (mut store, mut queue) = setup();
        let mut context = store.context(&mut queue);

        match block_on(ClientLikeHandler.handle(
            &mut context,
            &mut "/inbox".to_owned(),
            &mut "/like".to_owned(),
        )) {
            Ok(()) => {
                assert!(
                    store.has_read_all(&["/like", "/subject"]),
                    "Handler did not read the Like nor the subject"
                );
                assert!(
                    store.contains("/liked", "/object"),
                    "Handler did not register the Like"
                );
            }

            Err(e) => panic!("handler returned error: {}", e),
        }
    }
}
