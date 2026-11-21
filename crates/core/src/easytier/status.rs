//! Ported from burningtnt/Terracotta@694980b src/easytier/mod.rs (member model and CLI peer parsing).
//! Supports flat EasyTier 2.6.4 peers and the older grouped route output.

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
        if let Some(route) = entry.get("route") {
            for member in route.as_array()? {
                members.push(parse_member(member)?);
            }
        } else {
            members.push(parse_member(entry)?);
        }
    }
    Some(members)
}

fn parse_member(member: &Value) -> Option<EasyTierMember> {
    let hostname = member.get("hostname")?.as_str()?.to_owned();
    let address = member.get("ipv4")?.as_str()?.parse().ok();
    let nat = member
        .get("nat_type")?
        .as_str()
        .and_then(NatType::from_name)
        .unwrap_or(NatType::Unknown);
    let is_local = member.get("cost")?.as_str()? == "Local";
    Some(EasyTierMember {
        hostname,
        address,
        is_local,
        nat,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_flat_easytier_264_peers() {
        let json = r#"[
            {"hostname":"guest","ipv4":"","cost":"Local","nat_type":"Unknown"},
            {"hostname":"cuhub-host-24444","ipv4":"10.42.0.7","cost":"p2p","nat_type":"NewNat"}
        ]"#;
        let members = parse_peers(json).unwrap();
        assert!(members[0].is_local);
        assert_eq!(members[0].address, None);
        assert!(!members[1].is_local);
        assert_eq!(members[1].address, Some(Ipv4Addr::new(10, 42, 0, 7)));
        assert_eq!(members[1].nat, NatType::Unknown);
    }

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

        assert!(parse_peers("not json").is_none());
    }
}
