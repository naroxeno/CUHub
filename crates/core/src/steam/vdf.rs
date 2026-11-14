//! Valve KeyValues reader for libraryfolders.vdf and appmanifest_*.acf. Parsing only.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VdfNode {
    name: String,
    value: Option<String>,
    children: Vec<VdfNode>,
}

impl VdfNode {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn value(&self) -> Option<&str> {
        self.value.as_deref()
    }

    pub fn is_object(&self) -> bool {
        self.value.is_none()
    }

    pub fn children(&self) -> impl Iterator<Item = &VdfNode> {
        self.children.iter()
    }

    pub fn child(&self, name: &str) -> Option<&VdfNode> {
        self.children
            .iter()
            .find(|child| child.name.eq_ignore_ascii_case(name))
    }

    pub fn child_value(&self, name: &str) -> Option<&str> {
        self.child(name)?.value.as_deref()
    }

    pub fn collect_library_paths(&self) -> Vec<&str> {
        let mut paths = Vec::new();
        for child in &self.children {
            match child.value.as_deref() {
                None => {
                    if let Some(path) = child.child_value("path").filter(|path| !path.is_empty()) {
                        paths.push(path);
                    }
                }
                Some(value) if !value.is_empty() && child.name.parse::<u64>().is_ok() => {
                    paths.push(value);
                }
                Some(_) => {}
            }
        }
        paths
    }

    pub fn parse(text: &str) -> VdfNode {
        let mut root = VdfNode {
            name: "<root>".to_string(),
            value: None,
            children: Vec::new(),
        };
        let mut index = 0;
        parse_into(&mut root, text, &mut index, 0);
        root
    }
}

fn parse_into(parent: &mut VdfNode, text: &str, index: &mut usize, depth: u32) {
    if depth > 32 {
        return;
    }
    loop {
        let Some(key) = read_token(text, index) else {
            return;
        };
        if key == "}" {
            return;
        }
        if key == "{" {
            continue;
        }
        let save = *index;
        match read_token(text, index).as_deref() {
            None => return,
            Some("{") => {
                let mut node = VdfNode {
                    name: key,
                    value: None,
                    children: Vec::new(),
                };
                parse_into(&mut node, text, index, depth + 1);
                parent.children.push(node);
            }
            Some("}") => {
                parent.children.push(VdfNode {
                    name: key,
                    value: Some(String::new()),
                    children: Vec::new(),
                });
                *index = save;
                return;
            }
            Some(value) => parent.children.push(VdfNode {
                name: key,
                value: Some(value.to_string()),
                children: Vec::new(),
            }),
        }
    }
}

fn read_token(text: &str, index: &mut usize) -> Option<String> {
    loop {
        while let Some(ch) = text[*index..].chars().next() {
            if !ch.is_whitespace() {
                break;
            }
            *index += ch.len_utf8();
        }
        if *index >= text.len() {
            return None;
        }
        if text[*index..].starts_with("//") {
            while let Some(ch) = text[*index..].chars().next() {
                if ch == '\n' || ch == '\r' {
                    break;
                }
                *index += ch.len_utf8();
            }
            continue;
        }
        break;
    }

    let ch = text[*index..].chars().next()?;
    if ch == '{' || ch == '}' {
        *index += ch.len_utf8();
        return Some(ch.to_string());
    }
    if ch == '"' {
        *index += 1;
        let mut value = String::new();
        while *index < text.len() {
            let ch = text[*index..].chars().next()?;
            if ch == '"' {
                *index += 1;
                return Some(value);
            }
            if ch == '\\' && *index + 1 < text.len() {
                *index += 1;
                let escaped = text[*index..].chars().next()?;
                value.push(match escaped {
                    'n' => '\n',
                    't' => '\t',
                    'r' => '\r',
                    other => other,
                });
                *index += escaped.len_utf8();
                continue;
            }
            value.push(ch);
            *index += ch.len_utf8();
        }
        return Some(value);
    }

    let start = *index;
    while *index < text.len() {
        let ch = text[*index..].chars().next()?;
        if ch.is_whitespace() || ch == '{' || ch == '}' || ch == '"' {
            break;
        }
        *index += ch.len_utf8();
    }
    if *index > start {
        Some(text[start..*index].to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_modern_nested_libraryfolders() {
        let text = r#""libraryfolders"
{
    "0"
    {
        "path"        "/home/u/.local/share/Steam"
        "label"        ""
        "apps"
        {
            "228980"        "434588"
        }
    }
    "1"
    {
        "path"        "/mnt/games/SteamLibrary"
        "label"        "games"
    }
}"#;
        let root = VdfNode::parse(text);
        let container = root.children().next().unwrap();
        assert_eq!(
            container.collect_library_paths(),
            vec!["/home/u/.local/share/Steam", "/mnt/games/SteamLibrary"]
        );
    }

    #[test]
    fn parses_legacy_flat_libraryfolders() {
        let text = r#""LibraryFolders"
{
    "TimeNextStatsReport"        "1234567890"
    "ContentStatsID"        "-987654321"
    "1"        "D:\\SteamLibrary"
    "2"        "E:\\Games\\Steam"
}"#;
        let root = VdfNode::parse(text);
        let container = root.children().next().unwrap();
        assert_eq!(
            container.collect_library_paths(),
            vec![r"D:\SteamLibrary", r"E:\Games\Steam"]
        );
    }

    #[test]
    fn unescapes_backslashes_and_quotes() {
        let text = r#""root"
{
    "path"        "C:\\Program Files\\Steam"
    "quoted"        "a \"b\" c"
}"#;
        let root = VdfNode::parse(text);
        let root = root.children().next().unwrap();
        assert_eq!(root.child_value("path"), Some(r"C:\Program Files\Steam"));
        assert_eq!(root.child_value("quoted"), Some(r#"a "b" c"#));
    }

    #[test]
    fn reads_an_app_manifest() {
        let text = r#""AppState"
{
    "appid"        "3167550"
    "name"        "Casualties Unknown Demo"
    "installdir"        "Casualties Unknown Demo"
    "StateFlags"        "4"
}"#;
        let root = VdfNode::parse(text);
        let state = root.children().next().unwrap();
        assert_eq!(state.child_value("appid"), Some("3167550"));
        assert_eq!(
            state.child_value("installdir"),
            Some("Casualties Unknown Demo")
        );
    }

    #[test]
    fn child_lookup_ignores_key_casing() {
        let root = VdfNode::parse("\"AppState\"\n{\n\t\"AppID\"\t\"123\"\n}");
        let state = root.children().next().unwrap();
        assert_eq!(state.child_value("appid"), Some("123"));
    }

    #[test]
    fn skips_line_comments() {
        let text = r#""root"
{
    // a comment Valve sometimes writes
    "path"        "/games"
}"#;
        let root = VdfNode::parse(text);
        assert_eq!(
            root.children().next().unwrap().child_value("path"),
            Some("/games")
        );
    }

    #[test]
    fn malformed_input_does_not_panic() {
        for text in ["", "   ", "\"unclosed\" {", "}}}}", "\"a\" \"b\" \"c\""] {
            let node = VdfNode::parse(text);
            let _ = node.collect_library_paths();
        }
    }
}
