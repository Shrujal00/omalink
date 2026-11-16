//! Icon verification: the desktop shows one icon, the phone shows three, and
//! the person taps the one that matches.
//!
//! It runs whenever two devices first trust each other over the network, and
//! again before every screen-mirroring session. The network connection is
//! already encrypted and tied to each side's key; the icon proves those keys
//! are the ones on the two screens in front of the person, not a stranger's
//! in between.
//!
//! An icon is only six bits, which an attacker could find by trying keys, so
//! neither side may choose its contribution after seeing the other's. The
//! phone commits to a random nonce first, the desktop answers with its own,
//! and only then does the phone reveal its nonce:
//!
//! ```text
//! phone                                  desktop
//!   VerifyCommit { H(phone_nonce) }  ──►
//!                                    ◄──  VerifyNonce { desktop_nonce }
//!   VerifyReveal { phone_nonce }     ──►  checks it against the commitment
//!   both derive the icon from the two ids and the two nonces
//!   (the person taps one of three on the phone)
//!   VerifyResult { ok }              ──►
//!                                    ◄──  VerifyDone { ok }
//! ```
//!
//! Someone in the middle then matches the two screens with probability 1/64
//! per attempt, and every failed attempt is visible to the person.

use sha2::{Digest, Sha256};

/// The icons, as emoji with a name for screen readers. Single code points
/// only, so they draw the same on every emoji font.
pub const ICONS: [(&str, &str); 64] = [
    ("🐶", "dog"),
    ("🐱", "cat"),
    ("🦊", "fox"),
    ("🐻", "bear"),
    ("🐼", "panda"),
    ("🐨", "koala"),
    ("🐯", "tiger"),
    ("🦁", "lion"),
    ("🐮", "cow"),
    ("🐷", "pig"),
    ("🐸", "frog"),
    ("🐵", "monkey"),
    ("🐔", "chicken"),
    ("🐧", "penguin"),
    ("🦉", "owl"),
    ("🐴", "horse"),
    ("🦄", "unicorn"),
    ("🐝", "bee"),
    ("🐢", "turtle"),
    ("🐙", "octopus"),
    ("🦀", "crab"),
    ("🐬", "dolphin"),
    ("🐳", "whale"),
    ("🦈", "shark"),
    ("🐘", "elephant"),
    ("🦒", "giraffe"),
    ("🦓", "zebra"),
    ("🦋", "butterfly"),
    ("🐌", "snail"),
    ("🐞", "ladybug"),
    ("🌵", "cactus"),
    ("🌲", "tree"),
    ("🌻", "sunflower"),
    ("🌹", "rose"),
    ("🍄", "mushroom"),
    ("🍀", "clover"),
    ("🍁", "maple leaf"),
    ("🌙", "moon"),
    ("🌟", "star"),
    ("🔥", "fire"),
    ("🌈", "rainbow"),
    ("🍎", "apple"),
    ("🍋", "lemon"),
    ("🍉", "watermelon"),
    ("🍇", "grapes"),
    ("🍓", "strawberry"),
    ("🍒", "cherries"),
    ("🍍", "pineapple"),
    ("🥕", "carrot"),
    ("🌽", "corn"),
    ("🍕", "pizza"),
    ("🍔", "burger"),
    ("🍩", "doughnut"),
    ("🎂", "cake"),
    ("🍪", "cookie"),
    ("🎈", "balloon"),
    ("🎁", "gift"),
    ("🏀", "basketball"),
    ("🎸", "guitar"),
    ("🚀", "rocket"),
    ("🚲", "bicycle"),
    ("🔑", "key"),
    ("💡", "light bulb"),
    ("📷", "camera"),
];

/// Bytes in each side's nonce.
pub const NONCE: usize = 32;

/// What a verification is for. Mixed into the icon, so an icon shown for
/// pairing can't be replayed to approve a mirroring session.
pub const PAIR: &str = "pair";
pub const MIRROR: &str = "mirror";

/// What both screens show for one verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sas {
    /// Index into [`ICONS`]: the one the desktop shows.
    pub icon: usize,
    /// What the phone offers, in the order it shows them. Exactly one is
    /// `icon`; the other two differ from it and from each other.
    pub choices: [usize; 3],
}

/// The phone's commitment to its nonce.
pub fn commit(phone_nonce: &[u8]) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"omalink-commit-v1");
    hash.update(phone_nonce);
    hash.finalize().into()
}

