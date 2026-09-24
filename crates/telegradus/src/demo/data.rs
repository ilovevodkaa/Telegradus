//! Canned demo content: the user, ~45 chats, folders and message histories.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{Duration, Local, TimeZone};
use telegradus_core::{
    ChatId, ChatKind, ChatListEntry, ChatSummary, FileId, FileRef, Folder, FormattedText, Me,
    Message, MessageContent, MessageId, MessagePreview, Sender, SendingState, TextEntity,
    TextEntityKind, UserId,
};

use super::images::Picture;
use crate::format;

pub const ME: UserId = 1000;
/// The group chat with the richest history (opened by `--demo=chat`).
pub const SHOWCASE_CHAT: ChatId = 2;
/// Chats in the main list before the first `LoadChats`.
pub const FIRST_PAGE: usize = 24;

/// A file the demo can "download".
#[derive(Debug, Clone)]
pub enum DemoFile {
    Picture(Picture),
    Document,
}

pub struct World {
    pub me: Me,
    pub chats: HashMap<ChatId, ChatSummary>,
    pub main: Vec<ChatListEntry>,
    pub archive: Vec<ChatListEntry>,
    pub folders: Vec<(Folder, Vec<ChatListEntry>)>,
    pub histories: HashMap<ChatId, Vec<Message>>,
    /// Pages returned by `LoadOlder`, newest page first.
    pub older: HashMap<ChatId, Vec<Vec<Message>>>,
    pub files: HashMap<FileId, (FileRef, DemoFile)>,
}

#[derive(Clone, Copy)]
enum Kind {
    Private(UserId),
    Group,
    Channel,
    Secret(UserId),
}

