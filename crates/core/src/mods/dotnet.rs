//! Read assembly and BepInPlugin evidence from a managed DLL.

use std::path::Path;

use dotnetdll::prelude::*;

use crate::catalogs::DependencyRequirement;

#[derive(Debug, Default, Clone)]
pub(crate) struct DllEvidence {
    pub(crate) assembly_version: Option<String>,
    pub(crate) plugin_guid: Option<String>,
    pub(crate) plugin_version: Option<String>,
    pub(crate) dependencies: Vec<DependencyRequirement>,
}

const MAX_DLL_BYTES: u64 = 64 * 1024 * 1024;

pub(crate) fn read_dll(path: &Path) -> Option<DllEvidence> {
    let metadata = std::fs::metadata(path).ok()?;
    if metadata.len() > MAX_DLL_BYTES {
        return None;
    }
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
            let constructor = attribute.constructor.show(&resolution);
            if constructor.contains("BepInDependency") {
                let Ok(data) = attribute.instantiation_data(&AlwaysFailsResolver, &resolution)
                else {
                    continue;
                };
                let mut arguments = data.constructor_args.iter();
                let guid =
                    string_argument(arguments.next()).filter(|value| !value.trim().is_empty());
                let Some(guid) = guid else {
                    continue;
                };
                let second = arguments.next();
                let minimum_version =
                    string_argument(second).filter(|value| !value.trim().is_empty());
                let optional = integral_argument(second)
                    .map(|flags| flags & 2 != 0)
                    .unwrap_or(false);
                let requirement = DependencyRequirement {
                    name: guid.clone(),
                    guid: Some(guid),
                    minimum_version,
                    optional,
                    ..DependencyRequirement::default()
                };
                if !evidence.dependencies.iter().any(|existing| {
                    existing.matches(&requirement.name, requirement.guid.as_deref())
                }) {
                    evidence.dependencies.push(requirement);
                }
                continue;
            }
            if !constructor.contains("BepInPlugin") {
                continue;
            }
            let Ok(data) = attribute.instantiation_data(&AlwaysFailsResolver, &resolution) else {
                continue;
            };
            let mut arguments = data.constructor_args.iter();
            let guid = string_argument(arguments.next());
            let _name = string_argument(arguments.next());
            let version = string_argument(arguments.next());
            if let Some(guid) = guid.filter(|value| !value.trim().is_empty()) {
                evidence.plugin_guid = Some(guid);
            }
            if let Some(version) = version.filter(|value| !value.trim().is_empty()) {
                evidence.plugin_version = Some(version);
            }
        }
    }
    Some(evidence)
}

fn string_argument(argument: Option<&FixedArg<'_>>) -> Option<String> {
    match argument {
        Some(FixedArg::String(Some(value))) => Some(value.clone().into_owned()),
        _ => None,
    }
}

fn integral_argument(argument: Option<&FixedArg<'_>>) -> Option<i64> {
    match argument {
        Some(FixedArg::Integral(value)) => Some(integral_value(value)),
        Some(FixedArg::Enum(_, value)) => Some(integral_value(value)),
        _ => None,
    }
}

fn integral_value(value: &IntegralParam) -> i64 {
    match value {
        IntegralParam::Int8(value) => i64::from(*value),
        IntegralParam::Int16(value) => i64::from(*value),
        IntegralParam::Int32(value) => i64::from(*value),
        IntegralParam::Int64(value) => *value,
        IntegralParam::UInt8(value) => i64::from(*value),
        IntegralParam::UInt16(value) => i64::from(*value),
        IntegralParam::UInt32(value) => i64::from(*value),
        IntegralParam::UInt64(value) => i64::try_from(*value).unwrap_or(i64::MAX),
    }
}

fn format_version(version: Version) -> String {
    format!(
        "{}.{}.{}.{}",
        version.major, version.minor, version.build, version.revision
    )
}
