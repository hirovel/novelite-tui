use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

pub(crate) struct Keymap {
    bindings: BTreeMap<String, String>,
    path: Option<PathBuf>,
    pub warning: Option<String>,
}
fn defaults() -> BTreeMap<String, String> {
    [
        ("Ctrl+s", "save"),
        ("F2", "save"),
        ("Ctrl+q", "quit"),
        ("F10", "quit"),
        ("F6", "focus-other-pane"),
        ("F4", "design-settings"),
        ("Ctrl+,", "design-settings"),
        ("Ctrl+f", "find"),
        ("Ctrl+Shift+f", "search-book"),
        ("F7", "focus-book-search"),
        ("F3", "find-next"),
        ("Shift+F3", "find-previous"),
        ("Ctrl+z", "undo"),
        ("Ctrl+Shift+z", "redo"),
        ("Ctrl+y", "redo"),
        ("Ctrl+a", "select-all"),
        ("Ctrl+c", "copy"),
        ("Ctrl+x", "cut"),
        ("Ctrl+v", "paste"),
    ]
    .into_iter()
    .map(|(key, id)| (key.into(), id.into()))
    .collect()
}
impl Default for Keymap {
    fn default() -> Self {
        Self {
            bindings: defaults(),
            path: None,
            warning: None,
        }
    }
}
impl Keymap {
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }
    pub fn load(&mut self, path: &Path, known: &[&str]) {
        self.path = Some(path.to_owned());
        self.bindings = defaults();
        self.warning = None;
        let result = (|| -> io::Result<()> {
            let contents = match fs::read_to_string(path) {
                Ok(text) => text,
                Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
                Err(error) => return Err(error),
            };
            let overrides: UniqueBindings =
                serde_json::from_str(&contents).map_err(io::Error::other)?;
            let mut normalized = BTreeMap::new();
            for (key, id) in overrides.0 {
                let key = normalize(&key)?;
                if !known.contains(&id.as_str()) {
                    return Err(io::Error::other(format!("Unknown command: {id}")));
                }
                if normalized.insert(key, id).is_some() {
                    return Err(io::Error::other("Duplicate shortcut after normalization"));
                }
            }
            self.bindings.extend(normalized);
            Ok(())
        })();
        if let Err(error) = result {
            self.warning = Some(format!("键位配置未加载，使用默认键位：{error}"));
        }
    }
    pub fn set(&mut self, key: &str, id: &str) -> io::Result<()> {
        let mut next = self.bindings.clone();
        next.insert(normalize(key)?, id.into());
        self.persist(&next)?;
        self.bindings = next;
        self.warning = None;
        Ok(())
    }
    pub fn reset(&mut self) -> io::Result<()> {
        self.persist(&BTreeMap::new())?;
        self.bindings = defaults();
        self.warning = None;
        Ok(())
    }
    fn persist(&self, bindings: &BTreeMap<String, String>) -> io::Result<()> {
        let path = self
            .path
            .as_ref()
            .ok_or_else(|| io::Error::other("No writable keymap path configured"))?;
        let path = std::path::absolute(path)?;
        let parent = path
            .parent()
            .ok_or_else(|| io::Error::other("Missing configuration directory"))?;
        fs::create_dir_all(parent)?;
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        file.write_all(&serde_json::to_vec_pretty(bindings).map_err(io::Error::other)?)?;
        file.as_file().sync_all()?;
        file.persist(path).map_err(|error| error.error)?;
        Ok(())
    }
    pub fn lookup(&self, event: KeyEvent) -> Option<String> {
        self.bindings.get(&key_name(event)?).cloned()
    }
    pub fn shortcuts(&self, id: &str) -> String {
        self.bindings
            .iter()
            .filter(|(_, command)| *command == id)
            .map(|(key, _)| key.as_str())
            .collect::<Vec<_>>()
            .join(" / ")
    }
}