#[derive(Clone, Copy)]
enum Who {
    Me,
    Them(&'static str),
    Post,
}

struct Spec {
    id: ChatId,
    title: &'static str,
    kind: Kind,
    pinned: bool,
    muted: bool,
    unread: i32,
    mentions: i32,
    avatar: Option<u32>,
    last: (&'static str, Who),
    minutes_ago: i64,
    folders: &'static [i32],
    archived: bool,
}

const WORK: i32 = 2;
const PERSONAL: i32 = 1;
const CHANNELS: i32 = 3;

#[rustfmt::skip]
const SPECS: &[Spec] = &[
    Spec { id: 1, title: "Избранное", kind: Kind::Private(ME), pinned: true, muted: false, unread: 0, mentions: 0, avatar: None, last: ("Список покупок: молоко, хлеб, кофе в зёрнах, батарейки AA", Who::Me), minutes_ago: 60 * 5, folders: &[PERSONAL], archived: false },
    Spec { id: 2, title: "Команда Telegradus", kind: Kind::Group, pinned: true, muted: false, unread: 3, mentions: 1, avatar: Some(0), last: ("", Who::Me), minutes_ago: 0, folders: &[WORK], archived: false },
    Spec { id: 3, title: "Мама ❤️", kind: Kind::Private(2001), pinned: true, muted: false, unread: 1, mentions: 0, avatar: None, last: ("Не забудь позвонить бабушке в воскресенье 😘", Who::Them("Мама")), minutes_ago: 27, folders: &[PERSONAL], archived: false },
    Spec { id: 4, title: "Rust по-русски 🦀", kind: Kind::Group, pinned: false, muted: true, unread: 128, mentions: 0, avatar: Some(2), last: ("Кто-нибудь пробовал iced 0.14? Как там с текстовыми редакторами?", Who::Them("Олег")), minutes_ago: 4, folders: &[WORK], archived: false },
    Spec { id: 5, title: "Анна Смирнова", kind: Kind::Private(2002), pinned: false, muted: false, unread: 2, mentions: 0, avatar: Some(4), last: ("Скинула макеты, глянь, когда будет минутка", Who::Them("Анна Смирнова")), minutes_ago: 12, folders: &[PERSONAL, WORK], archived: false },
    Spec { id: 6, title: "Новости технологий", kind: Kind::Channel, pinned: false, muted: true, unread: 12, mentions: 0, avatar: Some(1), last: ("Linux 6.18 вышел: новый планировщик и ускорение ввода-вывода", Who::Post), minutes_ago: 35, folders: &[CHANNELS], archived: false },
    Spec { id: 7, title: "Дизайн-ревью", kind: Kind::Group, pinned: false, muted: false, unread: 0, mentions: 0, avatar: None, last: ("Согласовали: только монохром и пунктирные чипы", Who::Me), minutes_ago: 58, folders: &[WORK], archived: false },
    Spec { id: 8, title: "Игорь Лебедев", kind: Kind::Private(2003), pinned: false, muted: false, unread: 0, mentions: 0, avatar: None, last: ("Ок, созвонимся завтра в 11", Who::Me), minutes_ago: 95, folders: &[PERSONAL, WORK], archived: false },
    Spec { id: 9, title: "Книжный клуб 📚", kind: Kind::Group, pinned: false, muted: false, unread: 5, mentions: 1, avatar: None, last: ("@dorlov, твоя очередь выбирать книгу на март!", Who::Them("Вера")), minutes_ago: 130, folders: &[PERSONAL], archived: false },
    Spec { id: 10, title: "Arch Linux RU", kind: Kind::Group, pinned: false, muted: true, unread: 42, mentions: 0, avatar: Some(5), last: ("После обновления mesa всё летает, рекомендую", Who::Them("Сергей")), minutes_ago: 150, folders: &[], archived: false },
    Spec { id: 11, title: "Катя 🌸", kind: Kind::Private(2004), pinned: false, muted: false, unread: 0, mentions: 0, avatar: Some(4), last: ("Фото", Who::Them("Катя")), minutes_ago: 60 * 4, folders: &[PERSONAL], archived: false },
    Spec { id: 12, title: "Бег по утрам 🏃", kind: Kind::Group, pinned: false, muted: false, unread: 0, mentions: 0, avatar: None, last: ("Завтра в 7:00 у фонтана, погода обещает +12", Who::Them("Тимур")), minutes_ago: 60 * 6, folders: &[PERSONAL], archived: false },
    Spec { id: 13, title: "Telegram Tips", kind: Kind::Channel, pinned: false, muted: false, unread: 1, mentions: 0, avatar: Some(1), last: ("Совет дня: закрепляйте важные чаты, чтобы не терять их в списке", Who::Post), minutes_ago: 60 * 7, folders: &[CHANNELS], archived: false },
    Spec { id: 14, title: "Сергей Иванов", kind: Kind::Private(2005), pinned: false, muted: false, unread: 0, mentions: 0, avatar: None, last: ("Спасибо, всё получил 👍", Who::Them("Сергей Иванов")), minutes_ago: 60 * 9, folders: &[PERSONAL], archived: false },
    Spec { id: 15, title: "Семья 🏡", kind: Kind::Group, pinned: false, muted: false, unread: 4, mentions: 0, avatar: Some(3), last: ("В субботу шашлыки на даче, кто едет?", Who::Them("Папа")), minutes_ago: 60 * 10, folders: &[PERSONAL], archived: false },
    Spec { id: 16, title: "Доставка «Вкусно»", kind: Kind::Private(2006), pinned: false, muted: true, unread: 0, mentions: 0, avatar: None, last: ("Ваш заказ №4815 доставлен. Приятного аппетита!", Who::Them("Доставка «Вкусно»")), minutes_ago: 60 * 20, folders: &[], archived: false },
    Spec { id: 17, title: "Ольга (бухгалтерия)", kind: Kind::Private(2007), pinned: false, muted: false, unread: 0, mentions: 0, avatar: None, last: ("Акты подписала, оригиналы отправлю почтой", Who::Them("Ольга")), minutes_ago: 60 * 26, folders: &[WORK], archived: false },
    Spec { id: 18, title: "Хакатон 2026", kind: Kind::Group, pinned: false, muted: false, unread: 0, mentions: 0, avatar: Some(2), last: ("Регистрация закрывается в пятницу в 18:00", Who::Them("Организаторы")), minutes_ago: 60 * 28, folders: &[WORK], archived: false },
    Spec { id: 19, title: "Кино по пятницам 🎬", kind: Kind::Group, pinned: false, muted: true, unread: 7, mentions: 0, avatar: None, last: ("Голосуем: «Дюна» или «Интерстеллар»?", Who::Them("Лена")), minutes_ago: 60 * 30, folders: &[PERSONAL], archived: false },
    Spec { id: 20, title: "Алексей Ким", kind: Kind::Private(2008), pinned: false, muted: false, unread: 0, mentions: 0, avatar: None, last: ("Голосовое сообщение", Who::Them("Алексей Ким")), minutes_ago: 60 * 33, folders: &[PERSONAL], archived: false },
    Spec { id: 21, title: "Ремонт квартиры 🔨", kind: Kind::Group, pinned: false, muted: false, unread: 0, mentions: 0, avatar: None, last: ("Плитку привезут в четверг до обеда", Who::Me), minutes_ago: 60 * 40, folders: &[PERSONAL], archived: false },
    Spec { id: 22, title: "Linux Weekly", kind: Kind::Channel, pinned: false, muted: true, unread: 3, mentions: 0, avatar: None, last: ("Выпуск №212: Wayland по умолчанию, новые драйверы и немного истории", Who::Post), minutes_ago: 60 * 44, folders: &[CHANNELS], archived: false },
    Spec { id: 23, title: "Марина Петрова", kind: Kind::Private(2009), pinned: false, muted: false, unread: 0, mentions: 0, avatar: Some(3), last: ("До встречи на конференции!", Who::Me), minutes_ago: 60 * 50, folders: &[PERSONAL, WORK], archived: false },
    Spec { id: 24, title: "Фотоклуб 📷", kind: Kind::Group, pinned: false, muted: false, unread: 0, mentions: 0, avatar: Some(0), last: ("Фото · Рассвет над заливом", Who::Them("Аркадий")), minutes_ago: 60 * 55, folders: &[PERSONAL], archived: false },
    Spec { id: 25, title: "Дмитрий", kind: Kind::Private(2010), pinned: false, muted: false, unread: 0, mentions: 0, avatar: None, last: ("Посмотри PR, там пара мелочей по стилю", Who::Them("Дмитрий")), minutes_ago: 60 * 70, folders: &[WORK], archived: false },
    Spec { id: 26, title: "Походы и горы ⛰️", kind: Kind::Group, pinned: false, muted: true, unread: 15, mentions: 0, avatar: None, last: ("Маршрут на Эльбрус скинул в закреп", Who::Them("Глеб")), minutes_ago: 60 * 75, folders: &[PERSONAL], archived: false },
    Spec { id: 27, title: "Вакансии Rust", kind: Kind::Channel, pinned: false, muted: true, unread: 9, mentions: 0, avatar: Some(2), last: ("Senior Rust Engineer, удалённо, вилка обсуждается", Who::Post), minutes_ago: 60 * 80, folders: &[CHANNELS, WORK], archived: false },
    Spec { id: 28, title: "Юля Соколова", kind: Kind::Private(2011), pinned: false, muted: false, unread: 0, mentions: 0, avatar: None, last: ("Стикер", Who::Them("Юля Соколова")), minutes_ago: 60 * 90, folders: &[PERSONAL], archived: false },
    Spec { id: 29, title: "Настолки по средам 🎲", kind: Kind::Group, pinned: false, muted: false, unread: 0, mentions: 0, avatar: None, last: ("Беру «Каркассон» и «Колонизаторов»", Who::Me), minutes_ago: 60 * 100, folders: &[PERSONAL], archived: false },
    Spec { id: 30, title: "Бот погоды ☀️", kind: Kind::Private(2012), pinned: false, muted: true, unread: 0, mentions: 0, avatar: Some(1), last: ("Москва: +14°, облачно, ветер 3 м/с", Who::Them("Бот погоды")), minutes_ago: 60 * 110, folders: &[], archived: false },
    Spec { id: 31, title: "Школа №57 — родители", kind: Kind::Group, pinned: false, muted: true, unread: 31, mentions: 0, avatar: None, last: ("Собрание переносится на четверг, 19:00", Who::Them("Классный руководитель")), minutes_ago: 60 * 120, folders: &[PERSONAL], archived: false },
    Spec { id: 32, title: "Никита", kind: Kind::Private(2013), pinned: false, muted: false, unread: 0, mentions: 0, avatar: None, last: ("Го в субботу на каток?", Who::Them("Никита")), minutes_ago: 60 * 130, folders: &[PERSONAL], archived: false },
    Spec { id: 33, title: "Open Source Moscow", kind: Kind::Group, pinned: false, muted: false, unread: 0, mentions: 0, avatar: Some(5), last: ("Слайды с митапа выложили на сайт", Who::Them("Алина")), minutes_ago: 60 * 150, folders: &[WORK], archived: false },
    Spec { id: 34, title: "Вера Николаевна", kind: Kind::Private(2014), pinned: false, muted: false, unread: 0, mentions: 0, avatar: None, last: ("Спасибо большое за помощь с ноутбуком!", Who::Them("Вера Николаевна")), minutes_ago: 60 * 170, folders: &[PERSONAL], archived: false },
    Spec { id: 35, title: "Кофейня «Зерно» ☕", kind: Kind::Channel, pinned: false, muted: false, unread: 0, mentions: 0, avatar: Some(3), last: ("Новый сорт из Эфиопии уже на полках", Who::Post), minutes_ago: 60 * 190, folders: &[CHANNELS], archived: false },
    Spec { id: 36, title: "Артём (гитара)", kind: Kind::Private(2015), pinned: false, muted: false, unread: 0, mentions: 0, avatar: None, last: ("Занятие в четверг в силе", Who::Them("Артём")), minutes_ago: 60 * 200, folders: &[PERSONAL], archived: false },
    Spec { id: 37, title: "Английский клуб 🇬🇧", kind: Kind::Group, pinned: false, muted: false, unread: 0, mentions: 0, avatar: None, last: ("Topic for Sunday: travel stories ✈️", Who::Them("Emma")), minutes_ago: 60 * 220, folders: &[PERSONAL], archived: false },
    Spec { id: 38, title: "Лена", kind: Kind::Private(2016), pinned: false, muted: false, unread: 0, mentions: 0, avatar: Some(4), last: ("Ахаха, это гениально 😂", Who::Them("Лена")), minutes_ago: 60 * 240, folders: &[PERSONAL], archived: false },
    Spec { id: 39, title: "Велосипедисты СПб 🚲", kind: Kind::Group, pinned: false, muted: true, unread: 64, mentions: 0, avatar: None, last: ("Велоночь в эту субботу, старт от Дворцовой", Who::Them("Макс")), minutes_ago: 60 * 260, folders: &[], archived: false },
    Spec { id: 40, title: "Тимур", kind: Kind::Private(2017), pinned: false, muted: false, unread: 0, mentions: 0, avatar: None, last: ("Отправил тебе документы", Who::Them("Тимур")), minutes_ago: 60 * 300, folders: &[WORK], archived: false },
    Spec { id: 41, title: "Старые друзья", kind: Kind::Group, pinned: false, muted: false, unread: 0, mentions: 0, avatar: None, last: ("Встреча выпускников 20 июня!", Who::Them("Денис")), minutes_ago: 60 * 400, folders: &[PERSONAL], archived: false },
    Spec { id: 42, title: "Анна Смирнова", kind: Kind::Secret(2002), pinned: false, muted: false, unread: 0, mentions: 0, avatar: Some(4), last: ("Пароль от Wi-Fi на даче: ********", Who::Them("Анна Смирнова")), minutes_ago: 60 * 500, folders: &[PERSONAL], archived: false },
    Spec { id: 43, title: "Спам-рассылка", kind: Kind::Channel, pinned: false, muted: true, unread: 99, mentions: 0, avatar: None, last: ("Только сегодня скидки 90%!!!", Who::Post), minutes_ago: 60 * 30, folders: &[], archived: true },
    Spec { id: 44, title: "Старый проект", kind: Kind::Group, pinned: false, muted: false, unread: 0, mentions: 0, avatar: None, last: ("Проект закрыт, всем спасибо", Who::Me), minutes_ago: 60 * 24 * 40, folders: &[], archived: true },
    Spec { id: 45, title: "Крипто-сигналы 🚀", kind: Kind::Channel, pinned: false, muted: true, unread: 0, mentions: 0, avatar: None, last: ("Биткоин снова пробил уровень сопротивления", Who::Post), minutes_ago: 60 * 24 * 12, folders: &[], archived: true },
];

fn now() -> i64 {
    Local::now().timestamp()
}

/// Local time `days` days ago at `hh:mm`.
fn day_at(days: i64, hour: u32, minute: u32) -> i64 {
    let date = Local::now().date_naive() - Duration::days(days);
    date.and_hms_opt(hour, minute, 0)
        .and_then(|t| Local.from_local_datetime(&t).earliest())
        .map_or_else(now, |t| t.timestamp())
}

/// Text with entities marked by the first occurrence of each substring.
fn rich(text: &str, marks: &[(&str, TextEntityKind)]) -> FormattedText {
    let mut entities: Vec<TextEntity> = marks
        .iter()
        .filter_map(|(needle, kind)| {
            let start = text.find(needle)?;
            Some(TextEntity {
                range: start..start + needle.len(),
                kind: kind.clone(),
            })
        })
        .collect();
    entities.sort_by_key(|e| (e.range.start, std::cmp::Reverse(e.range.end)));
    FormattedText {
        text: text.to_owned(),
        entities,
    }
}

fn plain(text: &str) -> FormattedText {
    FormattedText {
        text: text.to_owned(),
        entities: Vec::new(),
    }
}

fn text(t: &str) -> MessageContent {
    MessageContent::Text(plain(t))
}

fn file_ref(id: FileId, size: i64) -> FileRef {
    FileRef {
        id,
        size,
        local_path: None,
        is_downloading: false,
        downloaded_size: 0,
    }
}

struct History<'a> {
    chat_id: ChatId,
    next_id: MessageId,
    messages: Vec<Message>,
    files: &'a mut HashMap<FileId, (FileRef, DemoFile)>,
    next_file: &'a mut FileId,
}

impl History<'_> {
    fn push(&mut self, date: i64, who: Who, content: MessageContent) -> MessageId {
        let id = self.next_id;
        self.next_id += 1;
        let (sender, sender_name, is_outgoing) = match who {
            Who::Me => (Sender::User(ME), "Даниил Орлов".to_owned(), true),
            Who::Them(name) => (Sender::User(user_id(name)), name.to_owned(), false),
            Who::Post => (Sender::Chat(self.chat_id), String::new(), false),
        };
        self.messages.push(Message {
            id,
            chat_id: self.chat_id,
            sender,
            sender_name,
            date,
            edit_date: 0,
            is_outgoing,
            sending_state: SendingState::Sent,
            reply_to: None,
            content,
        });
        id
    }

