use std::fs;
use std::io::Write;

use std::path::{Path, PathBuf};
use std::time::Instant;

use super::paths::*;
use reqwest::blocking::Client;
use sha2::{Digest, Sha256};
use std::io::{copy, Read};

use zip::ZipArchive;

const DEPENDENCIES: &[&str] = &[
    "com.google.code.findbugs:jsr305:3.0.2",
    "org.apache.commons:commons-lang3:3.18.0",
    "commons-io:commons-io:2.19.0",
    "commons-codec:commons-codec:1.18.0",
    "commons-logging:commons-logging:1.3.5",
    "org.apache.commons:commons-compress:1.28.0",
    "org.apache.httpcomponents.client5:httpclient5:5.6.4",
    "org.apache.httpcomponents.core5:httpcore5:5.4.3",
    "org.apache.logging.log4j:log4j-api:2.24.3",
    "org.apache.logging.log4j:log4j-core:2.24.3",
    "com.google.code.gson:gson:2.12.1",
    "com.google.guava:guava:33.4.0-jre",
    "com.google.guava:failureaccess:1.0.3",
    "io.netty:netty-all:4.1.25.Final",
    "net.sf.jopt-simple:jopt-simple:5.0.4",
    "it.unimi.dsi:fastutil:8.5.14",
    "net.java.dev.jna:jna:5.16.0",
    "ca.weblite:java-objc-bridge:1.0.0",
    "com.ibm.icu:icu4j:76.1",
    "net.java.dev.jna:jna-platform:5.16.0",
    "org.lwjgl:lwjgl:3.4.3",
    "org.lwjgl:lwjgl-assimp:3.4.3",
    "org.lwjgl:lwjgl-glfw:3.4.3",
    "org.lwjgl:lwjgl-openal:3.4.3",
    "org.lwjgl:lwjgl-opengl:3.4.3",
    "org.lwjgl:lwjgl-stb:3.4.3",
    "org.lwjgl:lwjgl-tinyfd:3.4.3",
];

const LWJGL_NATIVE_MODULES: &[&str] = &[
    "org.lwjgl:lwjgl:3.4.3",
    "org.lwjgl:lwjgl-assimp:3.4.3",
    "org.lwjgl:lwjgl-glfw:3.4.3",
    "org.lwjgl:lwjgl-openal:3.4.3",
    "org.lwjgl:lwjgl-opengl:3.4.3",
    "org.lwjgl:lwjgl-stb:3.4.3",
    "org.lwjgl:lwjgl-tinyfd:3.4.3",
];

fn download_file(client: &Client, url: &str, destination: &Path) -> Result<(), String> {
    let display_name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file");

    // Проверка существующего файла
    if destination.exists() {
        core_log(&format!(
            "Destination already exists, size={}",
            fs::metadata(destination).map(|m| m.len()).unwrap_or(0)
        ));

        if is_archive_file(destination) {
            if is_valid_zip(destination) {
                core_log("Existing archive is valid");
                return Ok(());
            }
            core_log("Existing archive is INVALID, deleting");
            fs::remove_file(destination)
                .map_err(|e| format!("failed removing corrupted file: {}", e))?;
        } else {
            return Ok(());
        }
    }

    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    progress_log(&format!("Downloading {}: connecting...", display_name));

    // Добавляем таймауты и follow_redirects
    let response = client
        .get(url)
        .header("Accept", "application/octet-stream")
        .timeout(std::time::Duration::from_secs(300)) // 5 минут на весь запрос
        .send()
        .map_err(|e| {
            let msg = format!("HTTP request failed: {}", e);
            core_log(&msg);
            msg
        })?;

    core_log(&format!("HTTP STATUS: {}", response.status()));

    if !response.status().is_success() {
        let status = response.status();
        let body = response
            .text()
            .unwrap_or_else(|_| "<cannot read body>".to_string());
        core_log(&format!("ERROR BODY: {}", body));
        return Err(format!("HTTP {}", status));
    }

    let content_length = response.content_length().unwrap_or(0);

    progress_log(&format!(
        "Downloading {}: 0.0% ({} bytes)",
        display_name, content_length
    ));

    let temp = destination.with_extension("download");

    // Удаляем старый временный файл если существует
    let _ = fs::remove_file(&temp);

    core_log(&format!("Writing to temporary file: {}", temp.display()));

    {
        let mut file = fs::File::create(&temp).map_err(|e| e.to_string())?;

        // Streaming загрузка с прогрессом
        let mut downloaded = 0u64;
        let mut buffer = vec![0u8; 1024 * 1024]; // 1 МБ буфер
        let mut reader = response;
        let started = Instant::now();
        let mut last_reported_percent = 0u64;

        loop {
            match std::io::Read::read(&mut reader, &mut buffer) {
                Ok(0) => break, // EOF
                Ok(n) => {
                    file.write_all(&buffer[..n]).map_err(|e| e.to_string())?;

                    downloaded += n as u64;

                    let percent = if content_length > 0 {
                        (downloaded.saturating_mul(100) / content_length) as u64
                    } else {
                        0
                    };
                    if content_length > 0 && percent > last_reported_percent {
                        last_reported_percent = percent;
                        let speed = downloaded as f64 / started.elapsed().as_secs_f64().max(0.001);
                        progress_log(&format!(
                            "Downloading {}: {}% ({:.1} MB/s)",
                            display_name,
                            percent,
                            speed / 1_048_576.0
                        ));
                    } else if content_length == 0 {
                        let speed = downloaded as f64 / started.elapsed().as_secs_f64().max(0.001);
                        progress_log(&format!(
                            "Downloading {}: {} bytes ({:.1} MB/s)",
                            display_name,
                            downloaded,
                            speed / 1_048_576.0
                        ));
                    }
                }
                Err(e) => {
                    let msg = format!("failed reading response: {}", e);
                    core_log(&msg);
                    let _ = fs::remove_file(&temp);
                    return Err(msg);
                }
            }
        }

        file.sync_all().map_err(|e| e.to_string())?;

        progress_log(&format!(
            "Downloaded {}: 100% ({} bytes)",
            display_name, downloaded
        ));
    }

    // Валидация архива
    if is_archive_file(destination) {
        core_log("Checking downloaded archive...");

        if !is_valid_zip(&temp) {
            core_log("ARCHIVE IS INVALID!");
            let _ = fs::remove_file(&temp);
            return Err(format!("downloaded archive is corrupted: {}", url));
        }

        core_log("Archive is VALID");
    }

    // Атомарный rename
    fs::rename(&temp, destination).map_err(|e| format!("failed moving file: {}", e))?;

    progress_log(&format!("Ready: {}", display_name));

    Ok(())
}

