use std::{env, io, path::PathBuf};

const APP_DIRECTORY: &str = "pookie-paste";

const DATABASE_FILE: &str = "pookie-paste.db";

pub fn data_directory() -> io::Result<PathBuf> {
    if let Some(data_home) = env::var_os("XDG_DATA_HOME")
        && !data_home.is_empty()
    {
        return Ok(PathBuf::from(data_home).join(APP_DIRECTORY));
    }

    let home = env::var_os("HOME").ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "neither XDG_DATA_HOME nor HOME is available",
        )
    })?;

    if home.is_empty() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "HOME is empty"));
    }

    Ok(PathBuf::from(home)
        .join(".local")
        .join("share")
        .join(APP_DIRECTORY))
}

pub fn database_path() -> io::Result<PathBuf> {
    Ok(data_directory()?.join(DATABASE_FILE))
}

pub fn ensure_data_directory() -> io::Result<PathBuf> {
    let directory = data_directory()?;

    std::fs::create_dir_all(&directory)?;

    Ok(directory)
}

const CONFIG_FILE: &str = "config.toml";

pub fn config_directory() -> io::Result<PathBuf> {
    if let Some(config_home) = env::var_os("XDG_CONFIG_HOME")
        && !config_home.is_empty()
    {
        return Ok(PathBuf::from(config_home).join(APP_DIRECTORY));
    }

    let home = env::var_os("HOME").ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "neither XDG_CONFIG_HOME nor HOME is available",
        )
    })?;

    if home.is_empty() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "HOME is empty"));
    }

    Ok(PathBuf::from(home).join(".config").join(APP_DIRECTORY))
}

pub fn config_path() -> io::Result<PathBuf> {
    let default_path = config_directory()?.join(CONFIG_FILE);
    if default_path.exists() {
        return Ok(default_path);
    }

    // Also check ~/.config/pookie/config.toml for user convenience
    if let Some(config_home) = env::var_os("XDG_CONFIG_HOME")
        && !config_home.is_empty()
    {
        let alt = PathBuf::from(config_home).join("pookie").join(CONFIG_FILE);
        if alt.exists() {
            return Ok(alt);
        }
    } else if let Some(home) = env::var_os("HOME")
        && !home.is_empty()
    {
        let alt = PathBuf::from(home)
            .join(".config")
            .join("pookie")
            .join(CONFIG_FILE);
        if alt.exists() {
            return Ok(alt);
        }
    }

    Ok(default_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_file_name_is_stable() {
        assert_eq!(DATABASE_FILE, "pookie-paste.db",);
    }

    #[test]
    fn app_directory_name_is_stable() {
        assert_eq!(APP_DIRECTORY, "pookie-paste",);
    }

    #[test]
    fn config_file_name_is_stable() {
        assert_eq!(CONFIG_FILE, "config.toml");
    }

    #[test]
    fn config_directory_ends_with_app_directory() {
        let dir = config_directory().expect("failed to resolve config directory");
        assert!(dir.ends_with(APP_DIRECTORY));
    }
}