    fn reply(&mut self, date: i64, who: Who, content: MessageContent, to: MessageId) -> MessageId {
        let id = self.push(date, who, content);
        if let Some(last) = self.messages.last_mut() {
            last.reply_to = Some(to);
        }
        id
    }

    fn photo(&mut self, picture: Picture, caption: FormattedText) -> MessageContent {
        let id = self.new_file(120_000, DemoFile::Picture(picture));
        let (width, height) = picture.size();
        MessageContent::Photo {
            file: file_ref(id, 120_000),
            width: width as i32,
            height: height as i32,
            caption,
        }
    }

    fn document(&mut self, name: &str, size: i64) -> MessageContent {
        let id = self.new_file(size, DemoFile::Document);
        MessageContent::Document {
            file_name: name.to_owned(),
            file: file_ref(id, size),
            caption: FormattedText::default(),
        }
    }

    fn new_file(&mut self, size: i64, kind: DemoFile) -> FileId {
        let id = *self.next_file;
        *self.next_file += 1;
        self.files.insert(id, (file_ref(id, size), kind));
        id
    }
}

fn user_id(name: &str) -> UserId {
    3000 + name
        .bytes()
        .fold(0i64, |h, b| (h * 31 + i64::from(b)) % 100_000)
}

/// Builds the whole demo world.
pub fn world() -> World {
    let mut files = HashMap::new();
    let mut next_file: FileId = 7000;
    let mut histories = HashMap::new();
    let mut older = HashMap::new();
    let mut chats = HashMap::new();

    for spec in SPECS {
        let mut history = History {
            chat_id: spec.id,
            next_id: 100,
            messages: Vec::new(),
            files: &mut files,
            next_file: &mut next_file,
        };
        if spec.id == SHOWCASE_CHAT {
            showcase(&mut history);
            let pages = (0..2).map(|page| filler_page(spec.id, page)).collect();
            older.insert(spec.id, pages);
        } else {
            generic(&mut history, spec);
        }
        let messages = std::mem::take(&mut history.messages);
        let last = messages.last();
        let photo = spec.avatar.map(|seed| {
            let id = 5000 + spec.id as FileId;
            files.insert(
                id,
                (
                    file_ref(id, 9_000),
                    DemoFile::Picture(Picture::Avatar { seed }),
                ),
            );
            file_ref(id, 9_000)
        });
        let kind = match spec.kind {
            Kind::Private(user_id) => ChatKind::Private { user_id },
            Kind::Secret(user_id) => ChatKind::Secret { user_id },
            Kind::Group => ChatKind::Supergroup,
            Kind::Channel => ChatKind::Channel,
        };
        let is_group = matches!(spec.kind, Kind::Group);
        let summary = ChatSummary {
            id: spec.id,
            title: spec.title.to_owned(),
            kind,
            photo,
            last_message: last.map(|m| preview(m, is_group)),
            unread_count: spec.unread,
            unread_mention_count: spec.mentions,
            is_muted: spec.muted,
            draft: (spec.id == 7).then(|| "Нужно ещё обсудить иконки".to_owned()),
            can_send_messages: !matches!(spec.kind, Kind::Channel),
            last_read_inbox_message_id: last.map_or(0, |m| m.id - i64::from(spec.unread.min(3))),
        };
        chats.insert(spec.id, summary);
        histories.insert(spec.id, messages);
    }

    let entry = |spec: &Spec| ChatListEntry {
        chat_id: spec.id,
        is_pinned: spec.pinned,
    };
    let main = SPECS.iter().filter(|s| !s.archived).map(entry).collect();
    let archive = SPECS.iter().filter(|s| s.archived).map(entry).collect();
    let folder = |id: i32, title: &str| {
        let entries = SPECS
            .iter()
            .filter(|s| s.folders.contains(&id))
            .map(|s| ChatListEntry {
                chat_id: s.id,
                is_pinned: false,
            })
            .collect();
        (
            Folder {
                id,
                title: title.to_owned(),
            },
            entries,
        )
    };

    World {
        me: Me {
            id: ME,
            first_name: "Даниил".into(),
            last_name: "Орлов".into(),
            username: Some("dorlov".into()),
            phone_number: "+7 900 123-45-67".into(),
        },
        chats,
        main,
        archive,
        folders: vec![
            folder(PERSONAL, "Личное"),
            folder(WORK, "Работа"),
            folder(CHANNELS, "Каналы"),
        ],
        histories,
        older,
        files,
    }
}

