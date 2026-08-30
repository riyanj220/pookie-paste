use std::env;
use std::path::PathBuf;

pub fn socket_path() -> PathBuf {
    if let Some(runtime_dir) = env::var_os("XDG_RUNTIME_DIR") {
        return PathBuf::from(runtime_dir)
            .join("pookie-paste")
            .join("pookie.sock");
    }

    PathBuf::from("/tmp")
        .join(format!("pookie-paste-{}", current_user_id(),))
        .join("pookie.sock")
}

fn current_user_id() -> u32 {
    unsafe { libc::geteuid() }
}
