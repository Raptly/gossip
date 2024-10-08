use nostr_types::Unixtime;
use speedy::{Readable, Writable};

#[derive(Debug, Clone, PartialEq, Eq, Readable, Writable)]
pub struct PersonListMetadata1 {
    pub dtag: String,
    pub title: String,
    pub last_edit_time: Unixtime,
    pub event_created_at: Unixtime,
    pub event_public_len: usize,
    pub event_private_len: Option<usize>,
}

impl Default for PersonListMetadata1 {
    fn default() -> PersonListMetadata1 {
        PersonListMetadata1 {
            dtag: "".to_owned(),
            title: "".to_owned(),
            last_edit_time: Unixtime::now(),
            event_created_at: Unixtime(0),
            event_public_len: 0,
            event_private_len: None,
        }
    }
}