pub fn preview(message: &Message, is_group: bool) -> MessagePreview {
    MessagePreview {
        id: message.id,
        date: message.date,
        is_outgoing: message.is_outgoing,
        sender_name: (is_group && !message.is_outgoing && !message.sender_name.is_empty())
            .then(|| first_name(&message.sender_name).to_owned()),
        text: format::content_preview(&message.content).into_owned(),
    }
}

fn first_name(name: &str) -> &str {
    name.split_whitespace().next().unwrap_or(name)
}

/// A plausible short conversation ending with the chat's list preview.
fn generic(h: &mut History<'_>, spec: &Spec) {
    let base = now() - spec.minutes_ago * 60;
    let them = match spec.last.1 {
        Who::Them(name) => Who::Them(name),
        _ => Who::Them(first_name(spec.title)),
    };
    match spec.kind {
        Kind::Channel => {
            h.push(
                base - 3 * 86_400,
                Who::Post,
                text("Добро пожаловать! Здесь мы публикуем самое интересное за неделю."),
            );
            let picture = h.photo(
                Picture::Landscape {
                    seed: spec.id as u32,
                },
                plain("Фото недели"),
            );
            h.push(base - 2 * 86_400, Who::Post, picture);
            h.push(
                base - 86_400,
                Who::Post,
                MessageContent::Text(rich(
                    "Подборка материалов: как устроены современные мессенджеры и почему нативные клиенты быстрее. Читать: example.com/messengers",
                    &[("example.com/messengers", TextEntityKind::Url)],
                )),
            );
        }
        _ => {
            let lines = [
                (Who::Me, "Привет! Как дела?"),
                (them, "Привет! Всё хорошо, спасибо 🙂 А у тебя?"),
                (Who::Me, "Отлично, доделываю новый клиент для Telegram"),
                (them, "О, интересно! На чём пишешь?"),
                (
                    Who::Me,
                    "На Rust, интерфейс на iced — получается очень лёгким",
                ),
                (them, "Покажешь потом?"),
                (Who::Me, "Конечно, скоро будет первая сборка"),
            ];
            for (i, (who, line)) in lines.iter().enumerate() {
                let date = base - 2 * 86_400 + i as i64 * 540;
                h.push(date, *who, text(line));
            }
        }
    }
    let (last_text, who) = spec.last;
    let content = match last_text {
        "Фото" => h.photo(
            Picture::City {
                seed: spec.id as u32,
            },
            FormattedText::default(),
        ),
        "Голосовое сообщение" => MessageContent::Voice {
            duration: 37,
            caption: FormattedText::default(),
        },
        "Стикер" => MessageContent::Sticker {
            emoji: "🐱".into()
        },
        "Фото · Рассвет над заливом" => {
            h.photo(Picture::Landscape { seed: 2 }, plain("Рассвет над заливом"))
        }
        _ => text(last_text),
    };
    h.push(base, who, content);
}