/// Derives the icon and the phone's three choices. The ids are the two
/// devices' public keys as the connection authenticated them, so each side
/// computes this from what it saw, never from what the other claims.
pub fn sas(purpose: &str, desktop_id: &[u8], phone_id: &[u8], phone_nonce: &[u8], desktop_nonce: &[u8]) -> Sas {
    let mut hash = Sha256::new();
    hash.update(b"omalink-sas-v1");
    for part in [purpose.as_bytes(), desktop_id, phone_id, phone_nonce, desktop_nonce] {
        hash.update((part.len() as u32).to_be_bytes());
        hash.update(part);
    }
    let digest: [u8; 32] = hash.finalize().into();

    // 64 divides 256, so taking six bits of a byte is unbiased.
    let icon = (digest[0] % 64) as usize;
    let mut decoys = Vec::with_capacity(2);
    for &byte in &digest[1..31] {
        let candidate = (byte % 64) as usize;
        if candidate != icon && !decoys.contains(&candidate) {
            decoys.push(candidate);
            if decoys.len() == 2 {
                break;
            }
        }
    }
    // Thirty bytes all landing on two values is not a real possibility, but
    // the result must still be three different icons.
    let mut next = (icon + 1) % 64;
    while decoys.len() < 2 {
        if !decoys.contains(&next) {
            decoys.push(next);
        }
        next = (next + 1) % 64;
    }
    let mut choices = [decoys[0], decoys[1], decoys[0]];
    let slot = (digest[31] % 3) as usize;
    let mut d = decoys.into_iter();
    for (i, choice) in choices.iter_mut().enumerate() {
        *choice = if i == slot { icon } else { d.next().unwrap_or(icon) };
    }
    Sas { icon, choices }
}

/// Lowercase hex, for nonces and commitments inside JSON messages.
pub fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn from_hex(text: &str) -> Option<Vec<u8>> {
    if text.len() % 2 != 0 {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| text.get(i..i + 2).and_then(|pair| u8::from_str_radix(pair, 16).ok()))
        .collect()
}

/// The desktop's half: it shows the icon and checks the phone's answer.
#[derive(Debug)]
pub struct Shower {
    pub purpose: String,
    desktop_id: Vec<u8>,
    phone_id: Vec<u8>,
    commitment: Vec<u8>,
    desktop_nonce: Vec<u8>,
    sas: Option<Sas>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum VerifyError {
    /// A message that doesn't belong at this step.
    OutOfOrder,
    /// The revealed nonce isn't the one the phone committed to.
    BadReveal,
    Malformed,
}

impl Shower {
    /// Takes the phone's `VerifyCommit` and the desktop's fresh random
    /// nonce; the caller sends `VerifyNonce { nonce: to_hex(desktop_nonce) }`.
    pub fn new(purpose: &str, desktop_id: &[u8], phone_id: &[u8], commitment_hex: &str, desktop_nonce: &[u8]) -> Result<Self, VerifyError> {
        let commitment = from_hex(commitment_hex).filter(|c| c.len() == 32).ok_or(VerifyError::Malformed)?;
        if desktop_nonce.len() != NONCE {
            return Err(VerifyError::Malformed);
        }
        Ok(Self {
            purpose: purpose.to_string(),
            desktop_id: desktop_id.to_vec(),
            phone_id: phone_id.to_vec(),
            commitment,
            desktop_nonce: desktop_nonce.to_vec(),
            sas: None,
        })
    }

    /// Takes the phone's `VerifyReveal`; returns the icon to show.
    pub fn reveal(&mut self, phone_nonce_hex: &str) -> Result<Sas, VerifyError> {
        if self.sas.is_some() {
            return Err(VerifyError::OutOfOrder);
        }
        let nonce = from_hex(phone_nonce_hex).filter(|n| n.len() == NONCE).ok_or(VerifyError::Malformed)?;
        if commit(&nonce)[..] != self.commitment[..] {
            return Err(VerifyError::BadReveal);
        }
        let sas = sas(&self.purpose, &self.desktop_id, &self.phone_id, &nonce, &self.desktop_nonce);
        self.sas = Some(sas);
        Ok(sas)
    }

    /// The icon on screen, once revealed.
    pub fn icon(&self) -> Option<usize> {
        self.sas.map(|s| s.icon)
    }
}

/// The phone's half: it commits, reveals, and offers three icons.
#[derive(Debug)]
pub struct Chooser {
    pub purpose: String,
    desktop_id: Vec<u8>,
    phone_id: Vec<u8>,
    phone_nonce: Vec<u8>,
    sas: Option<Sas>,
}

impl Chooser {
    /// Takes the phone's fresh random nonce; the caller sends
    /// `VerifyCommit { purpose, commit: chooser.commitment() }`.
    pub fn new(purpose: &str, desktop_id: &[u8], phone_id: &[u8], phone_nonce: &[u8]) -> Result<Self, VerifyError> {
        if phone_nonce.len() != NONCE {
            return Err(VerifyError::Malformed);
        }
        Ok(Self {
            purpose: purpose.to_string(),
            desktop_id: desktop_id.to_vec(),
            phone_id: phone_id.to_vec(),
            phone_nonce: phone_nonce.to_vec(),
            sas: None,
        })
    }

    pub fn commitment(&self) -> String {
        to_hex(&commit(&self.phone_nonce))
    }

