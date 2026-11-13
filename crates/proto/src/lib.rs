//! The wire protocol spoken between the desktop and the phone.
//!
//! Both ends link this crate, so a message that one side can write is by
//! construction one the other side can read. The transport is a Bluetooth
//! RFCOMM stream; nothing here does I/O. Bytes go into [`Codec::feed`], whole
//! messages come out of [`Codec::next`], and [`encode`] turns a message back
//! into a frame.
//!
//! A frame is a 4-byte big-endian length followed by that many bytes of JSON.

use serde::{Deserialize, Serialize};

/// The RFCOMM service the phone listens on and the desktop connects to.
pub const SERVICE_UUID: &str = "7d3c1a52-9b0e-4f6a-8e21-6f6d616c696e";

/// Bumped whenever a change would confuse an older peer.
pub const PROTOCOL_VERSION: u32 = 1;

/// Larger frames are treated as a corrupt stream rather than allocated.
pub const MAX_FRAME: usize = 8 * 1024 * 1024;

/// How a file crosses the network.
///
/// Bluetooth is too slow for files, so it only arranges the transfer: the
/// phone opens a TCP port for one file and tells the desktop where it is and
/// the key to use. The phone is always the one listening, because desktops
/// commonly firewall incoming connections and phones don't.
///
/// The stream is a run of records, each a 4-byte big-endian length followed
/// by that many bytes of AES-256-GCM output (ciphertext and tag). A record's
/// nonce is four zero bytes then its index, counting from zero, as eight
/// big-endian bytes; a key is used for one file in one direction, so no
/// nonce repeats. A record of no plaintext ends the file, which is how a
/// connection cut short is told from a finished one.
pub mod transfer {
    /// Plaintext bytes per record, except the last two.
    pub const CHUNK: usize = 64 * 1024;
    pub const TAG: usize = 16;
}