// Вспомогательная функция для проверки архива
fn is_archive_file(path: &Path) -> bool {
    path.extension()
        .and_then(|x| x.to_str())
        .map(|x| x.eq_ignore_ascii_case("zip") || x.eq_ignore_ascii_case("jar"))
        .unwrap_or(false)
}
fn fetch_sha256(client: &Client, url: &str) -> Result<String, String> {
    let response = client
        .get(url)
        .send()
        .map_err(|e| format!("failed to download checksum: {e}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "checksum request returned HTTP {}",
            response.status()
        ));
    }

    let body = response
        .text()
        .map_err(|e| format!("failed to read checksum: {e}"))?;

    // Принимает как просто хэш, так и формат "хэш  имя-файла".
    let hash = body
        .split_whitespace()
        .next()
        .ok_or_else(|| "checksum file is empty".to_string())?
        .to_ascii_lowercase();

    if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("checksum file does not contain a valid SHA-256 hash".to_string());
    }

    Ok(hash)
}

fn ensure_file_matches_sha256(
    client: &Client,
    name: &str,
    file_url: &str,
    checksum_url: &str,
    destination: &Path,
) -> Result<(), String> {
    let expected_hash = fetch_sha256(client, checksum_url)?;

    let matches = destination.exists()
        && sha256_file(destination)
            .map(|hash| hash.eq_ignore_ascii_case(&expected_hash))
            .unwrap_or(false);

    if matches {
        progress_log(&format!("{name}: SHA-256 matches"));
        return Ok(());
    }

    progress_log(&format!("{name} is missing or outdated; downloading"));

    if destination.exists() {
        fs::remove_file(destination)
            .map_err(|e| format!("failed to remove outdated {name}: {e}"))?;
    }

    download_file(client, file_url, destination)?;

    let actual_hash = sha256_file(destination)?;
    if !actual_hash.eq_ignore_ascii_case(&expected_hash) {
        let _ = fs::remove_file(destination);
        return Err(format!(
            "{name} SHA-256 mismatch: expected {expected_hash}, got {actual_hash}"
        ));
    }

    progress_log(&format!("{name} downloaded and SHA-256 verified"));
    Ok(())
}