pub(crate) fn resolve_destination(path: &Path) -> io::Result<PathBuf> {
    let mut existing = std::path::absolute(path)?;
    let mut suffix = Vec::new();
    while !existing.try_exists()? {
        suffix.push(
            existing
                .file_name()
                .ok_or_else(|| io::Error::other("Invalid configuration path"))?
                .to_os_string(),
        );
        if !existing.pop() {
            return Err(io::Error::other("Missing configuration parent"));
        }
    }
    let mut resolved = fs::canonicalize(existing)?;
    for part in suffix.into_iter().rev() {
        resolved.push(part);
    }
    Ok(resolved)
}

struct UniqueBindings(BTreeMap<String, String>);
impl<'de> Deserialize<'de> for UniqueBindings {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct BindingsVisitor;
        impl<'de> Visitor<'de> for BindingsVisitor {
            type Value = UniqueBindings;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a map of unique shortcuts to command IDs")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                let mut result = BTreeMap::new();
                while let Some((key, value)) = map.next_entry::<String, String>()? {
                    if result.insert(key, value).is_some() {
                        return Err(serde::de::Error::custom("Duplicate shortcut in JSON"));
                    }
                }
                Ok(UniqueBindings(result))
            }
        }
        deserializer.deserialize_map(BindingsVisitor)
    }
}

fn normalize(key: &str) -> io::Result<String> {
    let lower = key.trim().to_ascii_lowercase();
    if let Some(number) = lower
        .strip_prefix('f')
        .and_then(|number| number.parse::<u8>().ok())
        .filter(|number| (2..=12).contains(number))
    {
        return Ok(format!("F{number}"));
    }
    let parts: Vec<_> = lower.split('+').collect();
    if parts.len() < 2 {
        return Err(io::Error::other(
            "Use Ctrl/Alt + letter or F2–F12; F1 and Esc are reserved",
        ));
    }
    let character = parts.last().unwrap();
    let function = character
        .strip_prefix('f')
        .and_then(|n| n.parse::<u8>().ok())
        .filter(|n| (2..=12).contains(n));
    if function.is_none()
        && (character.len() != 1
            || !(character.as_bytes()[0].is_ascii_lowercase() || *character == ","))
    {
        return Err(io::Error::other("Shortcut needs one ASCII letter or comma"));
    }
    let mut modifiers = KeyModifiers::NONE;
    for part in &parts[..parts.len() - 1] {
        let modifier = match *part {
            "ctrl" => KeyModifiers::CONTROL,
            "alt" => KeyModifiers::ALT,
            "shift" => KeyModifiers::SHIFT,
            _ => return Err(io::Error::other("Unknown modifier")),
        };
        if modifiers.contains(modifier) {
            return Err(io::Error::other("Repeated modifier"));
        }
        modifiers.insert(modifier);
    }
    if function.is_none() && !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) {
        return Err(io::Error::other("Plain typing cannot be rebound"));
    }
    let normalized = key_name(KeyEvent::new(
        function.map_or_else(
            || KeyCode::Char(character.chars().next().unwrap()),
            KeyCode::F,
        ),
        modifiers,
    ))
    .unwrap();
    if normalized == "Ctrl+p" {
        return Err(io::Error::other("Ctrl+P opens commands and is reserved"));
    }
    Ok(normalized)
}
fn key_name(key: KeyEvent) -> Option<String> {
    let base = match key.code {
        KeyCode::F(number) => format!("F{number}"),
        KeyCode::Char(ch) if ch.is_ascii_alphabetic() || ch == ',' => {
            ch.to_ascii_lowercase().to_string()
        }
        _ => return None,
    };
    if (!matches!(key.code, KeyCode::F(_))
        && !key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT))
        || key.modifiers.contains(KeyModifiers::SUPER)
    {
        return None;
    }
    let mut result = String::new();
    for (modifier, name) in [
        (KeyModifiers::CONTROL, "Ctrl+"),
        (KeyModifiers::ALT, "Alt+"),
        (KeyModifiers::SHIFT, "Shift+"),
    ] {
        if key.modifiers.contains(modifier) {
            result.push_str(name);
        }
    }
    result.push_str(&base);
    Some(result)
}
