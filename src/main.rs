// Copyright (C) [2026/10/19] kotaro,termin
//
// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU General Public License
// as published by the Free Software Foundation; version 2
// of the License.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program; if not, write to the Free Software
// Foundation, Inc., 51 Franklin Street, Fifth Floor, Boston, MA  02110-1301, USA.

pub mod config;
pub mod i18n;
pub mod menus;
pub mod session;
pub mod ui;
pub mod vte;

use std::io::Write;
use std::net::TcpListener;
use std::path::PathBuf;
use std::thread;

use gtk4::gio;
use gtk4::prelude::*;

use crate::config::theme::{ThemeSettings, set_custom_config_path};
use crate::i18n::load_catalog_for_language;
use crate::ui::app::FwerApp;
use crate::ui::scheduler::PollMode;

pub const APP_PROGRAM_NAME: &str = "termin";
pub const APP_VERSION: &str = "0.0.3";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CliOptions {
    pub show_help: bool,
    pub show_version: bool,
    pub maximize: bool,
    pub debug_level: u8,
    pub config_path: Option<PathBuf>,
    pub config_json: Option<String>,
    pub startup_command: Option<String>,
    pub target_fps: Option<u32>,
    pub explicit_mode: Option<PollMode>,
    pub open_prefs: bool,
}

impl CliOptions {
    pub fn resolve_poll_mode(&self) -> PollMode {
        if let Some(mode) = self.explicit_mode {
            mode
        } else if self.target_fps.is_some() {
            PollMode::ContinuousFps
        } else {
            PollMode::EventDriven
        }
    }
}

pub fn shell_quote_arg(arg: &str) -> String {
    if arg.is_empty() {
        return "''".to_string();
    }
    let safe = arg.chars().all(|c| {
        c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | ':' | '=' | '+' | ',')
    });
    if safe {
        arg.to_string()
    } else {
        format!("'{}'", arg.replace('\'', "'\\''"))
    }
}

