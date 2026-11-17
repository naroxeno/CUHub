//! Room code (C1) encoding and host discovery by hostname. No processes, no network, no UI.

use std::fmt;
use std::net::Ipv4Addr;

use aes_gcm::aead::OsRng;
use aes_gcm::aead::rand_core::RngCore;

use crate::easytier::EasyTierMember;

const ALPHABET: &[u8; 32] = b"23456789ABCDEFGHJKLMNPQRSTUVWXYZ";
const VERSION: &str = "C1";
const NAME_LEN: usize = 8;
const SECRET_LEN: usize = 7;
const PAYLOAD_LEN: usize = NAME_LEN + SECRET_LEN;
const CODE_LEN: usize = PAYLOAD_LEN + 1;
const GROUP: usize = 4;
const NETWORK_PREFIX: &str = "cuhub-";
const HOST_PREFIX: &str = "cuhub-host-";

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct RoomCode {
    digits: [u8; CODE_LEN],
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct HostCandidate {
    pub hostname: String,
    pub address: Ipv4Addr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoomError {
    InvalidLength,
    InvalidChar,
    BadChecksum,
    UnsupportedVersion,
    Io(String),
}

impl fmt::Display for RoomError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLength => formatter.write_str("room code has the wrong length"),
            Self::InvalidChar => formatter.write_str("room code contains an unknown character"),
            Self::BadChecksum => formatter.write_str("room code checksum does not match"),
            Self::UnsupportedVersion => formatter.write_str("room code comes from a newer version"),
            Self::Io(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for RoomError {}

impl RoomCode {
    pub fn generate() -> Self {
        let mut bytes = [0u8; 16];
        OsRng.fill_bytes(&mut bytes);
        let mut value = u128::from_le_bytes(bytes) & ((1u128 << (PAYLOAD_LEN * 5)) - 1);
        let mut digits = [0u8; CODE_LEN];
        for digit in &mut digits[..PAYLOAD_LEN] {
            *digit = (value & 0x1f) as u8;
            value >>= 5;
        }
        digits[PAYLOAD_LEN] = checksum(&digits[..PAYLOAD_LEN]);
        Self { digits }
    }

    pub fn parse(input: &str) -> Result<Self, RoomError> {
        let mut cleaned = [0u8; VERSION.len() + CODE_LEN];
        let mut length = 0;
        for character in input.chars() {
            if character.is_whitespace() || character == '-' || character == '/' {
                continue;
            }
            let character = character.to_ascii_uppercase();
            if !character.is_ascii() {
                return Err(RoomError::InvalidChar);
            }
            if length == cleaned.len() {
                return Err(RoomError::InvalidLength);
            }
            cleaned[length] = character as u8;
            length += 1;
        }

        if length == CODE_LEN {
            return Self::from_payload(&cleaned[..CODE_LEN]);
        }
        if length >= 2 && cleaned[0] == b'C' && cleaned[1].is_ascii_digit() && cleaned[1] != b'1' {
            return Err(RoomError::UnsupportedVersion);
        }
        if length != cleaned.len() || &cleaned[..VERSION.len()] != VERSION.as_bytes() {
            return Err(RoomError::InvalidLength);
        }
        Self::from_payload(&cleaned[VERSION.len()..])
    }

    fn from_payload(bytes: &[u8]) -> Result<Self, RoomError> {
        let mut digits = [0u8; CODE_LEN];
        for (index, byte) in bytes.iter().enumerate() {
            digits[index] = decode(*byte).ok_or(RoomError::InvalidChar)?;
        }
        if checksum(&digits[..PAYLOAD_LEN]) != digits[PAYLOAD_LEN] {
            return Err(RoomError::BadChecksum);
        }
        Ok(Self { digits })
    }

    pub fn encode(&self) -> String {
        let mut code = String::with_capacity(VERSION.len() + CODE_LEN + CODE_LEN / GROUP);
        code.push_str(VERSION);
        for (index, digit) in self.digits.iter().enumerate() {
            if index % GROUP == 0 {
                code.push('-');
            }
            code.push(ALPHABET[*digit as usize] as char);
        }
        code
    }

    pub fn network_name(&self) -> String {
        let mut name = String::with_capacity(NETWORK_PREFIX.len() + NAME_LEN);
        name.push_str(NETWORK_PREFIX);
        for digit in &self.digits[..NAME_LEN] {
            name.push(ALPHABET[*digit as usize] as char);
        }
        name
    }

    pub fn network_secret(&self) -> String {
        let mut secret = String::with_capacity(SECRET_LEN);
        for digit in &self.digits[NAME_LEN..PAYLOAD_LEN] {
            secret.push(ALPHABET[*digit as usize] as char);
        }
        secret
    }
}

impl fmt::Debug for RoomCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = self.network_name();
        let shown = format!("{}…", &name[..NETWORK_PREFIX.len() + 4]);
        formatter
            .debug_struct("RoomCode")
            .field("network", &shown)
            .finish()
    }
}

pub fn host_hostname(port: u16) -> String {
    format!("{HOST_PREFIX}{port}")
}

pub fn find_host_candidates(peers: &[EasyTierMember], port: u16) -> Vec<HostCandidate> {
    if port == 0 {
        return Vec::new();
    }
    let hostname = host_hostname(port);
    let mut candidates: Vec<HostCandidate> = Vec::new();
    for member in peers {
        if member.is_local || member.hostname != hostname {
            continue;
        }
        let Some(address) = member.address else {
            continue;
        };
        if candidates.iter().any(|known| known.address == address) {
            continue;
        }
        candidates.push(HostCandidate {
            hostname: hostname.clone(),
            address,
        });
    }
    candidates
}

pub fn find_host(peers: &[EasyTierMember], port: u16) -> Option<HostCandidate> {
    let mut candidates = find_host_candidates(peers, port);
    if candidates.len() == 1 {
        candidates.pop()
    } else {
        None
    }
}

fn checksum(digits: &[u8]) -> u8 {
    digits
        .iter()
        .fold(0, |accumulated, digit| accumulated ^ digit)
}

fn decode(byte: u8) -> Option<u8> {
    ALPHABET
        .iter()
        .position(|known| *known == byte)
        .map(|index| index as u8)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::easytier::NatType;

    const SAMPLE: &str = "C1-7K3M-9Q2X-4H8P-V6RW";

    fn member(hostname: &str, address: &str, is_local: bool) -> EasyTierMember {
        EasyTierMember {
            hostname: hostname.to_owned(),
            address: Some(address.parse().unwrap()),
            is_local,
            nat: NatType::Unknown,
        }
    }

    fn replace_digit(index: usize, shift: u8) -> String {
        let mut characters: Vec<char> = SAMPLE
            .chars()
            .filter(|character| *character != '-')
            .collect();
        let offset = VERSION.len() + index;
        let current = ALPHABET
            .iter()
            .position(|known| *known as char == characters[offset])
            .unwrap();
        characters[offset] = ALPHABET[(current + shift as usize) % ALPHABET.len()] as char;
        let payload: String = characters[VERSION.len()..].iter().collect();
        format!(
            "{VERSION}-{}-{}-{}-{}",
            &payload[0..4],
            &payload[4..8],
            &payload[8..12],
            &payload[12..16]
        )
    }

    #[test]
    fn generated_codes_round_trip() {
        for _ in 0..64 {
            let code = RoomCode::generate();
            let encoded = code.encode();
            let parsed = RoomCode::parse(&encoded).unwrap();
            assert_eq!(parsed, code);
            assert_eq!(parsed.encode(), encoded);
            assert_eq!(parsed.network_name(), code.network_name());
            assert_eq!(parsed.network_secret(), code.network_secret());
        }
    }

    #[test]
    fn generated_codes_are_distinct() {
        let codes: Vec<RoomCode> = (0..32).map(|_| RoomCode::generate()).collect();
        for (index, code) in codes.iter().enumerate() {
            assert!(!codes[..index].contains(code));
        }
    }

    #[test]
    fn canonical_shape_and_derivations() {
        let code = RoomCode::parse(SAMPLE).unwrap();
        assert_eq!(code.encode(), SAMPLE);
        assert_eq!(code.encode().len(), 22);
        assert_eq!(code.network_name(), "cuhub-7K3M9Q2X");
        assert_eq!(code.network_name().len(), "cuhub-XXXXXXXX".len());
        assert_eq!(code.network_secret(), "4H8PV6R");
        assert_eq!(code.network_secret().len(), 7);
    }

    #[test]
    fn accepts_lowercase_and_handwritten_separators() {
        let expected = RoomCode::parse(SAMPLE).unwrap();
        for input in [
            "c1-7k3m-9q2x-4h8p-v6rw",
            "  C1 7K3M 9Q2X 4H8P V6RW  ",
            "c1/7k3m 9q2x-4h8p/v6rw",
            "C1\n7K3M\t9Q2X 4H8P V6RW",
        ] {
            assert_eq!(RoomCode::parse(input).unwrap(), expected, "{input:?}");
        }
    }

    #[test]
    fn rejects_every_single_character_change() {
        for index in 0..CODE_LEN {
            for shift in [1u8, 7, 16] {
                let changed = replace_digit(index, shift);
                assert_eq!(
                    RoomCode::parse(&changed).unwrap_err(),
                    RoomError::BadChecksum,
                    "{changed}"
                );
            }
        }
    }

    #[test]
    fn rejects_wrong_checksum() {
        assert_eq!(
            RoomCode::parse("C1-7K3M-9Q2X-4H8P-V6RD").unwrap_err(),
            RoomError::BadChecksum
        );
    }

    #[test]
    fn rejects_wrong_length() {
        for input in ["", "C1", "C1-7K3M-9Q2X-4H8P", "C1-7K3M-9Q2X-4H8P-V6RWX"] {
            assert_eq!(
                RoomCode::parse(input).unwrap_err(),
                RoomError::InvalidLength,
                "{input:?}"
            );
        }
    }

    #[test]
    fn rejects_invalid_characters() {
        for input in [
            "C1-0K3M-9Q2X-4H8P-V6RW",
            "C1-OK3M-9Q2X-4H8P-V6RW",
            "C1-1K3M-9Q2X-4H8P-V6RW",
            "C1-IK3M-9Q2X-4H8P-V6RW",
            "C1-码K3M-9Q2X-4H8P-V6RW",
        ] {
            assert_eq!(
                RoomCode::parse(input).unwrap_err(),
                RoomError::InvalidChar,
                "{input:?}"
            );
        }
    }

    #[test]
    fn reports_codes_from_a_newer_version() {
        for input in ["C2-7K3M-9Q2X-4H8P-V6RW", "c2/7k3m-9q2x-4h8p-v6rw", "C9"] {
            assert_eq!(
                RoomCode::parse(input).unwrap_err(),
                RoomError::UnsupportedVersion,
                "{input:?}"
            );
        }
    }

    #[test]
    fn debug_output_hides_the_secret() {
        let code = RoomCode::parse(SAMPLE).unwrap();
        let debug = format!("{code:?}");
        assert!(!debug.contains("4H8PV6R"));
        assert!(!debug.contains("9Q2X"));
        assert!(debug.contains("cuhub-7K3M…"));
    }

    #[test]
    fn host_hostname_uses_the_port() {
        assert_eq!(host_hostname(25565), "cuhub-host-25565");
        assert_eq!(host_hostname(1), "cuhub-host-1");
        assert_eq!(host_hostname(65535), "cuhub-host-65535");
    }

    #[test]
    fn finds_the_host_by_hostname() {
        let peers = [
            member("cuhub-host-25566", "10.42.0.8", false),
            member("cuhub-host-25565", "10.42.0.7", false),
            member("guest-laptop", "10.42.0.9", false),
        ];
        let found = find_host(&peers, 25565).unwrap();
        assert_eq!(found.hostname, "cuhub-host-25565");
        assert_eq!(found.address, Ipv4Addr::new(10, 42, 0, 7));
    }

    #[test]
    fn ignores_peers_on_another_port() {
        let peers = [
            member("cuhub-host-25566", "10.42.0.7", false),
            member("cuhub-host-1", "10.42.0.7", false),
        ];
        assert!(find_host(&peers, 25565).is_none());
    }

    #[test]
    fn ignores_similar_hostnames() {
        let peers = [
            member("cuhub-host-25565-alt", "10.42.0.7", false),
            member("cuhub-host-", "10.42.0.7", false),
            member("cuhub-host-abc", "10.42.0.7", false),
            member("cuhub-host-025565", "10.42.0.7", false),
            member("cuhub-host-25565 ", "10.42.0.7", false),
            member("cuhub-hostx-25565", "10.42.0.7", false),
            member("xcuhub-host-25565", "10.42.0.7", false),
        ];
        assert!(find_host(&peers, 25565).is_none());
    }

    #[test]
    fn ignores_the_local_member_and_members_without_an_address() {
        let peers = [
            member("cuhub-host-25565", "10.42.0.7", true),
            EasyTierMember {
                hostname: "cuhub-host-25565".to_owned(),
                address: None,
                is_local: false,
                nat: NatType::Unknown,
            },
        ];
        assert!(find_host(&peers, 25565).is_none());
    }

    #[test]
    fn refuses_to_choose_between_multiple_hosts() {
        let peers = [
            member("cuhub-host-25565", "10.42.0.7", false),
            member("cuhub-host-25565", "10.42.0.8", false),
        ];
        assert!(find_host(&peers, 25565).is_none());
    }

    #[test]
    fn accepts_a_code_without_the_version_prefix() {
        let code = RoomCode::parse(&SAMPLE.replace("C1-", "")).unwrap();
        assert_eq!(code.encode(), SAMPLE);
    }

    #[test]
    fn reports_every_matching_host() {
        let peers = [
            member("cuhub-host-25565", "10.42.0.7", false),
            member("cuhub-host-25565", "10.42.0.8", false),
            member("cuhub-host-25565", "10.42.0.7", false),
        ];
        let candidates = find_host_candidates(&peers, 25565);
        assert_eq!(candidates.len(), 2);
        assert!(find_host(&peers, 25565).is_none());
    }

    #[test]
    fn accepts_repeated_rows_for_the_same_host() {
        let peers = [
            member("cuhub-host-25565", "10.42.0.7", false),
            member("cuhub-host-25565", "10.42.0.7", false),
        ];
        assert_eq!(
            find_host(&peers, 25565).unwrap().address,
            Ipv4Addr::new(10, 42, 0, 7)
        );
    }

    #[test]
    fn rejects_port_zero() {
        let peers = [member("cuhub-host-0", "10.42.0.7", false)];
        assert!(find_host(&peers, 0).is_none());
    }
}
