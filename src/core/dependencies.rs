use std::fs;
use std::io::Write;

use std::path::{Path, PathBuf};

use reqwest::blocking::Client;
use std::io::copy;
use zip::ZipArchive;

use super::paths::*;

const DEPENDENCIES: &[&str] = &[
    "com.google.code.findbugs:jsr305:3.0.2",
    "org.apache.commons:commons-lang3:3.18.0",
    "commons-io:commons-io:2.19.0",
    "commons-codec:commons-codec:1.18.0",
    "commons-logging:commons-logging:1.1.3.5",
    "org.apache.commons:commons-compress:1.28.0",
    "org.apache.httpcomponents.client5:httpclient5:5.6.4",
    "org.apache.httpcomponents.core5:httpcore5:5.4.3",
    "org.apache.logging.log4j:log4j-api:2.24.3",
    "org.apache.logging.log4j:log4j-core:2.24.3",
    "com.google.code.gson:gson:2.12.1",
    "com.google.guava:guava:33.4.0-jre",
    "io.netty:netty-all:4.1.25.Final",
    "net.sf.jopt-simple:jopt-simple:5.0.4",
    "it.unimi.dsi:fastutil:8.5.14",
    "net.java.dev.jna:jna:5.16.0",
    "ca.weblite:java-objc-bridge:1.0.0",
    "com.ibm.icu:icu4j:76.1",
    "net.java.dev.jna:jna-platform:5.16.0",
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

    core_log("Sending HTTP request...");

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

    core_log(&format!("Content-Length: {} bytes", content_length));

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

        loop {
            match std::io::Read::read(&mut reader, &mut buffer) {
                Ok(0) => break, // EOF
                Ok(n) => {
                    file.write_all(&buffer[..n]).map_err(|e| e.to_string())?;

                    downloaded += n as u64;

                    if content_length > 0 {
                        let percent = (downloaded as f64 / content_length as f64) * 100.0;
                        core_log(&format!(
                            "Downloaded: {} / {} bytes ({:.1}%)",
                            downloaded, content_length, percent
                        ));
                    } else {
                        core_log(&format!("Downloaded: {} bytes", downloaded));
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

        core_log(&format!("Download complete: {} bytes total", downloaded));
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

    core_log(&format!("File saved to: {}", destination.display()));

    Ok(())
}

// Вспомогательная функция для проверки архива
fn is_archive_file(path: &Path) -> bool {
    path.extension()
        .and_then(|x| x.to_str())
        .map(|x| x.eq_ignore_ascii_case("zip") || x.eq_ignore_ascii_case("jar"))
        .unwrap_or(false)
}

pub fn ensure_minecraft_artifacts(client: &Client) -> Result<(), String> {
    let jar = minecraft_jar();
    let local_libs_dir = local_libraries_dir();

    // Прямые URL для Git LFS файлов
    let jar_url = "https://media.githubusercontent.com/media/cactus-spike1/LauncherMC/refs/heads/main/mc/minecraft.jar";
    let libs_url = "https://media.githubusercontent.com/media/cactus-spike1/LauncherMC/refs/heads/main/mc/local-libs.zip";

    // Скачиваем minecraft.jar
    if !jar.exists() {
        download_file(client, jar_url, &jar)?;
    }

    // Скачиваем и распаковываем local-libs.zip
    let zip_path = minecraft_dir().join("local-libs.zip");
    download_file(client, libs_url, &zip_path)?;

    fs::create_dir_all(&local_libs_dir).map_err(|e| e.to_string())?;

    let file = fs::File::open(&zip_path).map_err(|e| e.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|e| e.to_string())?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().to_string();

        if name.contains("..") || name.starts_with('/') {
            continue;
        }

        let outpath = local_libs_dir.join(&name);

        if entry.is_dir() {
            fs::create_dir_all(&outpath).map_err(|e| e.to_string())?;
            continue;
        }

        if let Some(parent) = outpath.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }

        let mut outfile = fs::File::create(&outpath).map_err(|e| e.to_string())?;
        copy(&mut entry, &mut outfile).map_err(|e| e.to_string())?;
    }

    let _ = fs::remove_file(&zip_path);

    Ok(())
}

pub fn download_dependencies() -> Result<(), String> {
    let client = Client::builder()
        .user_agent("SimpleMinecraftLauncher/1.0")
        .build()
        .map_err(|e| e.to_string())?;
    fs::create_dir_all(libraries_dir()).map_err(|e| e.to_string())?;
    for dependency in DEPENDENCIES {
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
    for dependency in LWJGL_NATIVE_MODULES {
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
