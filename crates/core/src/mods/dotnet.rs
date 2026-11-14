//! Read assembly and BepInPlugin evidence from a managed DLL.

use std::path::Path;

use dotnetdll::prelude::*;

#[derive(Debug, Default, Clone)]
pub(crate) struct DllEvidence {
    pub(crate) assembly_version: Option<String>,
    pub(crate) plugin_guid: Option<String>,
    pub(crate) plugin_version: Option<String>,
}

pub(crate) fn read_dll(path: &Path) -> Option<DllEvidence> {
    let bytes = std::fs::read(path).ok()?;
    read_bytes(&bytes)
}

fn read_bytes(bytes: &[u8]) -> Option<DllEvidence> {
    let resolution = Resolution::parse(bytes, ReadOptions::default()).ok()?;
    let assembly_version = resolution
        .assembly
        .as_ref()
        .map(|assembly| format_version(assembly.version));
    let mut evidence = DllEvidence {
        assembly_version,
        ..DllEvidence::default()
    };

    for (type_index, _) in resolution.enumerate_type_definitions() {
        let Ok(attributes) = resolution.type_attributes(type_index) else {
            continue;
        };
        for attribute in attributes {
            if !attribute
                .constructor
                .show(&resolution)
                .contains("BepInPlugin")
            {
                continue;
            }
            let Ok(data) = attribute.instantiation_data(&AlwaysFailsResolver, &resolution) else {
                continue;
            };
            let mut arguments = data.constructor_args.into_iter();
            let guid = string_argument(arguments.next());
            let _name = string_argument(arguments.next());
            let version = string_argument(arguments.next());
            if let Some(guid) = guid.filter(|value| !value.trim().is_empty()) {
                evidence.plugin_guid = Some(guid);
            }
            if let Some(version) = version.filter(|value| !value.trim().is_empty()) {
                evidence.plugin_version = Some(version);
            }
            if evidence.plugin_guid.is_some() && evidence.plugin_version.is_some() {
                return Some(evidence);
            }
        }
    }
    Some(evidence)
}

fn string_argument(argument: Option<FixedArg<'_>>) -> Option<String> {
    match argument {
        Some(FixedArg::String(Some(value))) => Some(value.into_owned()),
        _ => None,
    }
}

fn format_version(version: Version) -> String {
    format!(
        "{}.{}.{}.{}",
        version.major, version.minor, version.build, version.revision
    )
}