pub fn ensure_minecraft_artifacts(client: &Client) -> Result<(), String> {
    const MEDIA_BASE: &str =
        "https://media.githubusercontent.com/media/cactus-spike1/LauncherMC/refs/heads/main/mc";
    const RAW_BASE: &str = "https://raw.githubusercontent.com/cactus-spike1/LauncherMC/main/mc";

    let jar = minecraft_jar();
    let zip_path = minecraft_dir().join("local-libs.zip");
    let local_libs_dir = local_libraries_dir();

    ensure_file_matches_sha256(
        client,
        "minecraft.jar",
        &format!("{MEDIA_BASE}/minecraft.jar"),
        &format!("{RAW_BASE}/minecraft.jar.sha256"),
        &jar,
    )?;

    ensure_file_matches_sha256(
        client,
        "local-libs.zip",
        &format!("{MEDIA_BASE}/local-libs.zip"),
        &format!("{RAW_BASE}/local-libs.zip.sha256"),
        &zip_path,
    )?;

    fs::create_dir_all(&local_libs_dir).map_err(|e| e.to_string())?;

    let file = fs::File::open(&zip_path).map_err(|e| e.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|e| e.to_string())?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name();

        if name.contains("..") || name.starts_with('/') {
            continue;
        }

        let output_path = local_libs_dir.join(name);

        if entry.is_dir() {
            fs::create_dir_all(&output_path).map_err(|e| e.to_string())?;
            continue;
        }

        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }

        let mut output = fs::File::create(&output_path).map_err(|e| e.to_string())?;
        copy(&mut entry, &mut output).map_err(|e| e.to_string())?;
    }

    fs::remove_file(&zip_path).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn download_dependencies() -> Result<(), String> {
    let client = Client::builder()
        .user_agent("SimpleMinecraftLauncher/1.0")
        .build()
        .map_err(|e| e.to_string())?;
    fs::create_dir_all(libraries_dir()).map_err(|e| e.to_string())?;
    let total = DEPENDENCIES.len();
    for (index, dependency) in DEPENDENCIES.iter().enumerate() {
        progress_log(&format!("Libraries: {}/{}", index + 1, total));
        let (group, artifact, version) = parse_dependency(dependency)?;
        let url = maven_url(group, artifact, version);
        let destination = maven_path(group, artifact, version);
        download_file(&client, &url, &destination)?;
    }
    Ok(())
}

pub fn download_natives() -> Result<Vec<PathBuf>, String> {
    let client = Client::builder()
        .user_agent("SimpleMinecraftLauncher/1.0")
        .build()
        .map_err(|e| e.to_string())?;
    let platform = if cfg!(target_os = "linux") {
        "natives-linux"
    } else if cfg!(target_os = "windows") {
        "natives-windows"
    } else {
        "natives-macos"
    };
    fs::create_dir_all(libraries_dir()).map_err(|e| e.to_string())?;
    let mut native_jars = Vec::new();
    let total = LWJGL_NATIVE_MODULES.len();
    for (index, dependency) in LWJGL_NATIVE_MODULES.iter().enumerate() {
        progress_log(&format!("Native libraries: {}/{}", index + 1, total));
        let (group, artifact, version) = parse_dependency(dependency)?;
        let url = native_url(group, artifact, version, platform);
        let destination = native_path(group, artifact, version, platform);
        download_file(&client, &url, &destination)?;
        native_jars.push(destination);
    }
    Ok(native_jars)
}

pub fn extract_natives(native_jars: &[PathBuf]) -> Result<(), String> {
    let natives = natives_dir();
    fs::create_dir_all(&natives).map_err(|e| e.to_string())?;
    for jar in native_jars {
        eprintln!("Extract natives: {}", jar.display());
        let file = fs::File::open(jar).map_err(|e| e.to_string())?;
        let mut archive = ZipArchive::new(file).map_err(|e| e.to_string())?;
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
            let name = entry.name().to_string();
            if entry.is_dir() {
                continue;
            }
            let is_native =
                name.ends_with(".dll") || name.ends_with(".so") || name.ends_with(".dylib");
            if !is_native {
                continue;
            }
            let file_name = Path::new(&name)
                .file_name()
                .ok_or_else(|| format!("invalid native name: {}", name))?;
            let destination = natives.join(file_name);
            let mut output = fs::File::create(&destination).map_err(|e| e.to_string())?;
            copy(&mut entry, &mut output).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn is_valid_zip(path: &Path) -> bool {
    if !path.exists() {
        return false;
    }

    match fs::File::open(path) {
        Ok(file) => ZipArchive::new(file).is_ok(),
        Err(_) => false,
    }
}

fn core_log(message: &str) {
    let path = minecraft_dir().join("core.log");

    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(file, "{}", message);
    }
}

fn progress_log(message: &str) {
    core_log(message);
    eprintln!("Core: {}", message);
}

fn parse_dependency(dep: &str) -> Result<(&str, &str, &str), String> {
    let parts: Vec<&str> = dep.split(':').collect();
    if parts.len() != 3 {
        return Err(format!("Invalid dependency: {}", dep));
    }
    Ok((parts[0], parts[1], parts[2]))
}

fn maven_url(group: &str, artifact: &str, version: &str) -> String {
    let group_path = group.replace('.', "/");
    format!(
        "https://repo1.maven.org/maven2/{}/{}/{}/{}-{}.jar",
        group_path, artifact, version, artifact, version
    )
}

fn native_url(group: &str, artifact: &str, version: &str, classifier: &str) -> String {
    let group_path = group.replace('.', "/");
    format!(
        "https://repo1.maven.org/maven2/{}/{}/{}/{}-{}-{}.jar",
        group_path, artifact, version, artifact, version, classifier
    )
}
