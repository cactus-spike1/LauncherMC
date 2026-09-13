use crate::dependencies;
use crate::paths;
use reqwest::blocking::Client;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use uuid::Uuid;

fn find_jars(directory: &Path, result: &mut Vec<PathBuf>) -> Result<(), String> {
    if !directory.exists() {
        return Ok(());
    }
    let entries = fs::read_dir(directory).map_err(|e| e.to_string())?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.is_dir() {
            find_jars(&path, result)?;
        } else if path
            .extension()
            .map(|ext| ext.eq_ignore_ascii_case("jar"))
            .unwrap_or(false)
        {
            result.push(path);
        }
    }
    Ok(())
}

fn build_classpath() -> Result<String, String> {
    let minecraft = paths::minecraft_jar();
    if !minecraft.exists() {
        return Err(format!("Minecraft JAR not found: {}", minecraft.display()));
    }
    let mut jars = Vec::<PathBuf>::new();
    find_jars(&paths::libraries_dir(), &mut jars)?;
    find_jars(&paths::local_libraries_dir(), &mut jars)?;
    jars.push(minecraft);
    let separator = if cfg!(target_os = "windows") {
        ";"
    } else {
        ":"
    };
    Ok(jars
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join(separator))
}

fn find_java() -> String {
    if cfg!(target_os = "windows") {
        "java.exe".to_string()
    } else {
        "java".to_string()
    }
}

fn check_java() -> Result<(), String> {
    let output = Command::new(find_java())
        .arg("-version")
        .output()
        .map_err(|e| format!("Java not found: {}", e))?;
    if !output.status.success() {
        return Err("Java present but not running".to_string());
    }
    Ok(())
}

pub fn prepare_and_launch(username: &str) -> Result<u32, String> {
    // create dirs
    fs::create_dir_all(paths::minecraft_dir()).map_err(|e| e.to_string())?;
    fs::create_dir_all(paths::libraries_dir()).map_err(|e| e.to_string())?;
    fs::create_dir_all(paths::local_libraries_dir()).map_err(|e| e.to_string())?;
    fs::create_dir_all(paths::assets_dir()).map_err(|e| e.to_string())?;
    fs::create_dir_all(paths::natives_dir()).map_err(|e| e.to_string())?;

    let client = Client::builder()
        .user_agent("SimpleMinecraftLauncher/1.0")
        .build()
        .map_err(|e| e.to_string())?;

    dependencies::ensure_minecraft_artifacts(&client).map_err(|e| e.to_string())?;
    if !paths::minecraft_jar().exists() {
        return Err(format!(
            "Minecraft JAR not found: {}",
            paths::minecraft_jar().display()
        ));
    }
    check_java().map_err(|e| e.to_string())?;
    dependencies::download_dependencies().map_err(|e| e.to_string())?;
    let native_jars = dependencies::download_natives().map_err(|e| e.to_string())?;
    dependencies::extract_natives(&native_jars).map_err(|e| e.to_string())?;
    let classpath = build_classpath().map_err(|e| e.to_string())?;

    let minecraft_dir = paths::minecraft_dir();
    let natives = paths::natives_dir();
    let assets = paths::assets_dir();

    let main_class = "net.minecraft.client.main.Main";

    let mut bytes = [0u8; 16];
    let username_bytes = username.as_bytes();
    bytes[..username_bytes.len().min(16)]
        .copy_from_slice(&username_bytes[..username_bytes.len().min(16)]);
    let uuid_player = Uuid::from_bytes(bytes);
    let mut command = Command::new(find_java());
    command
        .current_dir(&minecraft_dir)
        .arg("-Xms512M")
        .arg("-Xmx2G")
        .arg(format!("-Dorg.lwjgl.librarypath={}", natives.display()))
        .arg("-cp")
        .arg(classpath)
        .arg(main_class)
        .arg("--username")
        .arg(username)
        .arg("--version")
        .arg("1.16.5")
        .arg("--gameDir")
        .arg(&minecraft_dir)
        .arg("--assetsDir")
        .arg(&assets)
        .arg("--assetIndex")
        .arg("1.16")
        .arg("--uuid")
        .arg(uuid_player.to_string())
        .arg("--accessToken")
        .arg("0")
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    match command.spawn() {
        Ok(child) => Ok(child.id()),
        Err(e) => Err(format!("failed to spawn java: {}", e)),
    }
}
