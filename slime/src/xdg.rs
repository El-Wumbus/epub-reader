use std::path::PathBuf;

pub struct Dirs {
    pub home: Option<PathBuf>,
    pub cache: Option<PathBuf>,
    pub config: Option<PathBuf>,
    pub data: Option<PathBuf>,
    pub runtime: Option<PathBuf>,
    pub state: Option<PathBuf>,
    pub executable: Option<PathBuf>,
    pub user_directories: Vec<UserDir>,
}

impl Dirs {
    pub fn all() -> Self {
        Self {
            home: Self::home_dir(),
            cache: Self::cache_dir(),
            config: Self::config_dir(),
            data: Self::data_dir(),
            runtime: Self::runtime_dir(),
            state: Self::state_dir(),
            executable: Self::executable_dir(),
            user_directories: Self::user_directories()
                .into_iter()
                .flatten()
                .collect(),
        }
    }
    pub fn home_dir() -> Option<PathBuf> {
        std::env::var_os("HOME")
            .and_then(|h| if h.is_empty() { None } else { Some(h) })
            .map(PathBuf::from)
    }

    pub fn cache_dir() -> Option<PathBuf> {
        std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .and_then(is_absolute)
            .or_else(|| Self::home_dir().map(|h| h.join(".cache")))
    }
    pub fn config_dir() -> Option<PathBuf> {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .and_then(is_absolute)
            .or_else(|| Self::home_dir().map(|h| h.join(".config")))
    }
    pub fn data_dir() -> Option<PathBuf> {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .and_then(is_absolute)
            .or_else(|| Self::home_dir().map(|h| h.join(".local/share")))
    }
    pub fn data_dirs() -> Vec<PathBuf> {
        let Ok(d) = std::env::var("XDG_DATA_DIRS") else {
            return vec![];
        };
        d.split(':').map(str::trim).map(PathBuf::from).collect()
    }
    pub fn runtime_dir() -> Option<PathBuf> {
        std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .and_then(is_absolute)
    }
    pub fn state_dir() -> Option<PathBuf> {
        std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .and_then(is_absolute)
            .or_else(|| Self::home_dir().map(|h| h.join(".local/state")))
    }
    pub fn executable_dir() -> Option<PathBuf> {
        std::env::var_os("XDG_BIN_HOME")
            .map(PathBuf::from)
            .and_then(is_absolute)
            .or_else(|| Self::home_dir().map(|h| h.join(".local/bin")))
    }

    pub fn user_directories() -> Option<Vec<UserDir>> {
        let home = Self::home_dir()?;
        let config = Self::config_dir()?;
        let users_file = config.join("user-dirs.dirs");
        let users = std::fs::read_to_string(users_file).ok()?;
        let parsed = UserDirParser::new(users.as_str(), home.as_path())
            .collect::<Vec<_>>();
        Some(parsed)
    }
}

/// Parse a XDG user directtory from `$(XDG_CONFIG_HOME)/user-dirs.dirs.`
pub struct UserDirParser<'a> {
    lines: std::str::Lines<'a>,
    /// The user's home directory. This is used for completing relative paths in the document.
    home: &'a std::path::Path,
}
#[derive(Debug)]
pub struct UserDir {
    /// Name like "desktop" from `XDG_DESKTOP_DIR`.
    pub name: String,
    pub path: PathBuf,
}
impl<'a> UserDirParser<'a> {
    pub fn new(s: &'a str, home: &'a std::path::Path) -> UserDirParser<'a> {
        Self {
            lines: s.lines(),
            home,
        }
    }
}

impl<'a> Iterator for UserDirParser<'a> {
    type Item = UserDir;
    fn next(&mut self) -> Option<UserDir> {
        while let Some(line) = self.lines.next() {
            let line = line.trim();
            if line.starts_with("#") {
                continue;
            } else if let Some((key, value)) = line.split_once('=') {
                if !key.starts_with("XDG_") || !key.ends_with("_DIR") {
                    continue;
                }

                // We assume the value is surrounded in double quotes
                // that's what `xdg-user-dirs-update` uses.
                if !value.starts_with('"') || !value.ends_with('"') {
                    continue;
                }
                let name = key
                    .get(4..key.len() - 4)
                    .expect("unreachable: key is at least eight characters")
                    .to_lowercase();
                let value = value
                    .get(1..value.len() - 1)
                    .expect("unreachable: value is at least two characters");

                if value == "$HOME/" {
                    // Directories are disabled/removed when they're assigned to the home directory
                    continue;
                }

                let value = shell_unescape(value);
                let path = if value.starts_with("$HOME/") {
                    let value = value
                        .get("$HOME/".len()..)
                        .expect("unreachable: we just checked for this");
                    self.home.join(value)
                } else if value.starts_with("/") {
                    PathBuf::from(value)
                } else {
                    continue;
                };
                return Some(UserDir { name, path });
            }
        }
        None
    }
}

pub fn current_desktop() -> Option<String> {
    std::env::var("XDG_CURRENT_DESKTOP").ok()
}

/// TODO: this is a stub.
pub fn query_default(mime: impl AsRef<str>) -> std::io::Result<()> {
    todo!()
}

fn shell_unescape(s: &str) -> String {
    let mut new = String::new();
    let mut chars = s.chars();

    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(ch) = chars.next() {
                new.push(ch);
            }
        } else {
            new.push(ch);
        }
    }
    new
}

fn is_absolute(path: PathBuf) -> Option<PathBuf> {
    if path.is_absolute() {
        Some(path)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn user_paser() {
        let config_home = Dirs::config_dir().unwrap();
        let home = Dirs::home_dir().unwrap();
        let users_file = config_home.join("user-dirs.dirs");
        let users = std::fs::read_to_string(users_file).unwrap();
        let parser = UserDirParser::new(users.as_str(), home.as_path());
        let parsed = parser.collect::<Vec<_>>();
        for x in parsed.iter() {
            println!("{x:?}");
        }
        assert!(parsed.into_iter().find(|x| x.name == "documents").is_some());
    }
}