pub fn join_command_args(args: &[String]) -> String {
    if args.len() == 1 {
        args[0].clone()
    } else {
        args.iter()
            .map(|a| shell_quote_arg(a))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

pub fn parse_cli_args<I, S>(args: I) -> CliOptions
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let raw: Vec<String> = args.into_iter().map(Into::into).collect();
    let mut opts = CliOptions::default();
    let mut i = 0;

    while i < raw.len() {
        let arg = &raw[i];
        match arg.as_str() {
            "-h" | "--help" => {
                opts.show_help = true;
            }
            "-v" | "--version" => {
                opts.show_version = true;
            }
            "-M" | "--maximize" | "-m" | "--maximise" => {
                opts.maximize = true;
            }
            "--prefs" => {
                opts.open_prefs = true;
            }
            "--event" | "--event-driven" => {
                opts.explicit_mode = Some(PollMode::EventDriven);
            }
            "--poll" | "--continuous" => {
                opts.explicit_mode = Some(PollMode::ContinuousFps);
            }
            "-d" | "--debug" => {
                opts.debug_level = opts.debug_level.saturating_add(1);
            }
            "-g" | "--config" => {
                if i + 1 < raw.len() {
                    i += 1;
                    opts.config_path = Some(PathBuf::from(&raw[i]));
                }
            }
            "-j" | "--config-json" => {
                if i + 1 < raw.len() {
                    i += 1;
                    opts.config_json = Some(raw[i].clone());
                }
            }
            "-f" | "--fps" => {
                if i + 1 < raw.len() {
                    i += 1;
                    if let Ok(fps) = raw[i].parse::<i64>() {
                        opts.target_fps = Some(fps.clamp(1, 240) as u32);
                    }
                }
            }
            "--mode" => {
                if i + 1 < raw.len() {
                    i += 1;
                    if let Some(m) = PollMode::from_str_opt(&raw[i]) {
                        opts.explicit_mode = Some(m);
                    }
                }
            }
            "-e" | "--command" | "-x" | "--execute" => {
                if i + 1 < raw.len() {
                    let rest = &raw[i + 1..];
                    opts.startup_command = Some(join_command_args(rest));
                }
                break;
            }
            other => {
                if other.starts_with("-d") && other.chars().skip(1).all(|c| c == 'd') {
                    let count = (other.len() - 1) as u8;
                    opts.debug_level = opts.debug_level.saturating_add(count);
                } else if let Some(val) = other.strip_prefix("--config=") {
                    opts.config_path = Some(PathBuf::from(val));
                } else if let Some(val) = other.strip_prefix("--config-json=") {
                    opts.config_json = Some(val.to_string());
                } else if let Some(val) = other.strip_prefix("--fps=") {
                    if let Ok(fps) = val.parse::<i64>() {
                        opts.target_fps = Some(fps.clamp(1, 240) as u32);
                    }
                } else if let Some(val) = other.strip_prefix("--mode=") {
                    if let Some(m) = PollMode::from_str_opt(val) {
                        opts.explicit_mode = Some(m);
                    }
                } else if let Some(val) = other
                    .strip_prefix("--command=")
                    .or_else(|| other.strip_prefix("--execute="))
                {
                    opts.startup_command = Some(val.to_string());
                }
            }
        }
        i += 1;
    }

    opts
}

pub fn format_version_string() -> String {
    format!("{} {}", APP_PROGRAM_NAME, APP_VERSION)
}

pub fn format_help_string() -> String {
    format!(
        "{name} {ver} - Lightweight GTK4 tiling & tabbed terminal emulator\n\n\
         USAGE:\n\
         \x20   {name} [OPTIONS]\n\n\
         OPTIONS:\n\
         \x20   -h, --help                   Show this help message and exit\n\
         \x20   -v, --version                Show version information and exit\n\
         \x20   -M, --maximize, -m           Maximize window on startup\n\
         \x20   -d, --debug                  Increase debug level (-dd starts debug server)\n\
         \x20   -g, --config <PATH>          Specify configuration file path\n\
         \x20   -j, --config-json <JSON>     Deep-merge JSON override onto config\n\
         \x20   -e, --command <CMD...>       Execute command in initial pane\n\
         \x20   -x, --execute <CMD...>       Execute command in initial pane\n\
         \x20   -f, --fps <1..240>           Override target FPS\n\
         \x20   --mode <event|poll>          Set repaint mode\n\
         \x20   --event, --event-driven      Use event-driven repaint mode\n\
         \x20   --poll, --continuous         Use continuous FPS polling mode\n\
         \x20   --prefs                      Open Preferences window on startup\n",
        name = APP_PROGRAM_NAME,
        ver = APP_VERSION
    )
}

pub fn start_debug_server_if_needed(opts: &CliOptions) {
    if opts.debug_level < 2 {
        return;
    }
    let bind_addr =
        std::env::var("TERMIN_DEBUG_ADDR").unwrap_or_else(|_| "127.0.0.1:0".to_string());
    if let Ok(listener) = TcpListener::bind(&bind_addr) {
        if let Ok(local_addr) = listener.local_addr() {
            eprintln!("[termin][debug] Debug server listening on {}", local_addr);
        }
        let cli_repr = format!("{:?}", opts);
        let pid = std::process::id();
        let _ = thread::Builder::new()
            .name("termin-debug-server".to_string())
            .spawn(move || {
                for mut stream in listener.incoming().flatten() {
                    let payload = serde_json::json!({
                        "program": APP_PROGRAM_NAME,
                        "version": APP_VERSION,
                        "pid": pid,
                        "cli": cli_repr,
                    });
                    let line = format!("{}\n", payload);
                    let _ = stream.write_all(line.as_bytes());
                    let _ = stream.flush();
                }
            });
    }
}

fn main() {
    let raw_args: Vec<String> = std::env::args().skip(1).collect();
    let cli = parse_cli_args(raw_args);

    if cli.show_help {
        print!("{}", format_help_string());
        return;
    }
    if cli.show_version {
        println!("{}", format_version_string());
        return;
    }

    if let Some(ref custom_cfg) = cli.config_path {
        set_custom_config_path(Some(custom_cfg.clone()));
    }

    let mut settings = ThemeSettings::load(cli.config_json.as_deref());
    if let Some(fps) = cli.target_fps {
        settings.target_fps = fps.clamp(1, 240);
    }
    load_catalog_for_language(&settings.language, &settings.custom_po_path);

    if cli.debug_level >= 1 {
        eprintln!(
            "[termin][debug] Starting with CLI options: {:?}, mode: {:?}",
            cli,
            cli.resolve_poll_mode()
        );
    }
    start_debug_server_if_needed(&cli);

    let app = gtk4::Application::builder()
        .application_id("org.kotaro.fwer")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();

    let poll_mode = cli.resolve_poll_mode();
    let startup_cmd = cli.startup_command.clone();
    let maximize = cli.maximize;
    let open_prefs = cli.open_prefs;

    app.connect_activate(move |gtk_app| {
        let _fwer = FwerApp::build(
            gtk_app,
            settings.clone(),
            startup_cmd.clone(),
            maximize,
            open_prefs,
            poll_mode,
        );
    });

    app.run_with_args(&[] as &[String]);
}

#[cfg(test)]
#[path = "test/main_tests.rs"]
mod main_tests;
