use std::env;
use std::path::PathBuf;
// -------------------------
// Minecraft prepare & launch
// -------------------------

pub fn project_dir() -> PathBuf {
    env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

pub fn minecraft_dir() -> PathBuf {
    project_dir().join("minecraft")
}

pub fn minecraft_jar() -> PathBuf {
    minecraft_dir().join("minecraft.jar")
}

pub fn libraries_dir() -> PathBuf {
    minecraft_dir().join("libraries")
}

pub fn local_libraries_dir() -> PathBuf {
    minecraft_dir().join("local-libs")
}

pub fn natives_dir() -> PathBuf {
    minecraft_dir().join("natives")
}

pub fn assets_dir() -> PathBuf {
    minecraft_dir().join("assets")
}
pub fn maven_path(group: &str, artifact: &str, version: &str) -> PathBuf {
    let group_path = group.replace('.', "/");
    libraries_dir()
        .join(group_path)
        .join(artifact)
        .join(version)
        .join(format!("{}-{}.jar", artifact, version))
}

pub fn native_path(group: &str, artifact: &str, version: &str, classifier: &str) -> PathBuf {
    let group_path = group.replace('.', "/");
    libraries_dir()
        .join(group_path)
        .join(artifact)
        .join(version)
        .join(format!("{}-{}-{}.jar", artifact, version, classifier))
}
