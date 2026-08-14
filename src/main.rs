use eframe::egui;
use reqwest::blocking::Client;
use std::{
    env,
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
const VERSION: &str = env!("CARGO_PKG_VERSION");

const GITHUB_OWNER: &str = "cactus-spike1";
const GITHUB_REPO: &str = "LauncherMC";
const DEPENDENCIES: &[&str] = &[
    // LWJGL
    "org.lwjgl:lwjgl:3.2.2",
    "org.lwjgl:lwjgl-assimp:3.2.2",
    "org.lwjgl:lwjgl-glfw:3.2.2",
    "org.lwjgl:lwjgl-openal:3.2.2",
    "org.lwjgl:lwjgl-opengl:3.2.2",
    "org.lwjgl:lwjgl-vulkan:3.2.2",
    "org.lwjgl:lwjgl-stb:3.2.2",
    "org.lwjgl:lwjgl-tinyfd:3.2.2",

    // Другие зависимости
    "com.google.code.findbugs:jsr305:3.0.2",
    "org.apache.commons:commons-lang3:3.5",
    "commons-io:commons-io:2.5",
    "commons-codec:commons-codec:1.10",
    "commons-logging:commons-logging:1.1.3",
    "org.apache.commons:commons-compress:1.8.1",
    "org.apache.httpcomponents:httpclient:4.3.3",
    "org.apache.httpcomponents:httpcore:4.3.2",
    "org.apache.logging.log4j:log4j-api:2.16.0",
    "org.apache.logging.log4j:log4j-core:2.16.0",
    "com.google.code.gson:gson:2.8.0",
    "com.google.guava:guava:21.0",
    "io.netty:netty-all:4.1.25.Final",
    "net.sf.jopt-simple:jopt-simple:5.0.3",
    "it.unimi.dsi:fastutil:8.2.1",
    "net.java.dev.jna:jna:4.4.0",
    "ca.weblite:java-objc-bridge:1.0.0",
    "com.ibm.icu:icu4j:66.1",
];

const LWJGL_NATIVE_MODULES: &[&str] = &[
    "org.lwjgl:lwjgl:3.2.2",
    "org.lwjgl:lwjgl-assimp:3.2.2",
    "org.lwjgl:lwjgl-glfw:3.2.2",
    "org.lwjgl:lwjgl-openal:3.2.2",
    "org.lwjgl:lwjgl-opengl:3.2.2",
    "org.lwjgl:lwjgl-stb:3.2.2",
    "org.lwjgl:lwjgl-tinyfd:3.2.2",
];

#[derive(Debug, Clone, Copy)]
enum Platform {
    Windows,
    Linux,
    MacOS,
}

struct Launcher {
    username: String,
    status: String,
}

impl Default for Launcher {
    fn default() -> Self {
        Self {
            username: String::new(),
            status: format!(
                "Launcher v{}",
                VERSION
            ),
        }
    }
}
impl Launcher {
    // ------------------------------------------------------------
    // Пути
    // ------------------------------------------------------------

    fn project_dir() -> PathBuf {
        env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    }

    fn minecraft_dir() -> PathBuf {
        Self::project_dir().join("minecraft")
    }

    fn minecraft_jar() -> PathBuf {
        Self::minecraft_dir().join("minecraft.jar")
    }

    fn libraries_dir() -> PathBuf {
        Self::minecraft_dir().join("libraries")
    }

    fn local_libraries_dir() -> PathBuf {
        Self::minecraft_dir().join("local-libs")
    }

    fn natives_dir() -> PathBuf {
        Self::minecraft_dir().join("natives")
    }

    fn assets_dir() -> PathBuf {
        Self::minecraft_dir().join("assets")
    }

    // ------------------------------------------------------------
    // OS
    // ------------------------------------------------------------

    fn platform() -> Result<Platform, String> {
        if cfg!(target_os = "windows") {
            Ok(Platform::Windows)
        } else if cfg!(target_os = "linux") {
            Ok(Platform::Linux)
        } else if cfg!(target_os = "macos") {
            Ok(Platform::MacOS)
        } else {
            Err("Эта ОС не поддерживается".to_string())
        }
    }

    fn native_classifier(platform: Platform) -> &'static str {
        match platform {
            Platform::Windows => "natives-windows",
            Platform::Linux => "natives-linux",
            Platform::MacOS => "natives-macos",
        }
    }

    // ------------------------------------------------------------
    // Maven
    // ------------------------------------------------------------

    fn parse_dependency(dep: &str) -> Result<(&str, &str, &str), String> {
        let parts: Vec<&str> = dep.split(':').collect();

        if parts.len() != 3 {
            return Err(format!(
                "Неверная Maven dependency: {}",
                dep
            ));
        }

        Ok((parts[0], parts[1], parts[2]))
    }

    fn maven_url(group: &str, artifact: &str, version: &str) -> String {
        let group_path = group.replace('.', "/");

        format!(
            "https://repo1.maven.org/maven2/{}/{}/{}/{}-{}.jar",
            group_path,
            artifact,
            version,
            artifact,
            version
        )
    }

    fn native_url(
        group: &str,
        artifact: &str,
        version: &str,
        classifier: &str,
    ) -> String {
        let group_path = group.replace('.', "/");

        format!(
            "https://repo1.maven.org/maven2/{}/{}/{}/{}-{}-{}.jar",
            group_path,
            artifact,
            version,
            artifact,
            version,
            classifier
        )
    }

    fn maven_path(
        group: &str,
        artifact: &str,
        version: &str,
    ) -> PathBuf {
        let group_path = group.replace('.', "/");

        Self::libraries_dir()
            .join(group_path)
            .join(artifact)
            .join(version)
            .join(format!("{}-{}.jar", artifact, version))
    }

    // ------------------------------------------------------------
    // Download
    // ------------------------------------------------------------

    fn download_file(
        client: &Client,
        url: &str,
        destination: &Path,
    ) -> Result<(), String> {
        if destination.exists() {
            return Ok(());
        }

        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| {
                    format!(
                        "Не удалось создать {}: {}",
                        parent.display(),
                        e
                    )
                })?;
        }

        println!("Downloading: {}", url);

        let response = client
            .get(url)
            .send()
            .map_err(|e| {
                format!("Ошибка загрузки {}: {}", url, e)
            })?;

        if !response.status().is_success() {
            return Err(format!(
                "HTTP {}: {}",
                response.status(),
                url
            ));
        }

        let bytes = response
            .bytes()
            .map_err(|e| {
                format!("Ошибка чтения {}: {}", url, e)
            })?;

        let mut file = File::create(destination)
            .map_err(|e| {
                format!(
                    "Не удалось создать {}: {}",
                    destination.display(),
                    e
                )
            })?;

        file.write_all(&bytes)
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    // ------------------------------------------------------------
    // Скачивание обычных библиотек
    // ------------------------------------------------------------

    fn download_dependencies(
        &self,
        client: &Client,
    ) -> Result<(), String> {
        fs::create_dir_all(Self::libraries_dir())
            .map_err(|e| e.to_string())?;

        for dependency in DEPENDENCIES {
            let (group, artifact, version) =
                Self::parse_dependency(dependency)?;

            let url = Self::maven_url(
                group,
                artifact,
                version,
            );

            let destination = Self::maven_path(
                group,
                artifact,
                version,
            );

            Self::download_file(
                client,
                &url,
                &destination,
            )?;
        }

        Ok(())
    }

    // ------------------------------------------------------------
    // Natives
    // ------------------------------------------------------------

    fn native_path(
        group: &str,
        artifact: &str,
        version: &str,
        classifier: &str,
    ) -> PathBuf {
        let group_path = group.replace('.', "/");

        Self::libraries_dir()
            .join(group_path)
            .join(artifact)
            .join(version)
            .join(format!(
                "{}-{}-{}.jar",
                artifact,
                version,
                classifier
            ))
    }

    fn download_natives(
        &self,
        client: &Client,
    ) -> Result<Vec<PathBuf>, String> {
        let platform = Self::platform()?;
        let classifier = Self::native_classifier(platform);

        fs::create_dir_all(Self::libraries_dir())
            .map_err(|e| e.to_string())?;

        let mut native_jars = Vec::new();

        for dependency in LWJGL_NATIVE_MODULES {
            let (group, artifact, version) =
                Self::parse_dependency(dependency)?;

            let url = Self::native_url(
                group,
                artifact,
                version,
                classifier,
            );

            let destination = Self::native_path(
                group,
                artifact,
                version,
                classifier,
            );

            Self::download_file(
                client,
                &url,
                &destination,
            )?;

            native_jars.push(destination);
        }

        Ok(native_jars)
    }

    fn extract_natives(
        &self,
        native_jars: &[PathBuf],
    ) -> Result<(), String> {
        let natives_dir = Self::natives_dir();

        fs::create_dir_all(&natives_dir)
            .map_err(|e| e.to_string())?;

        for jar in native_jars {
            println!(
                "Extract natives: {}",
                jar.display()
            );

            let file = File::open(jar)
                .map_err(|e| {
                    format!(
                        "Не удалось открыть {}: {}",
                        jar.display(),
                        e
                    )
                })?;

            let mut archive = zip::ZipArchive::new(file)
                .map_err(|e| {
                    format!(
                        "Неверный native JAR {}: {}",
                        jar.display(),
                        e
                    )
                })?;

            for i in 0..archive.len() {
                let mut entry = archive
                    .by_index(i)
                    .map_err(|e| e.to_string())?;

                let name = entry.name().to_string();

                if entry.is_dir() {
                    continue;
                }

                // Берём только native-файлы.
                let is_native = name.ends_with(".dll")
                    || name.ends_with(".so")
                    || name.ends_with(".dylib");

                if !is_native {
                    continue;
                }

                let file_name = Path::new(&name)
                    .file_name()
                    .ok_or_else(|| {
                        format!(
                            "Некорректное имя native: {}",
                            name
                        )
                    })?;

                let destination =
                    natives_dir.join(file_name);

                let mut output =
                    File::create(&destination)
                        .map_err(|e| {
                            format!(
                                "Не удалось создать {}: {}",
                                destination.display(),
                                e
                            )
                        })?;

                io::copy(&mut entry, &mut output)
                    .map_err(|e| e.to_string())?;
            }
        }

        Ok(())
    }

    // ------------------------------------------------------------
    // JAR поиск
    // ------------------------------------------------------------

    fn find_jars(
        directory: &Path,
        result: &mut Vec<PathBuf>,
    ) -> Result<(), String> {
        if !directory.exists() {
            return Ok(());
        }

        let entries = fs::read_dir(directory)
            .map_err(|e| {
                format!(
                    "Не удалось прочитать {}: {}",
                    directory.display(),
                    e
                )
            })?;

        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();

            if path.is_dir() {
                Self::find_jars(&path, result)?;
            } else if path
                .extension()
                .map(|ext| {
                    ext.eq_ignore_ascii_case("jar")
                })
                .unwrap_or(false)
            {
                result.push(path);
            }
        }

        Ok(())
    }

    // ------------------------------------------------------------
    // Classpath
    // ------------------------------------------------------------

    fn build_classpath(&self) -> Result<String, String> {
        let minecraft_jar = Self::minecraft_jar();

        if !minecraft_jar.exists() {
            return Err(format!(
                "Не найден Minecraft JAR:\n{}",
                minecraft_jar.display()
            ));
        }

        let mut jars = Vec::<PathBuf>::new();

        // Maven libraries
        Self::find_jars(
            &Self::libraries_dir(),
            &mut jars,
        )?;

        // Локальные 8 библиотек
        Self::find_jars(
            &Self::local_libraries_dir(),
            &mut jars,
        )?;

        // Сам Minecraft
        jars.push(minecraft_jar);

        let separator = if cfg!(target_os = "windows") {
            ";"
        } else {
            ":"
        };

        Ok(jars
            .iter()
            .map(|path| {
                path.to_string_lossy().to_string()
            })
            .collect::<Vec<_>>()
            .join(separator))
    }

    // ------------------------------------------------------------
    // Java
    // ------------------------------------------------------------

    fn find_java() -> String {
        if cfg!(target_os = "windows") {
            "java.exe".to_string()
        } else {
            "java".to_string()
        }
    }

    fn check_java() -> Result<(), String> {
        let output = Command::new(Self::find_java())
            .arg("-version")
            .output()
            .map_err(|e| {
                format!(
                    "Java не найдена.\n\
                     Установи Java и добавь её в PATH.\n\n\
                     Ошибка: {}",
                    e
                )
            })?;

        if !output.status.success() {
            return Err(
                "Java установлена, но не запускается.".to_string()
            );
        }

        Ok(())
    }

    // ------------------------------------------------------------
    // Подготовка Minecraft
    // ------------------------------------------------------------

    fn prepare(&self) -> Result<String, String> {
        let minecraft_dir = Self::minecraft_dir();

        fs::create_dir_all(&minecraft_dir)
            .map_err(|e| e.to_string())?;

        fs::create_dir_all(Self::libraries_dir())
            .map_err(|e| e.to_string())?;

        fs::create_dir_all(Self::local_libraries_dir())
            .map_err(|e| e.to_string())?;

        fs::create_dir_all(Self::assets_dir())
            .map_err(|e| e.to_string())?;

        fs::create_dir_all(Self::natives_dir())
            .map_err(|e| e.to_string())?;

        let client = Client::builder()
            .user_agent("SimpleMinecraftLauncher/1.0")
            .build()
            .map_err(|e| e.to_string())?;

        // Попробовать авто-загрузить minecraft.jar и local-libs из релиза на GitHub
        // делаем это до проверки наличия JAR, чтобы скачать его при отсутствии
        Self::ensure_minecraft_artifacts(&client).ok();

        if !Self::minecraft_jar().exists() {
            return Err(format!(
                "Не найден:\n{}",
                Self::minecraft_jar().display()
            ));
        }

        Self::check_java()?;

        // Обычные зависимости
        self.download_dependencies(&client)?;

        // Native JAR
        let native_jars =
            self.download_natives(&client)?;

        // Распаковываем DLL/SO/dylib
        self.extract_natives(&native_jars)?;

        // Строим classpath
        self.build_classpath()
    }

    // ------------------------------------------------------------
    // Запуск
    // ------------------------------------------------------------

    fn launch(&mut self) {
        let username = self.username.trim().to_string();

        if username.is_empty() {
            self.status = "Введите ник".to_string();
            return;
        }

        if username.len() > 16 {
            self.status =
                "Ник не должен быть длиннее 16 символов"
                    .to_string();
            return;
        }

        if !username
            .chars()
            .all(|c| {
                c.is_ascii_alphanumeric() || c == '_'
            })
        {
            self.status =
                "Ник может содержать только A-Z, 0-9 и _"
                    .to_string();
            return;
        }

        self.status =
            "Подготовка Minecraft...".to_string();

        let classpath = match self.prepare() {
            Ok(cp) => cp,

            Err(error) => {
                self.status = error;
                return;
            }
        };

        let minecraft_dir =
            Self::minecraft_dir();

        let natives_dir =
            Self::natives_dir();

        let assets_dir =
            Self::assets_dir();

        println!("Minecraft directory:");
        println!("{}", minecraft_dir.display());

        println!("Username: {}", username);

        let main_class =
            "net.minecraft.client.main.Main";

        let mut command =
            Command::new(Self::find_java());

        command
            .current_dir(&minecraft_dir)

            // JVM
            .arg("-Xms512M")
            .arg("-Xmx2G")

            // LWJGL natives
            .arg(format!(
                "-Dorg.lwjgl.librarypath={}",
                natives_dir.display()
            ))

            // Classpath
            .arg("-cp")
            .arg(classpath)

            // Main class
            .arg(main_class)

            // Minecraft arguments
            .arg("--username")
            .arg(&username)

            .arg("--version")
            .arg("1.16.5")

            .arg("--gameDir")
            .arg(&minecraft_dir)

            .arg("--assetsDir")
            .arg(&assets_dir)

            .arg("--assetIndex")
            .arg("1.16")

            // Offline UUID
            .arg("--uuid")
            .arg("00000000-0000-0000-0000-000000000000")

            // Offline token
            .arg("--accessToken")
            .arg("0")

            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());

        println!("Запуск Minecraft...");

        match command.spawn() {
            Ok(_) => {
                self.status =
                    "Minecraft запущен".to_string();
            }

            Err(error) => {
                self.status = format!(
                    "Не удалось запустить Minecraft:\n{}",
                    error
                );
            }
        }
    }
    fn check_update_on_start(&mut self) {
        self.status = "Проверка обновлений...".to_string();

        match check_for_update() {
            Ok(Some(url)) => {
                self.status = "Доступно обновление".to_string();

                if let Err(error) = start_updater(&url) {
                    self.status = format!("Ошибка обновления: {}", error);
                } else {
                    std::process::exit(0);
                }
            }

            Ok(None) => {
                self.status = format!("Launcher v{} — обновлений нет", VERSION);
            }

            Err(error) => {
                self.status = format!(
                    "Не удалось проверить обновления: {}",
                    error
                );
            }
        }
    }

}

