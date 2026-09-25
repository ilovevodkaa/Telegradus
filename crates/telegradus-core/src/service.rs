//! Russian texts for service messages ("Анна закрепил(а) сообщение", ...).
//!
//! Names are used in the nominative case with gender-neutral verb forms,
//! as Telegram's own Russian localization does.

use tdlib_rs::enums::MessageContent as C;

use crate::message::{Author, Names};
use crate::model::{MessageContent, UserId};
use crate::ru;

/// Renders a non-media message content. Common service messages become
/// [`MessageContent::Service`]; the rest is a labelled [`MessageContent::Unsupported`].
pub(crate) fn render(content: &C, author: &Author<'_>, names: &impl Names) -> MessageContent {
    let a = author.name;
    let is_channel = names.is_channel(author.chat_id);
    let user = |id: UserId| {
        names
            .user_name(id)
            .unwrap_or_else(|| "пользователь".to_owned())
    };
    let users = |ids: &[UserId]| {
        ids.iter()
            .map(|id| user(*id))
            .collect::<Vec<_>>()
            .join(", ")
    };

    let text = match content {
        C::MessageBasicGroupChatCreate(m) => format!("{a} создал(а) группу «{}»", m.title),
        C::MessageSupergroupChatCreate(m) if is_channel => format!("Канал «{}» создан", m.title),
        C::MessageSupergroupChatCreate(m) => format!("{a} создал(а) группу «{}»", m.title),
        C::MessageChatChangeTitle(m) if is_channel => {
            format!("Название канала изменено на «{}»", m.title)
        }
        C::MessageChatChangeTitle(m) => {
            format!("{a} изменил(а) название группы на «{}»", m.title)
        }
        C::MessageChatChangePhoto(_) if is_channel => "Фото канала обновлено".to_owned(),
        C::MessageChatChangePhoto(_) => format!("{a} обновил(а) фото группы"),
        C::MessageChatDeletePhoto if is_channel => "Фото канала удалено".to_owned(),
        C::MessageChatDeletePhoto => format!("{a} удалил(а) фото группы"),
        C::MessageChatAddMembers(m)
            if author.user_id().is_some_and(|id| m.member_user_ids == [id]) =>
        {
            format!("{a} присоединился(ась) к группе")
        }
        C::MessageChatAddMembers(m) => format!("{a} добавил(а) {}", users(&m.member_user_ids)),
        C::MessageChatJoinByLink => format!("{a} присоединился(ась) к группе по ссылке"),
        C::MessageChatJoinByRequest => format!("{a} принят(а) в группу"),
        C::MessageChatDeleteMember(m) if author.user_id() == Some(m.user_id) => {
            format!("{a} покинул(а) группу")
        }
        C::MessageChatDeleteMember(m) => format!("{a} удалил(а) {}", user(m.user_id)),
        C::MessageChatUpgradeTo(_) => "Группа преобразована в супергруппу".to_owned(),
        C::MessageChatUpgradeFrom(m) => {
            format!("Группа «{}» преобразована в супергруппу", m.title)
        }
        C::MessagePinMessage(_) if is_channel => "Сообщение закреплено".to_owned(),
        C::MessagePinMessage(_) => format!("{a} закрепил(а) сообщение"),
        C::MessageScreenshotTaken => format!("{a} сделал(а) снимок экрана"),
        C::MessageContactRegistered => format!("{a} теперь в Telegram"),
        C::MessageChatSetMessageAutoDeleteTime(m) if m.message_auto_delete_time <= 0 => {
            format!("{a} отключил(а) автоудаление сообщений")
        }
        C::MessageChatSetMessageAutoDeleteTime(m) => format!(
            "{a} установил(а) автоудаление сообщений через {}",
            ru::period(u64::from(m.message_auto_delete_time.unsigned_abs()))
        ),
        C::MessageVideoChatScheduled(_) => "Видеочат запланирован".to_owned(),
        C::MessageVideoChatStarted(_) if is_channel => "Трансляция начата".to_owned(),
        C::MessageVideoChatStarted(_) => format!("{a} начал(а) видеочат"),
        C::MessageVideoChatEnded(m) => {
            format!("Видеочат завершён ({})", ru::clock(m.duration))
        }
        C::MessageInviteVideoChatParticipants(m) => {
            format!("{a} пригласил(а) {} в видеочат", users(&m.user_ids))
        }
        C::MessageForumTopicCreated(m) => format!("Создана тема «{}»", m.name),
        C::MessageForumTopicEdited(_) => format!("{a} изменил(а) тему"),
        C::MessageForumTopicIsClosedToggled(m) if m.is_closed => "Тема закрыта".to_owned(),
        C::MessageForumTopicIsClosedToggled(_) => "Тема снова открыта".to_owned(),
        C::MessageCustomServiceAction(m) => m.text.clone(),
        C::MessageGameScore(m) => {
            let score = u64::from(m.score.unsigned_abs());
            format!(
                "{a} набрал(а) {}",
                ru::count(score, "очко", "очка", "очков")
            )
        }
        C::MessageChatSetTheme(_) => format!("{a} изменил(а) тему оформления чата"),
        C::MessageChatSetBackground(_) => format!("{a} изменил(а) обои чата"),
        C::MessageChatBoost(m) => {
            let boosts = u64::from(m.boost_count.unsigned_abs());
            format!(
                "{a} усилил(а) группу: {}",
                ru::count(boosts, "буст", "буста", "бустов")
            )
        }
        C::MessageChatOwnerLeft(_) => "Владелец покинул группу".to_owned(),
        C::MessageChatOwnerChanged(m) => {
            format!("Новый владелец группы: {}", user(m.new_owner_user_id))
        }
        C::MessageGift(_) | C::MessageUpgradedGift(_) => format!("{a} отправил(а) подарок"),
        C::MessageGiftedPremium(_) | C::MessagePremiumGiftCode(_) => {
            format!("{a} подарил(а) Telegram Premium")
        }
        C::MessageGiftedStars(_) => format!("{a} подарил(а) звёзды"),
        C::MessagePaymentSuccessful(_) | C::MessagePaymentSuccessfulBot(_) => {
            "Платёж выполнен".to_owned()
        }
        C::MessageSuggestProfilePhoto(_) => format!("{a} предлагает новое фото профиля"),
        C::MessageBotWriteAccessAllowed(_) => {
            "Вы разрешили боту отправлять вам сообщения".to_owned()
        }
        C::MessageWebAppDataSent(_) => "Данные отправлены боту".to_owned(),
        C::MessageProximityAlertTriggered(_) => "Оповещение о приближении".to_owned(),
        C::MessageGiveawayCreated(_) | C::MessageGiveaway(_) => "Розыгрыш".to_owned(),
        C::MessageGiveawayCompleted(_) | C::MessageGiveawayWinners(_) => {
            "Розыгрыш завершён".to_owned()
        }
        _ => return MessageContent::Unsupported("Служебное сообщение".to_owned()),
    };
    MessageContent::Service(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::tests::FakeNames;
    use crate::model::Sender;
    use tdlib_rs::types;

    fn render_as(content: C, sender: UserId, chat_id: i64, names: &FakeNames) -> MessageContent {
        let sender = Sender::User(sender);
        let name = names.user_name(1).unwrap_or_default();
        let author = Author {
            sender: &sender,
            name: &name,
            chat_id,
            is_outgoing: false,
        };
        render(&content, &author, names)
    }

    fn names() -> FakeNames {
        let mut names = FakeNames::default();
        names.users.insert(1, ("Анна", ""));
        names.users.insert(2, ("Борис", "Петров"));
        names.users.insert(3, ("Вера", ""));
        names.channels.push(-200);
        names
    }

    fn service(text: &str) -> MessageContent {
        MessageContent::Service(text.into())
    }

    #[test]
    fn membership_changes() {
        let names = names();
        let add = |ids: Vec<i64>| {
            C::MessageChatAddMembers(types::MessageChatAddMembers {
                member_user_ids: ids,
            })
        };
        assert_eq!(
            render_as(add(vec![2, 3]), 1, -100, &names),
            service("Анна добавил(а) Борис Петров, Вера")
        );
        assert_eq!(
            render_as(add(vec![1]), 1, -100, &names),
            service("Анна присоединился(ась) к группе")
        );
        let left = C::MessageChatDeleteMember(types::MessageChatDeleteMember { user_id: 1 });
        assert_eq!(
            render_as(left, 1, -100, &names),
            service("Анна покинул(а) группу")
        );
        let kicked = C::MessageChatDeleteMember(types::MessageChatDeleteMember { user_id: 3 });
        assert_eq!(
            render_as(kicked, 1, -100, &names),
            service("Анна удалил(а) Вера")
        );
        assert_eq!(
            render_as(C::MessageChatJoinByLink, 1, -100, &names),
            service("Анна присоединился(ась) к группе по ссылке")
        );
    }

    #[test]
    fn group_and_channel_changes() {
        let names = names();
        let title =
            |t: &str| C::MessageChatChangeTitle(types::MessageChatChangeTitle { title: t.into() });
        assert_eq!(
            render_as(title("Друзья"), 1, -100, &names),
            service("Анна изменил(а) название группы на «Друзья»")
        );
        assert_eq!(
            render_as(title("Новости"), 1, -200, &names),
            service("Название канала изменено на «Новости»")
        );
        let created = C::MessageSupergroupChatCreate(types::MessageSupergroupChatCreate {
            title: "Новости".into(),
        });
        assert_eq!(
            render_as(created, 1, -200, &names),
            service("Канал «Новости» создан")
        );
        let pinned = C::MessagePinMessage(types::MessagePinMessage { message_id: 5 });
        assert_eq!(
            render_as(pinned, 1, -100, &names),
            service("Анна закрепил(а) сообщение")
        );
        assert_eq!(
            render_as(C::MessageContactRegistered, 1, 1, &names),
            service("Анна теперь в Telegram")
        );
        assert_eq!(
            render_as(C::MessageChatDeletePhoto, 1, -100, &names),
            service("Анна удалил(а) фото группы")
        );
    }

    #[test]
    fn unknown_service_messages_are_labelled() {
        let names = names();
        let content = C::MessageChatShared(types::MessageChatShared {
            chat: types::SharedChat { chat_id: 1 },
            button_id: 0,
        });
        assert_eq!(
            render_as(content, 1, 1, &names),
            MessageContent::Unsupported("Служебное сообщение".into())
        );
    }
}