/// The showcase group: formatting, media, replies, service messages, several days.
fn showcase(h: &mut History<'_>) {
    let anna = Who::Them("Анна Смирнова");
    let igor = Who::Them("Игорь Лебедев");
    let maria = Who::Them("Мария Волкова");
    let now = now();

    h.push(
        day_at(4, 10, 2),
        Who::Me,
        MessageContent::Service("Даниил Орлов создал группу «Команда Telegradus»".into()),
    );
    h.push(
        day_at(4, 10, 3),
        anna,
        MessageContent::Service("Анна Смирнова присоединилась к группе".into()),
    );
    h.push(
        day_at(4, 10, 5),
        anna,
        MessageContent::Text(rich(
            "Всем привет! Кидаю сюда ссылку на макеты: figma.com/file/telegradus — там все экраны входа и главное окно.",
            &[("figma.com/file/telegradus", TextEntityKind::Url)],
        )),
    );
    h.push(
        day_at(4, 10, 21),
        Who::Me,
        text("Супер, посмотрю вечером 👀"),
    );

    h.push(
        day_at(3, 9, 40),
        igor,
        MessageContent::Text(rich(
            "Архитектура: ядро на TDLib живёт в отдельном потоке, интерфейс общается с ним только через команды и события. Никаких блокировок в UI-потоке и никаких unwrap() на данных из сети.",
            &[
                ("Архитектура:", TextEntityKind::Bold),
                ("команды", TextEntityKind::Italic),
                ("события", TextEntityKind::Italic),
                ("unwrap()", TextEntityKind::Code),
            ],
        )),
    );
    let code = "Минимальный цикл событий:\nloop {\n    let event = events.recv().await?;\n    app.update(event);\n}\nВсё остальное — детали.";
    let pre = "loop {\n    let event = events.recv().await?;\n    app.update(event);\n}";
    h.push(
        day_at(3, 9, 52),
        Who::Me,
        MessageContent::Text(rich(
            code,
            &[(
                pre,
                TextEntityKind::Pre {
                    language: "rust".into(),
                },
            )],
        )),
    );
    let photo = h.photo(
        Picture::Landscape { seed: 0 },
        plain("Новая палитра — только чёрно-белое, а цвет оставим фотографиям 🖤🤍"),
    );
    let photo_id = h.push(day_at(3, 11, 15), anna, photo);
    let doc = h.document("Архитектура Telegradus.pdf", 2_516_582);
    h.push(day_at(3, 11, 30), igor, doc);
    h.reply(
        day_at(3, 11, 34),
        Who::Me,
        MessageContent::Text(rich(
            "Выглядит очень чисто. Синий акцент больше не нужен.",
            &[("Синий акцент", TextEntityKind::Strikethrough)],
        )),
        photo_id,
    );

    h.push(
        day_at(1, 10, 0),
        maria,
        MessageContent::Service("Мария Волкова присоединилась к группе".into()),
    );
    h.push(
        day_at(1, 10, 1),
        maria,
        MessageContent::Sticker {
            emoji: "👋".into()
        },
    );
    h.push(
        day_at(1, 10, 3),
        maria,
        MessageContent::Voice {
            duration: 17,
            caption: FormattedText::default(),
        },
    );
    let call = h.push(
        day_at(1, 16, 45),
        anna,
        MessageContent::Text(rich(
            "Кто завтра на созвоне в 11:00? Напишите @dorlov, если не сможете.",
            &[("@dorlov", TextEntityKind::Mention)],
        )),
    );
    h.reply(day_at(1, 16, 50), Who::Me, text("Буду 👍"), call);
    h.push(
        day_at(1, 18, 20),
        igor,
        MessageContent::Video {
            duration: 42,
            caption: plain("Демо прокрутки списка из 10 000 чатов — 60 кадров без просадок"),
        },
    );
    let notes = h.document("release-notes.md", 18_432);
    h.push(day_at(1, 19, 5), anna, notes);

    let minutes = |m: i64| now - m * 60;
    h.push(
        minutes(95),
        anna,
        MessageContent::Text(rich(
            "Собрала релизные заметки. Спойлер: будет тёмная тема 😉",
            &[("будет тёмная тема", TextEntityKind::Spoiler)],
        )),
    );
    h.push(
        minutes(80),
        igor,
        MessageContent::Text(rich(
            "Не забудьте прогнать cargo clippy --all-targets перед пушем. Подробности в CONTRIBUTING, раздел «Стиль кода». #релиз",
            &[
                ("cargo clippy --all-targets", TextEntityKind::Code),
                (
                    "CONTRIBUTING",
                    TextEntityKind::TextUrl {
                        url: "https://github.com/ilovevodkaa/Telegradus".into(),
                    },
                ),
                ("«Стиль кода»", TextEntityKind::Underline),
                ("#релиз", TextEntityKind::Hashtag),
            ],
        )),
    );
    let city = h.photo(Picture::City { seed: 7 }, FormattedText::default());
    let city_id = h.push(minutes(64), maria, city);
    h.reply(
        minutes(62),
        anna,
        text("Какой вид! Это из нового офиса?"),
        city_id,
    );
    h.push(
        minutes(50),
        igor,
        MessageContent::Text(rich(
            "Преждевременная оптимизация — корень всех зол.\nНо у нас оптимизация вполне своевременная 😄",
            &[("Преждевременная оптимизация — корень всех зол.", TextEntityKind::Blockquote)],
        )),
    );
    let long = "Небольшой отчёт за неделю:\n\n1. Виртуализированный список чатов — рисуем только видимые строки, память не растёт с количеством чатов.\n2. Кэш изображений по FileId — аватарки и фото декодируются один раз.\n3. История сообщений подгружается страницами при прокрутке вверх.\n4. Выход из приложения корректно закрывает TDLib.\n\nСледующий шаг — поиск и папки. Если есть идеи, пишите сюда.";
    h.push(
        minutes(35),
        Who::Me,
        MessageContent::Text(rich(
            long,
            &[("Небольшой отчёт за неделю:", TextEntityKind::Bold)],
        )),
    );
    if let Some(last) = h.messages.last_mut() {
        last.edit_date = minutes(30);
    }
    h.push(
        minutes(9),
        anna,
        MessageContent::Text(rich(
            "@dorlov посмотри, пожалуйста, PR #42 — там виртуализация списка чатов",
            &[
                ("@dorlov", TextEntityKind::Mention),
                ("#42", TextEntityKind::Hashtag),
            ],
        )),
    );
    h.push(minutes(3), maria, text("Поддерживаю! Отличная работа 🔥"));
}