// ------------------------------------------------------------
// Обновления
// ------------------------------------------------------------

fn check_for_update() -> Result<Option<String>, String> {
    let api = format!(
        "https://api.github.com/repos/{}/{}/releases/latest",
        GITHUB_OWNER, GITHUB_REPO
    );

    let client = Client::builder()
        .user_agent("LauncherMC-Updater")
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client.get(&api).send().map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        return Err(format!("GitHub API error: {}", resp.status()));
    }

    let json: serde_json::Value = resp.json().map_err(|e| e.to_string())?;

    let tag = json
        .get("tag_name")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if tag == VERSION {
        return Ok(None);
    }

    let assets = json
        .get("assets")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "No assets in release".to_string())?;

    // Prefer an asset matching platform + launcher name, fallback to first
    for asset in assets {
        if let Some(url) = asset
            .get("browser_download_url")
            .and_then(|v| v.as_str())
        {
            return Ok(Some(url.to_string()));
        }
    }

    Ok(None)
}

fn start_updater(url: &str) -> Result<(), String> {
    let current = env::current_exe().map_err(|e| e.to_string())?;

    let mut updater_path = current.clone();

    let updater_name = if cfg!(target_os = "windows") {
        "LauncherMCupdater.exe"
    } else {
        "LauncherMCupdater"
    };

    updater_path.set_file_name(updater_name);

    if !updater_path.exists() {
        return Err(format!(
            "Updater not found: {}",
            updater_path.display()
        ));
    }

    Command::new(&updater_path)
        .arg("--url")
        .arg(url)
        .arg("--target")
        .arg(
            current
                .to_str()
                .ok_or_else(|| "Invalid executable path".to_string())?,
        )
        .spawn()
        .map_err(|e| e.to_string())?;

    Ok(())
}

