use std::path::PathBuf;

/// TODO: reconsider the fields of this structure
pub struct Dirs {
    pub home: Option<PathBuf>,
    pub cache: Option<PathBuf>,
    pub config_home: Option<PathBuf>,
    pub config_dirs: Vec<PathBuf>,
    pub data_home: Option<PathBuf>,
    pub data_dirs: Vec<PathBuf>,
    pub runtime: Option<PathBuf>,
    pub state: Option<PathBuf>,
    pub executable: Option<PathBuf>,
    pub user_directories: Vec<UserDir>,
    pub current_desktop: Vec<String>,
}

impl Dirs {
    pub fn all() -> Self {
        Self {
            home: Self::home_dir(),
            cache: Self::cache_dir(),
            config_home: Self::config_home_dir(),
            config_dirs: Self::config_dirs(),
            data_home: Self::data_home_dir(),
            data_dirs: Self::data_dirs(),
            runtime: Self::runtime_dir(),
            state: Self::state_dir(),
            executable: Self::executable_dir(),
            user_directories: Self::user_directories()
                .into_iter()
                .flatten()
                .collect(),
            current_desktop: Self::current_desktop(),
        }
    }

    /// Returns the paths where `mimeapps.list` files are found in accordance with the lookup order.
    ///
    /// The lookup order follows:
    /// - `$XDG_CONFIG_HOME/$desktop-mimeapps.list`
    /// - `$XDG_CONFIG_HOME/mimeapps.list`
    /// - `$XDG_CONFIG_DIRS/$desktop-mimeapps.list`
    /// - `$XDG_CONFIG_DIRS/mimeapps.list`
    /// - `$XDG_DATA_HOME/applications/$desktop-mimeapps.list`
    /// - `$XDG_DATA_HOME/applications/mimeapps.list`
    /// - `$XDG_DATA_DIRS/applications/$desktop-mimeapp`
    /// - `$XDG_DATA_DIRS/applications/mimeapps.list`
    ///
    /// Written in accordance with the specification found at
    /// <https://specifications.freedesktop.org/mime-apps-spec/latest/file.html>.
    pub fn mimeapps_search_paths(&self) -> Vec<PathBuf> {
        let mut search_paths = vec![];

        if let Some(dir) = self.config_home.as_deref() {
            for desktop in self.current_desktop.iter() {
                search_paths
                    .push(dir.join(format!("{}-mimeapps.list", desktop)));
            }
            search_paths.push(dir.join("mimeapps.list"));
        }
        for dir in self.config_dirs.iter() {
            for desktop in self.current_desktop.iter() {
                search_paths.push(dir.join(format!("{desktop}-mimeapps.list")));
            }
            search_paths.push(dir.join("mimeapps.list"));
        }

        if let Some(dir) = self.data_home.as_deref() {
            for desktop in self.current_desktop.iter() {
                search_paths.push(
                    dir.join(format!("applications/{desktop}-mimeapps.list")),
                );
            }
            search_paths.push(dir.join("applications/mimeapps.list"));
        }
        for dir in self.data_dirs.iter() {
            for desktop in self.current_desktop.iter() {
                search_paths.push(
                    dir.join(format!("applications/{desktop}-mimeapps.list")),
                );
            }
            search_paths.push(dir.join("applications/mimeapps.list"));
        }

        search_paths
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

    /// This is probably what you want.
    pub fn config_home_dir() -> Option<PathBuf> {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .and_then(is_absolute)
            .or_else(|| Self::home_dir().map(|h| h.join(".config")))
    }

    pub fn data_home_dir() -> Option<PathBuf> {
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

        let config = Self::config_home_dir()?;
        let users_file = config.join("user-dirs.dirs");
        let users = std::fs::read_to_string(users_file).ok()?;
        let parsed = UserDirParser::new(users.as_str(), home.as_path())
            .collect::<Vec<_>>();
        Some(parsed)
    }

    pub fn current_desktop() -> Vec<String> {
        let Some(desktop) = std::env::var("XDG_CURRENT_DESKTOP").ok() else {
            return vec![];
        };
        desktop
            .split(':')
            .map(str::trim)
            .filter(|x| !x.is_empty())
            .map(String::from)
            .collect()
    }

    pub fn config_dirs() -> Vec<PathBuf> {
        let Some(desktop) = std::env::var("XDG_CONFIG_DIRS").ok() else {
            return vec![];
        };
        desktop
            .split(':')
            .map(str::trim)
            .filter(|x| !x.is_empty())
            .map(PathBuf::from)
            .collect()
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
        let config_home = Dirs::config_home_dir().unwrap();
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

    #[test]
    fn test_finding_mimepaths_search_paths() {
        let mut found_one = false;
        for path in Dirs::all().mimeapps_search_paths() {
            if path.is_file() {
                println!("'{}' Found!", path.display());
                found_one = true;
            } else {
                println!("'{}' Not Found!", path.display());
            }
        }
        assert!(found_one, "Expected to find at least one mimeapps file");
    }
}
