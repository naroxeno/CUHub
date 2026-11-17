//! Ported from burningtnt/Terracotta@694980b src/easytier/mod.rs (member model and CLI peer parsing).
//! Local changes: parsing is a standalone parse_peers, and unknown NAT names return None instead of panicking.

use std::net::Ipv4Addr;

use serde_json::Value;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum NatType {
    Public,
    NoPat,
    FullCone,
    Restricted,
    PortRestricted,
    Symmetric,
    SymUdpFirewall,
    SymmetricEasyInc,
    SymmetricEasyDec,
    Unknown,
}

impl NatType {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "Public" => Some(Self::Public),
            "NoPat" => Some(Self::NoPat),
            "FullCone" => Some(Self::FullCone),
            "Restricted" => Some(Self::Restricted),
            "PortRestricted" => Some(Self::PortRestricted),
            "Symmetric" => Some(Self::Symmetric),
            "SymUdpFirewall" => Some(Self::SymUdpFirewall),
            "SymmetricEasyInc" => Some(Self::SymmetricEasyInc),
            "SymmetricEasyDec" => Some(Self::SymmetricEasyDec),
            "Unknown" => Some(Self::Unknown),
            _ => None,
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct EasyTierMember {
    pub hostname: String,
    pub address: Option<Ipv4Addr>,
    pub is_local: bool,
    pub nat: NatType,
}

pub fn parse_peers(json: &str) -> Option<Vec<EasyTierMember>> {
    let value: Value = serde_json::from_str(json).ok()?;
    let mut members = Vec::new();
    for entry in value.as_array()? {
        for member in entry.get("route")?.as_array()? {
            let hostname = member.get("hostname")?.as_str()?.to_owned();
            let address: Option<Ipv4Addr> = member.get("ipv4")?.as_str()?.parse().ok();
            let nat = NatType::from_name(member.get("nat_type")?.as_str()?)?;
            let is_local = member.get("cost")?.as_str()? == "Local";
            members.push(EasyTierMember {
                hostname,
                address,
                is_local,
                nat,
            });
        }
    }
    Some(members)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_local_node() {
        let json = r#"[{"route":[{"hostname":"laptop","ipv4":"10.126.126.1","cost":"Local","nat_type":"Unknown"}]}]"#;
        let members = parse_peers(json).unwrap();
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].hostname, "laptop");
        assert_eq!(members[0].address, Some(Ipv4Addr::new(10, 126, 126, 1)));
        assert!(members[0].is_local);
        assert_eq!(members[0].nat, NatType::Unknown);
    }

    #[test]
    fn parses_remote_peers_and_rejects_bad_input() {
        let json = r#"[{"route":[{"hostname":"peer","ipv4":"10.126.126.2","cost":"12.4 ms","nat_type":"FullCone"}]}]"#;
        let members = parse_peers(json).unwrap();
        assert_eq!(members[0].hostname, "peer");
        assert!(!members[0].is_local);
        assert_eq!(members[0].nat, NatType::FullCone);
        assert!(parse_peers(r#"[{"route":[{"hostname":"x","ipv4":"1.2.3.4","cost":"1 ms","nat_type":"MadeUp"}]}]"#).is_none());
        assert!(parse_peers("not json").is_none());
    }
}