impl Launcher {
    fn ensure_minecraft_artifacts(client: &Client) -> Result<(), String> {
        let jar = Self::minecraft_jar();

        // If jar already exists, nothing to do for jar
        let mut found_jar = jar.exists();

        // Prepare local-libs path
        let local_libs_dir = Self::local_libraries_dir();

        // Query latest release on GitHub
        let api = format!(
            "https://api.github.com/repos/{}/{}/releases/latest",
            GITHUB_OWNER, GITHUB_REPO
        );

        let resp = client.get(&api).send().map_err(|e| e.to_string())?;

        if !resp.status().is_success() {
            return Err(format!("GitHub API error: {}", resp.status()));
        }

        let json: serde_json::Value = resp.json().map_err(|e| e.to_string())?;

        let assets = json
            .get("assets")
            .and_then(|v| v.as_array())
            .ok_or_else(|| "No assets in release".to_string())?;

        for asset in assets {
            let name = asset
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let url = asset
                .get("browser_download_url")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            if name == "minecraft.jar" && !found_jar {
                Self::download_file(client, url, &jar)?;
                found_jar = true;
            }

            if name == "local-libs.zip" {
                let zip_path = Self::minecraft_dir().join("local-libs.zip");
                Self::download_file(client, url, &zip_path)?;

                // Extract safely into local-libs
                fs::create_dir_all(&local_libs_dir).map_err(|e| e.to_string())?;

                let file = File::open(&zip_path).map_err(|e| e.to_string())?;
                let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;

                for i in 0..archive.len() {
                    let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
                    let name = entry.name().to_string();

                    // Skip suspicious paths
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

                    let mut outfile = File::create(&outpath).map_err(|e| e.to_string())?;
                    io::copy(&mut entry, &mut outfile).map_err(|e| e.to_string())?;
                }

                let _ = fs::remove_file(&zip_path);
            }
        }

        Ok(())
    }
}
 