    /// Takes the desktop's `VerifyNonce`; returns the reveal to send and the
    /// three icons to offer.
    pub fn nonce(&mut self, desktop_nonce_hex: &str) -> Result<(String, Sas), VerifyError> {
        if self.sas.is_some() {
            return Err(VerifyError::OutOfOrder);
        }
        let desktop_nonce = from_hex(desktop_nonce_hex).filter(|n| n.len() == NONCE).ok_or(VerifyError::Malformed)?;
        let sas = sas(&self.purpose, &self.desktop_id, &self.phone_id, &self.phone_nonce, &desktop_nonce);
        self.sas = Some(sas);
        Ok((to_hex(&self.phone_nonce), sas))
    }

    /// Whether the icon the person tapped is the right one.
    pub fn check(&self, tapped: usize) -> bool {
        self.sas.is_some_and(|s| s.icon == tapped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DESKTOP: [u8; 32] = [1; 32];
    const PHONE: [u8; 32] = [2; 32];

    fn run(purpose: &str, pn: [u8; 32], dn: [u8; 32]) -> (Sas, Sas) {
        let mut chooser = Chooser::new(purpose, &DESKTOP, &PHONE, &pn).unwrap();
        let mut shower = Shower::new(purpose, &DESKTOP, &PHONE, &chooser.commitment(), &dn).unwrap();
        let (reveal, offered) = chooser.nonce(&to_hex(&dn)).unwrap();
        let shown = shower.reveal(&reveal).unwrap();
        (offered, shown)
    }

    #[test]
    fn both_sides_agree_and_the_icon_is_offered_once() {
        for seed in 0..=255u8 {
            let (offered, shown) = run(PAIR, [seed; 32], [seed.wrapping_mul(7); 32]);
            assert_eq!(offered, shown);
            assert_eq!(offered.choices.iter().filter(|&&c| c == offered.icon).count(), 1);
            let [a, b, c] = offered.choices;
            assert!(a != b && b != c && a != c, "{:?}", offered.choices);
            assert!(offered.choices.iter().all(|&c| c < ICONS.len()));
        }
    }

    #[test]
    fn the_right_icon_lands_in_every_position() {
        let mut slots = [0; 3];
        for seed in 0..=255u8 {
            let (sas, _) = run(PAIR, [seed; 32], [3; 32]);
            slots[sas.choices.iter().position(|&c| c == sas.icon).unwrap()] += 1;
        }
        assert!(slots.iter().all(|&n| n > 40), "{slots:?}");
    }

    #[test]
    fn purpose_and_ids_change_the_icon_set() {
        let a = sas(PAIR, &DESKTOP, &PHONE, &[5; 32], &[6; 32]);
        let b = sas(MIRROR, &DESKTOP, &PHONE, &[5; 32], &[6; 32]);
        let c = sas(PAIR, &DESKTOP, &[9; 32], &[5; 32], &[6; 32]);
        assert!(a != b || a != c);
        assert_eq!(a, sas(PAIR, &DESKTOP, &PHONE, &[5; 32], &[6; 32]));
    }

    #[test]
    fn a_reveal_that_breaks_the_commitment_is_refused() {
        let chooser = Chooser::new(PAIR, &DESKTOP, &PHONE, &[7; 32]).unwrap();
        let mut shower = Shower::new(PAIR, &DESKTOP, &PHONE, &chooser.commitment(), &[8; 32]).unwrap();
        assert_eq!(shower.reveal(&to_hex(&[9; 32])), Err(VerifyError::BadReveal));
        assert_eq!(shower.icon(), None);
    }

    #[test]
    fn a_second_reveal_is_refused() {
        let mut chooser = Chooser::new(PAIR, &DESKTOP, &PHONE, &[7; 32]).unwrap();
        let mut shower = Shower::new(PAIR, &DESKTOP, &PHONE, &chooser.commitment(), &[8; 32]).unwrap();
        let (reveal, _) = chooser.nonce(&to_hex(&[8; 32])).unwrap();
        shower.reveal(&reveal).unwrap();
        assert_eq!(shower.reveal(&reveal), Err(VerifyError::OutOfOrder));
    }

    #[test]
    fn only_the_right_tap_passes() {
        let mut chooser = Chooser::new(MIRROR, &DESKTOP, &PHONE, &[1; 32]).unwrap();
        let (_, sas) = chooser.nonce(&to_hex(&[2; 32])).unwrap();
        for &choice in &sas.choices {
            assert_eq!(chooser.check(choice), choice == sas.icon);
        }
    }

    #[test]
    fn malformed_input_is_refused() {
        assert!(Shower::new(PAIR, &DESKTOP, &PHONE, "zz", &[0; 32]).is_err());
        assert!(Chooser::new(PAIR, &DESKTOP, &PHONE, &[0; 3]).is_err());
        assert_eq!(from_hex("abc"), None);
        assert_eq!(from_hex("0aff"), Some(vec![10, 255]));
    }
}