/// What the phone needs the user's consent for. The names are ours, not
/// Android's, so the desktop can explain each one in plain words.
pub mod permission {
    pub const SMS: &str = "sms";
    pub const CONTACTS: &str = "contacts";
    pub const CALL_LOG: &str = "call_log";
    pub const PHONE: &str = "phone";
    pub const NOTIFICATIONS: &str = "notifications";
    pub const POST_NOTIFICATIONS: &str = "post_notifications";
    pub const BLUETOOTH: &str = "bluetooth";
    pub const BATTERY: &str = "battery";
    pub const WALLPAPER: &str = "wallpaper";
    pub const ALL: &[&str] = &[
        BLUETOOTH,
        POST_NOTIFICATIONS,
        NOTIFICATIONS,
        SMS,
        CONTACTS,
        CALL_LOG,
        PHONE,
        BATTERY,
        WALLPAPER,
    ];
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PhoneStatus {
    /// 0–100, or -1 when the phone doesn't say.
    pub battery: i32,
    pub charging: bool,
    /// 0–4 bars, or -1 when unknown.
    pub signal: i32,
    #[serde(default)]
    pub carrier: String,
    #[serde(default)]
    pub ringer: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Notification {
    /// Android's notification key; stable for the life of the notification.
    pub key: String,
    pub package: String,
    pub app: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub text: String,
    /// Milliseconds since the Unix epoch.
    pub time: i64,
    /// Labels of the notification's buttons, in order.
    #[serde(default)]
    pub actions: Vec<String>,
    /// Whether the phone can send a typed reply through this notification.
    #[serde(default)]
    pub can_reply: bool,
    #[serde(default)]
    pub clearable: bool,
    /// A small PNG of the app icon, base64, or empty.
    #[serde(default)]
    pub icon: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Conversation {
    pub thread_id: i64,
    pub address: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub snippet: String,
    pub time: i64,
    #[serde(default)]
    pub unread: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Sms {
    pub id: i64,
    pub thread_id: i64,
    pub address: String,
    pub body: String,
    pub time: i64,
    /// True for messages the user sent.
    pub outgoing: bool,
    #[serde(default)]
    pub read: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CallKind {
    #[default]
    Incoming,
    Outgoing,
    Missed,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct CallEntry {
    pub id: i64,
    pub number: String,
    #[serde(default)]
    pub name: String,
    pub kind: CallKind,
    pub time: i64,
    /// Seconds.
    #[serde(default)]
    pub duration: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Contact {
    pub name: String,
    pub numbers: Vec<String>,
    /// The phone's id for the contact's picture, which changes when the
    /// picture does; 0 for none. Fetched with `GetContactPhotos`.
    #[serde(default)]
    pub photo: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Photo {
    pub id: i64,
    /// A small JPEG, base64.
    pub data: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CallPhase {
    #[default]
    Idle,
    Ringing,
    Active,
}

/// Every message either side can send. Tagged as `{"t": "...", "d": {...}}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", content = "d", rename_all = "snake_case")]
pub enum Message {
    // Both directions.
    Hello {
        version: u32,
        name: String,
    },
    Ping {
        n: u64,
    },
    Pong {
        n: u64,
    },
    /// Text copied on one side, to be placed on the other's clipboard.
    Clipboard {
        text: String,
    },

    // Phone to desktop.
    Status(PhoneStatus),
    /// Sent on connect and whenever the user grants or revokes something.
    Permissions {
        granted: Vec<String>,
        missing: Vec<String>,
    },
    NotificationPosted(Notification),
    NotificationRemoved {
        key: String,
    },
    /// The complete current set, replacing whatever the desktop holds.
    Notifications {
        items: Vec<Notification>,
    },
    Conversations {
        items: Vec<Conversation>,
    },
    Messages {
        thread_id: i64,
        items: Vec<Sms>,
    },
    SmsSent {
        /// Echoes the `id` of the `SendSms` it answers.
        id: u64,
        ok: bool,
        #[serde(default)]
        error: String,
    },
    CallLog {
        items: Vec<CallEntry>,
    },
    Contacts {
        items: Vec<Contact>,
    },
    /// Answers `GetContactPhotos`, a few pictures at a time.
    ContactPhotos {
        items: Vec<Photo>,
    },
    /// The phone has a file for the desktop, waiting at any of `addrs` on
    /// `port`. `key` is 32 bytes, base64. See [`transfer`].
    FileOffer {
        id: u64,
        name: String,
        size: i64,
        port: u16,
        addrs: Vec<String>,
        key: String,
    },
    /// Answers `SendFile`: where the desktop should write the file to.
    FileReady {
        id: u64,
        port: u16,
        addrs: Vec<String>,
        key: String,
    },
    /// How a `SendFile` ended on the phone.
    FileSaved {
        id: u64,
        ok: bool,
        #[serde(default)]
        error: String,
    },
    /// The home screen wallpaper as a JPEG, base64, cropped to the screen.
    Wallpaper {
        image: String,
    },
    CallState {
        phase: CallPhase,
        #[serde(default)]
        number: String,
        #[serde(default)]
        name: String,
    },
    /// The phone could not do what was asked, and why.
    Failed {
        what: String,
        reason: String,
    },

    // Desktop to phone.
    /// Ask the phone to put up its permission prompt for these.
    RequestPermissions {
        permissions: Vec<String>,
    },
    /// Ask for fresh copies: any of "status", "permissions", "notifications",
    /// "conversations", "call_log", "contacts", "wallpaper".
    Sync {
        what: Vec<String>,
    },
    GetMessages {
        thread_id: i64,
        limit: u32,
    },
    SendSms {
        id: u64,
        to: String,
        body: String,
    },
    MarkRead {
        thread_id: i64,
    },
    GetContactPhotos {
        ids: Vec<i64>,
    },
    /// The desktop has a file for the phone; answered with `FileReady`.
    SendFile {
        id: u64,
        name: String,
        size: i64,
    },
    DismissNotification {
        key: String,
    },
    ReplyNotification {
        key: String,
        text: String,
    },
    NotificationAction {
        key: String,
        index: u32,
    },
    Dial {
        number: String,
    },
    AnswerCall,
    EndCall,
    /// Make the phone ring loudly so it can be found.
    FindPhone,
}

#[derive(Debug, PartialEq, Eq)]
pub enum DecodeError {
    /// The length prefix is larger than [`MAX_FRAME`]; the stream is unusable.
    TooLarge(usize),
    /// A whole frame arrived but was not a message this version knows. The
    /// stream is still in sync, so the caller can skip it and carry on.
    Unknown(String),
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecodeError::TooLarge(n) => write!(f, "frame of {n} bytes exceeds the limit"),
            DecodeError::Unknown(e) => write!(f, "unreadable message: {e}"),
        }
    }
}

impl std::error::Error for DecodeError {}

/// Frames a message for the wire.
pub fn encode(message: &Message) -> Vec<u8> {
    let body = serde_json::to_vec(message).expect("messages always serialize");
    frame(&body)
}

/// Frames JSON that is already known to be a valid message.
pub fn frame(body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len() + 4);
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    out.extend_from_slice(body);
    out
}

/// Reassembles messages from however the transport happens to chunk them.
#[derive(Default)]
pub struct Codec {
    buffer: Vec<u8>,
}

impl Codec {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        self.buffer.extend_from_slice(bytes);
    }

    /// The next whole frame's JSON, without interpreting it.
    pub fn next_raw(&mut self) -> Result<Option<Vec<u8>>, DecodeError> {
        if self.buffer.len() < 4 {
            return Ok(None);
        }
        let length = u32::from_be_bytes([self.buffer[0], self.buffer[1], self.buffer[2], self.buffer[3]]) as usize;
        if length > MAX_FRAME {
            return Err(DecodeError::TooLarge(length));
        }
        if self.buffer.len() < 4 + length {
            return Ok(None);
        }
        let body = self.buffer[4..4 + length].to_vec();
        self.buffer.drain(..4 + length);
        Ok(Some(body))
    }

    /// The next whole message, `Ok(None)` when more bytes are needed.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Result<Option<Message>, DecodeError> {
        match self.next_raw()? {
            None => Ok(None),
            Some(body) => serde_json::from_slice(&body)
                .map(Some)
                .map_err(|e| DecodeError::Unknown(e.to_string())),
        }
    }
}

/// Reduces a phone number to what identifies it, so "+1 (555) 010-9999" and
/// "5550109999" land on the same contact. Compares the last ten digits, which
/// is what dialers do when one side carries a country code and the other
/// doesn't.
pub fn number_key(number: &str) -> String {
    let digits: String = number.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() > 10 {
        digits[digits.len() - 10..].to_string()
    } else if digits.is_empty() {
        // Alphanumeric sender ids ("AMAZON") have no digits to compare.
        number.trim().to_lowercase()
    } else {
        digits
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_arbitrary_chunking() {
        let messages = vec![
            Message::Hello { version: PROTOCOL_VERSION, name: "desk".into() },
            Message::Clipboard { text: "héllo ✓".into() },
            Message::AnswerCall,
            Message::Status(PhoneStatus { battery: 81, charging: true, signal: 3, ..Default::default() }),
            Message::SendSms { id: 7, to: "+15550100".into(), body: "on my way".into() },
        ];
        let wire: Vec<u8> = messages.iter().flat_map(encode).collect();
        for chunk in [1, 3, 7, wire.len()] {
            let mut codec = Codec::new();
            let mut seen = Vec::new();
            for piece in wire.chunks(chunk) {
                codec.feed(piece);
                while let Some(message) = codec.next().unwrap() {
                    seen.push(message);
                }
            }
            assert_eq!(seen, messages, "chunk size {chunk}");
        }
    }

    #[test]
    fn an_unknown_message_does_not_lose_sync() {
        let mut codec = Codec::new();
        codec.feed(&frame(br#"{"t":"from_the_future","d":{}}"#));
        codec.feed(&encode(&Message::EndCall));
        assert!(matches!(codec.next(), Err(DecodeError::Unknown(_))));
        assert_eq!(codec.next(), Ok(Some(Message::EndCall)));
    }

    #[test]
    fn an_absurd_length_is_refused() {
        let mut codec = Codec::new();
        codec.feed(&[0xff, 0xff, 0xff, 0xff, 1, 2, 3]);
        assert!(matches!(codec.next(), Err(DecodeError::TooLarge(_))));
    }

    #[test]
    fn unit_variants_have_a_stable_shape() {
        let json = serde_json::to_string(&Message::FindPhone).unwrap();
        assert_eq!(json, r#"{"t":"find_phone"}"#);
        let json = serde_json::to_string(&Message::Dial { number: "123".into() }).unwrap();
        assert_eq!(json, r#"{"t":"dial","d":{"number":"123"}}"#);
    }

    #[test]
    fn numbers_match_across_formats() {
        assert_eq!(number_key("+1 (555) 010-9999"), number_key("5550109999"));
        assert_eq!(number_key("AMAZON"), "amazon");
        assert_eq!(number_key("12345"), "12345");
    }
}