// ------------------------------------------------------------
// GUI
// ------------------------------------------------------------

impl eframe::App for Launcher {
    fn update(
        &mut self,
        ctx: &egui::Context,
        _frame: &mut eframe::Frame,
    ) {
        egui::CentralPanel::default()
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(40.0);

                    ui.heading(
                        "Minecraft 1.16.5",
                    );

                    ui.add_space(20.0);

                    ui.label("Ник:");

                    let response = ui.add(
                        egui::TextEdit::singleline(
                            &mut self.username,
                        )
                        .hint_text("Steve")
                        .desired_width(250.0),
                    );

                    if response.lost_focus()
                        && ui.input(|input| {
                            input.key_pressed(
                                egui::Key::Enter
                            )
                        })
                    {
                        self.launch();
                    }

                    ui.add_space(10.0);

                    if ui
                        .add_sized(
                            [250.0, 40.0],
                            egui::Button::new(
                                "Запустить",
                            ),
                        )
                        .clicked()
                    {
                        self.launch();
                    }

                    ui.add_space(15.0);

                    ui.label(&self.status);

                    ui.add_space(30.0);

                    ui.small(
                        "Custom Minecraft 1.16.5",
                    );

                    ui.small(
                        "Offline / no authentication",
                    );
                });
            });
    }
}

// ------------------------------------------------------------
// Main
// ------------------------------------------------------------

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport:
            egui::ViewportBuilder::default()
                .with_inner_size([400.0, 300.0])
                .with_min_inner_size([350.0, 250.0]),

        ..Default::default()
    };

    eframe::run_native(
        "Minecraft 1.16.5 Launcher",
        options,
        Box::new(|_cc| {
            let mut launcher =
                Launcher::default();

            launcher.check_update_on_start();

            Ok(Box::new(launcher))
        }),
    )
}