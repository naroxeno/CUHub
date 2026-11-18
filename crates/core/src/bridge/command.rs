//! Bridge command whitelist and per-mod permission resolution. No transport.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Policy {
    Read,
    Control,
    Mutate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandSpec {
    pub id: &'static str,
    pub policy: Policy,
}

pub const COMMANDS: [CommandSpec; 15] = [
    CommandSpec {
        id: "mp.state",
        policy: Policy::Read,
    },
    CommandSpec {
        id: "mp.host",
        policy: Policy::Control,
    },
    CommandSpec {
        id: "mp.join",
        policy: Policy::Control,
    },
    CommandSpec {
        id: "mp.leave",
        policy: Policy::Control,
    },
    CommandSpec {
        id: "mp.kick",
        policy: Policy::Mutate,
    },
    CommandSpec {
        id: "save.list",
        policy: Policy::Read,
    },
    CommandSpec {
        id: "save.current",
        policy: Policy::Read,
    },
    CommandSpec {
        id: "save.save",
        policy: Policy::Mutate,
    },
    CommandSpec {
        id: "save.load",
        policy: Policy::Mutate,
    },
    CommandSpec {
        id: "save.backup",
        policy: Policy::Mutate,
    },
    CommandSpec {
        id: "save.restore",
        policy: Policy::Mutate,
    },
    CommandSpec {
        id: "skin.current",
        policy: Policy::Read,
    },
    CommandSpec {
        id: "skin.list",
        policy: Policy::Read,
    },
    CommandSpec {
        id: "skin.apply",
        policy: Policy::Control,
    },
    CommandSpec {
        id: "skin.reload",
        policy: Policy::Control,
    },
];

pub fn find(id: &str) -> Option<&'static CommandSpec> {
    COMMANDS.iter().find(|command| command.id == id)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Permission {
    Granted,
    Denied,
    Ask,
}

impl Permission {
    fn rank(self) -> u8 {
        match self {
            Self::Granted => 0,
            Self::Ask => 1,
            Self::Denied => 2,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BridgeSettings {
    pub permissions: BTreeMap<String, BTreeMap<String, Permission>>,
}

#[derive(Debug, Clone, Default)]
pub struct Permissions {
    entries: BTreeMap<String, BTreeMap<String, Permission>>,
}

impl Permissions {
    pub fn new(entries: BTreeMap<String, BTreeMap<String, Permission>>) -> Self {
        Self { entries }
    }

    pub fn from_settings(settings: &BridgeSettings) -> Self {
        Self::new(settings.permissions.clone())
    }

    pub fn resolve(&self, command: &CommandSpec, mods: &[String]) -> Permission {
        let mut resolved: Option<Permission> = None;
        for guid in mods {
            let Some(permission) = self
                .entries
                .get(guid)
                .and_then(|commands| commands.get(command.id))
                .copied()
            else {
                continue;
            };
            resolved = Some(match resolved {
                Some(current) if current.rank() >= permission.rank() => current,
                _ => permission,
            });
        }
        resolved.unwrap_or_else(|| default_permission(command.policy))
    }

    pub fn allows(&self, command: &CommandSpec, mods: &[String]) -> bool {
        self.resolve(command, mods) == Permission::Granted
    }

    pub fn snapshot(&self, mods: &[String]) -> BTreeMap<String, Permission> {
        COMMANDS
            .iter()
            .map(|command| (command.id.to_owned(), self.resolve(command, mods)))
            .collect()
    }
}

fn default_permission(policy: Policy) -> Permission {
    match policy {
        Policy::Read => Permission::Granted,
        Policy::Control | Policy::Mutate => Permission::Denied,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guids(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    fn entries(
        guid: &str,
        command: &str,
        permission: Permission,
    ) -> BTreeMap<String, BTreeMap<String, Permission>> {
        BTreeMap::from([(
            guid.to_owned(),
            BTreeMap::from([(command.to_owned(), permission)]),
        )])
    }

    #[test]
    fn command_table_matches_protocol() {
        assert_eq!(COMMANDS.len(), 15);
        assert_eq!(
            find("mp.state").map(|command| command.policy),
            Some(Policy::Read)
        );
        assert_eq!(
            find("mp.kick").map(|command| command.policy),
            Some(Policy::Mutate)
        );
        assert_eq!(
            find("skin.apply").map(|command| command.policy),
            Some(Policy::Control)
        );
        assert!(find("mp.unknown").is_none());
    }

    #[test]
    fn defaults_follow_policy() {
        let permissions = Permissions::default();
        let known = guids(&["cuhub.bridge"]);
        assert_eq!(
            permissions.resolve(find("mp.state").unwrap(), &known),
            Permission::Granted
        );
        assert_eq!(
            permissions.resolve(find("mp.host").unwrap(), &known),
            Permission::Denied
        );
        assert_eq!(
            permissions.resolve(find("save.save").unwrap(), &known),
            Permission::Denied
        );
        assert!(!permissions.allows(find("mp.kick").unwrap(), &known));
    }

    #[test]
    fn explicit_permission_overrides_default() {
        let permissions = Permissions::new(entries("cuhub.bridge", "mp.host", Permission::Granted));
        let known = guids(&["cuhub.bridge"]);
        assert!(permissions.allows(find("mp.host").unwrap(), &known));
        assert_eq!(
            permissions.resolve(find("mp.state").unwrap(), &known),
            Permission::Granted
        );
    }

    #[test]
    fn denied_wins_across_mods_and_unknown_mods_are_ignored() {
        let mut map = BTreeMap::new();
        map.insert(
            "cuhub.bridge".to_owned(),
            BTreeMap::from([("mp.state".to_owned(), Permission::Granted)]),
        );
        map.insert(
            "other.mod".to_owned(),
            BTreeMap::from([("mp.state".to_owned(), Permission::Denied)]),
        );
        let permissions = Permissions::new(map);
        let both = guids(&["cuhub.bridge", "other.mod"]);
        let only_known = guids(&["cuhub.bridge"]);
        let unknown = guids(&["not.declared"]);
        assert_eq!(
            permissions.resolve(find("mp.state").unwrap(), &both),
            Permission::Denied
        );
        assert_eq!(
            permissions.resolve(find("mp.state").unwrap(), &only_known),
            Permission::Granted
        );
        assert_eq!(
            permissions.resolve(find("mp.state").unwrap(), &unknown),
            Permission::Granted
        );
    }

    #[test]
    fn snapshot_lists_every_command() {
        let permissions = Permissions::new(entries("cuhub.bridge", "skin.reload", Permission::Ask));
        let snapshot = permissions.snapshot(&guids(&["cuhub.bridge"]));
        assert_eq!(snapshot.len(), 15);
        assert_eq!(snapshot.get("skin.reload"), Some(&Permission::Ask));
        assert_eq!(snapshot.get("mp.kick"), Some(&Permission::Denied));
        assert!(!permissions.allows(find("skin.reload").unwrap(), &guids(&["cuhub.bridge"])));
    }

    #[test]
    fn settings_round_trip_camel_case() {
        let settings = BridgeSettings {
            permissions: entries("cuhub.bridge", "mp.host", Permission::Granted),
        };
        let raw = serde_json::to_string(&settings).unwrap();
        assert!(raw.contains("\"permissions\""));
        assert!(raw.contains("\"granted\""));
        let parsed: BridgeSettings = serde_json::from_str(&raw).unwrap();
        assert_eq!(parsed, settings);
        let missing: BridgeSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(missing, BridgeSettings::default());
    }
}
