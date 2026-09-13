use eframe::egui;
use std::{
    env,
    io::{BufRead, BufReader, Write},
};

use std::net::TcpStream;

const VERSION: &str = env!("CARGO_PKG_VERSION");

struct Launcher {
    username: String,
    status: String,
    stream: Option<TcpStream>,
    reader: Option<BufReader<TcpStream>>,
}

impl Default for Launcher {
    fn default() -> Self {
        Self {
            username: String::new(),
            status: format!("Launcher v{}", VERSION),
            stream: None,
            reader: None,
        }
    }
}
impl Launcher {
    fn connect_core(&mut self) -> Result<(), String> {
        let addr = "127.0.0.1:31337";

        let stream = TcpStream::connect(addr).map_err(|e| format!("connect error: {}", e))?;

        stream.set_nodelay(true).ok();
        self.reader = Some(BufReader::new(stream.try_clone().unwrap()));
        self.stream = Some(stream);

        Ok(())
    }

    fn send_core_command(
        &mut self,
        cmd: &str,
        args: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, String> {
        let stream = self.stream.as_mut().ok_or("Core not connected")?;
        let reader = self.reader.as_mut().ok_or("Core reader missing")?;

        let mut msg = serde_json::json!({ "cmd": cmd });

        if let Some(a) = args {
            msg["args"] = a;
        }

        let s = serde_json::to_string(&msg).map_err(|e| e.to_string())?;
        stream.write_all(s.as_bytes()).map_err(|e| e.to_string())?;
        stream.write_all(b"\n").map_err(|e| e.to_string())?;

        let mut resp = String::new();
        reader.read_line(&mut resp).map_err(|e| e.to_string())?;

        let v: serde_json::Value = serde_json::from_str(resp.trim()).map_err(|e| e.to_string())?;

        Ok(v)
    }

    fn format_core(&self, v: &serde_json::Value) -> String {
        // Показываем cmd/status/msg/running в удобном виде, иначе pretty-print
        if let Some(cmd) = v.get("cmd").and_then(|c| c.as_str()) {
            let status = v.get("status").and_then(|s| s.as_str()).unwrap_or("");
            let msg = v.get("msg").and_then(|m| m.as_str()).unwrap_or("");
            let running = v.get("running").and_then(|r| r.as_bool());

            let mut parts = Vec::new();
            parts.push(format!("cmd={}", cmd));
            if !status.is_empty() {
                parts.push(format!("status={}", status));
            }
            if !msg.is_empty() {
                parts.push(format!("msg={}", msg));
            }
            if let Some(r) = running {
                parts.push(format!("running={}", r));
            }

            return parts.join(" ");
        }

        if let Some(status) = v.get("status").and_then(|s| s.as_str()) {
            let msg = v.get("msg").and_then(|m| m.as_str()).unwrap_or("");
            if !msg.is_empty() {
                return format!("{} - {}", status, msg);
            }
            if let Some(running) = v.get("running").and_then(|r| r.as_bool()) {
                return format!("{} running={}", status, running);
            }
            return status.to_string();
        }

        serde_json::to_string_pretty(v).unwrap_or_else(|_| v.to_string())
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
            self.status = "Ник не должен быть длиннее 16 символов".to_string();
            return;
        }

        if !username
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            self.status = "Ник может содержать только A-Z, 0-9 и _".to_string();
            return;
        }

        // Delegate launch to core service
        self.status = "Запуск через core...".to_string();

        match self.send_core_command("launch", Some(serde_json::json!({"username": username}))) {
            Ok(v) => self.status = format!("Core: {}", self.format_core(&v)),
            Err(e) => {
                let v = serde_json::json!({ "status": "error", "msg": e.to_string() });
                self.status = format!("Core error: {}", self.format_core(&v));
            }
        }
    }
}

// ------------------------------------------------------------
// GUI
// ------------------------------------------------------------

impl eframe::App for Launcher {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(40.0);

                ui.heading("Minecraft 1.16.5");

                ui.add_space(20.0);

                ui.label("Ник:");

                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.username)
                        .hint_text("Steve")
                        .desired_width(250.0),
                );

                if response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                    self.launch();
                }

                ui.add_space(10.0);

                if ui
                    .add_sized([250.0, 40.0], egui::Button::new("Запустить"))
                    .clicked()
                {
                    self.launch();
                }

                ui.add_space(15.0);

                ui.label(&self.status);

                ui.horizontal(|ui| {
                    if ui.button("Core: Ping").clicked() {
                        match self.send_core_command("ping", None) {
                            Ok(v) => self.status = format!("Core: {}", v),
                            Err(e) => self.status = format!("Core error: {}", e),
                        }
                    }

                    if ui.button("Core: Status").clicked() {
                        match self.send_core_command("status", None) {
                            Ok(v) => self.status = format!("Core: {}", v),
                            Err(e) => self.status = format!("Core error: {}", e),
                        }
                    }
                });

                ui.add_space(30.0);

                ui.small("Custom Minecraft 1.16.5");

                ui.small("Offline / no authentication");
            });
        });
    }
}

// ------------------------------------------------------------
// Main
// ------------------------------------------------------------

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([400.0, 300.0])
            .with_min_inner_size([350.0, 250.0]),

        ..Default::default()
    };

    // Если подключение успешно — запускаем GUI
    eframe::run_native(
        "Minecraft 1.16.5 Launcher",
        options,
        Box::new(|_cc| {
            let mut launcher = Launcher::default();

            // Проверяем подключение
            if let Err(e) = launcher.connect_core() {
                eprintln!("Не удалось подключиться к core: {}", e);
                return Err("Core недоступен".into());
            }

            Ok(Box::new(launcher))
        }),
    )
}