/// Older messages served by `LoadOlder` for the showcase chat.
fn filler_page(chat_id: ChatId, page: i64) -> Vec<Message> {
    const LINES: [(&str, &str); 6] = [
        ("Анна Смирнова", "Обсудим итоги спринта?"),
        ("Игорь Лебедев", "Да, давайте в пятницу"),
        ("Мария Волкова", "Я подготовлю демо"),
        ("Даниил Орлов", "Отлично, тогда фиксируем"),
        ("Анна Смирнова", "Добавила задачи на доску"),
        ("Игорь Лебедев", "Взял себе две про кэширование"),
    ];
    let first_id = 70 - (page + 1) * 30;
    (0..30)
        .map(|i| {
            let (name, line) = LINES[(i as usize + page as usize) % LINES.len()];
            let outgoing = name == "Даниил Орлов";
            Message {
                id: first_id + i,
                chat_id,
                sender: Sender::User(if outgoing { ME } else { user_id(name) }),
                sender_name: name.to_owned(),
                date: day_at(6 + page * 2, 9 + (i / 4) as u32, (i % 4 * 12) as u32),
                edit_date: 0,
                is_outgoing: outgoing,
                sending_state: SendingState::Sent,
                reply_to: None,
                content: text(line),
            }
        })
        .collect()
}

/// Newest message id of a history (0 if empty).
pub fn last_id(messages: &[Message]) -> MessageId {
    messages.last().map_or(0, |m| m.id)
}

/// Shared, immutable folder list for events.
pub fn folders(world: &World) -> Arc<[Folder]> {
    world.folders.iter().map(|(f, _)| f.clone()).collect()
}
