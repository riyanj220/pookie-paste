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
}
