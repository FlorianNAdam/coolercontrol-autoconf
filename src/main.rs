use anyhow::{anyhow, bail, Context, Result};
use clap::Parser;
use reqwest::{Client, Url};
use serde::{Deserialize, Deserializer};
use serde_json::{json, Value};
use std::{fs, path::PathBuf, process::Command, time::Duration};

const THEME_MODE_CUSTOM: &str = "custom theme";
const DEFAULT_CURRENT_PASSWORD: &str = "coolAdmin";

#[derive(Debug, Parser)]
#[command(
    author,
    version,
    about = "Set CoolerControl's custom UI theme through the daemon session API"
)]
struct Args {
    /// CoolerControl daemon base URL.
    #[arg(
        long,
        env = "COOLERCONTROL_URL",
        default_value = "http://localhost:11987"
    )]
    url: Url,

    /// CoolerControl admin password.
    #[arg(long, env = "COOLERCONTROL_PASSWORD", conflicts_with = "password_file")]
    password: Option<String>,

    /// File containing the CoolerControl admin password.
    #[arg(long, env = "COOLERCONTROL_PASSWORD_FILE", conflicts_with = "password")]
    password_file: Option<PathBuf>,

    /// Set the CoolerControl admin password from --password/--password-file if needed.
    #[arg(long)]
    set_password: bool,

    /// Path to the coolercontrold executable used by --set-password.
    #[arg(long, default_value = "coolercontrold")]
    coolercontrold: PathBuf,

    /// Wait until the CoolerControl daemon accepts HTTP requests before applying settings.
    #[arg(long)]
    wait: bool,

    /// Interval between daemon readiness checks when --wait is set.
    #[arg(long, default_value = "1s", value_parser = parse_wait_interval)]
    wait_interval: Duration,

    /// JSON file containing CoolerControl UI settings.
    theme_file: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    require_root()?;

    let args = Args::parse();
    let client = Client::builder()
        .cookie_store(true)
        .build()
        .context("failed to build HTTP client")?;
    let password = read_password(&args)?;

    if args.wait {
        wait_for_server(&client, &args.url, args.wait_interval).await?;
    }

    if args.set_password {
        set_password(&client, &args.url, &password, &args.coolercontrold).await?;
    }

    login(&client, &args.url, &password).await?;

    let settings_file = load_settings_file(&args.theme_file)?;
    let mut settings = load_ui_settings(&client, &args.url).await?;
    merge_settings(&mut settings, settings_file)?;
    save_ui_settings(&client, &args.url, &settings).await?;

    println!(
        "Set CoolerControl UI settings from {}",
        args.theme_file.display()
    );
    Ok(())
}

fn require_root() -> Result<()> {
    let effective_uid = unsafe { libc::geteuid() };
    if effective_uid != 0 {
        bail!("coolercontrol-autoconf must be run as root; use sudo");
    }
    Ok(())
}

fn read_password(args: &Args) -> Result<String> {
    read_password_value(
        args.password.as_deref(),
        args.password_file.as_ref(),
        "CoolerControl admin password",
    )
}

fn parse_wait_interval(value: &str) -> std::result::Result<Duration, String> {
    let duration = humantime::parse_duration(value).map_err(|error| error.to_string())?;
    if duration.is_zero() {
        return Err("wait interval must be greater than 0".to_string());
    }

    Ok(duration)
}

fn read_password_value(
    inline: Option<&str>,
    file: Option<&PathBuf>,
    description: &str,
) -> Result<String> {
    let password = match (inline, file) {
        (Some(password), None) => password.to_string(),
        (None, Some(path)) => fs::read_to_string(path)
            .with_context(|| format!("failed to read password file {}", path.display()))?,
        (None, None) => bail!("provide {description}"),
        (Some(_), Some(_)) => bail!("provide {description} only once"),
    };

    normalize_password(&password, description)
}

fn normalize_password(password: &str, description: &str) -> Result<String> {
    let password = password.trim_end_matches(['\r', '\n']).to_string();
    if password.is_empty() {
        bail!("{description} is empty");
    }
    Ok(password)
}

#[derive(Debug, Clone)]
struct Color(String);

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let color = String::deserialize(deserializer)?;
        normalize_color(&color)
            .map(Self)
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Theme {
    accent: Color,
    bg_one: Color,
    bg_two: Color,
    border_one: Color,
    text_color: Color,
    text_color_secondary: Color,
}

impl Theme {
    fn into_value(self) -> Value {
        json!({
            "accent": self.accent.0,
            "bgOne": self.bg_one.0,
            "bgTwo": self.bg_two.0,
            "borderOne": self.border_one.0,
            "textColor": self.text_color.0,
            "textColorSecondary": self.text_color_secondary.0,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum BuiltInTheme {
    System,
    Light,
    Dark,
    #[serde(rename = "high-contrast-dark")]
    HighContrastDark,
    #[serde(rename = "high-contrast-light")]
    HighContrastLight,
}

impl BuiltInTheme {
    fn as_str(&self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
            Self::HighContrastDark => "high-contrast-dark",
            Self::HighContrastLight => "high-contrast-light",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ThemeSetting {
    BuiltIn(BuiltInTheme),
    Custom(Theme),
}

impl ThemeSetting {
    fn merge_into(self, settings: &mut Value) {
        match self {
            Self::BuiltIn(theme) => {
                settings["themeMode"] = json!(theme.as_str());
            }
            Self::Custom(theme) => {
                settings["themeMode"] = json!(THEME_MODE_CUSTOM);
                settings["customTheme"] = theme.into_value();
            }
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SettingsFile {
    theme: ThemeSetting,
    eye_candy: Option<bool>,
    show_onboarding: Option<bool>,
    collapsed_main_menu: Option<bool>,
    hide_menu_collapse_icon: Option<bool>,
    main_menu_width_rem: Option<serde_json::Number>,
    frequency_precision: Option<serde_json::Number>,
    chart_line_scale: Option<serde_json::Number>,
    #[serde(rename = "time24")]
    time_24: Option<bool>,
}

impl SettingsFile {
    fn merge_into(self, settings: &mut Value) {
        self.theme.merge_into(settings);

        if let Some(value) = self.eye_candy {
            settings["eyeCandy"] = json!(value);
        }
        if let Some(value) = self.show_onboarding {
            settings["showOnboarding"] = json!(value);
        }
        if let Some(value) = self.collapsed_main_menu {
            settings["collapsedMainMenu"] = json!(value);
        }
        if let Some(value) = self.hide_menu_collapse_icon {
            settings["hideMenuCollapseIcon"] = json!(value);
        }
        if let Some(value) = self.main_menu_width_rem {
            settings["mainMenuWidthRem"] = Value::Number(value);
        }
        if let Some(value) = self.frequency_precision {
            settings["frequencyPrecision"] = Value::Number(value);
        }
        if let Some(value) = self.chart_line_scale {
            settings["chartLineScale"] = Value::Number(value);
        }
        if let Some(value) = self.time_24 {
            settings["time24"] = json!(value);
        }
    }
}

fn load_settings_file(path: &PathBuf) -> Result<SettingsFile> {
    let body = fs::read_to_string(path)
        .with_context(|| format!("failed to read settings file {}", path.display()))?;
    serde_json::from_str(&body)
        .with_context(|| format!("settings file has invalid settings: {}", path.display()))
}

fn merge_settings(settings: &mut Value, settings_file: SettingsFile) -> Result<()> {
    settings_file.merge_into(settings);
    Ok(())
}

fn normalize_color(color: &str) -> Result<String> {
    let color = color.trim();
    if let Some(hex) = color.strip_prefix('#') {
        return hex_to_rgb_theme_string(hex);
    }
    if color.chars().all(|c| c.is_ascii_hexdigit()) && (color.len() == 3 || color.len() == 6) {
        return hex_to_rgb_theme_string(color);
    }

    validate_rgb_theme_string(color)?;
    Ok(color.to_string())
}

fn hex_to_rgb_theme_string(hex: &str) -> Result<String> {
    let expanded;
    let hex = match hex.len() {
        3 => {
            expanded = hex.chars().flat_map(|c| [c, c]).collect::<String>();
            expanded.as_str()
        }
        6 => hex,
        _ => bail!("hex colors must be #RGB or #RRGGBB"),
    };

    let red = u8::from_str_radix(&hex[0..2], 16).context("invalid red hex component")?;
    let green = u8::from_str_radix(&hex[2..4], 16).context("invalid green hex component")?;
    let blue = u8::from_str_radix(&hex[4..6], 16).context("invalid blue hex component")?;
    Ok(format!("{red} {green} {blue}"))
}

fn validate_rgb_theme_string(color: &str) -> Result<()> {
    let parts = color.split_whitespace().collect::<Vec<_>>();
    if !(parts.len() == 3 || parts.len() == 4) {
        bail!("colors must be hex or `R G B` strings");
    }

    for component in &parts[0..3] {
        component
            .parse::<u8>()
            .with_context(|| format!("invalid RGB component `{component}`"))?;
    }
    if let Some(alpha) = parts.get(3) {
        alpha
            .parse::<f32>()
            .with_context(|| format!("invalid alpha component `{alpha}`"))?;
    }

    Ok(())
}

async fn login(client: &Client, base_url: &Url, password: &str) -> Result<()> {
    let url = base_url.join("login").context("invalid login URL")?;
    let response = client
        .post(url)
        .basic_auth("CCAdmin", Some(password))
        .send()
        .await
        .context("login request failed")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        bail!("login failed with {status}: {body}");
    }

    Ok(())
}

async fn wait_for_server(client: &Client, base_url: &Url, interval: Duration) -> Result<()> {
    loop {
        if handshake(client, base_url).await.is_ok() {
            return Ok(());
        }

        eprintln!(
            "CoolerControl daemon is not ready at {base_url}; retrying in {}",
            humantime::format_duration(interval)
        );
        tokio::time::sleep(interval).await;
    }
}

async fn handshake(client: &Client, base_url: &Url) -> Result<()> {
    let url = base_url
        .join("handshake")
        .context("invalid handshake URL")?;
    let response = client
        .get(url)
        .send()
        .await
        .context("handshake request failed")?;

    if !response.status().is_success() {
        let status = response.status();
        bail!("handshake failed with {status}");
    }

    let body = response
        .json::<Value>()
        .await
        .context("daemon returned invalid handshake JSON")?;
    if body.get("shake").and_then(Value::as_bool) != Some(true) {
        bail!("daemon returned invalid handshake response");
    }

    Ok(())
}

async fn set_password(
    client: &Client,
    base_url: &Url,
    password: &str,
    coolercontrold: &PathBuf,
) -> Result<()> {
    if login(client, base_url, password).await.is_ok() {
        return Ok(());
    }

    reset_password_to_default(coolercontrold)?;
    login(client, base_url, DEFAULT_CURRENT_PASSWORD)
        .await
        .context("failed to create session with reset default password")?;

    let url = base_url
        .join("set-passwd")
        .context("invalid set-passwd URL")?;
    let response = client
        .post(url)
        .basic_auth("CCAdmin", Some(password))
        .json(&json!({ "current_password": DEFAULT_CURRENT_PASSWORD }))
        .send()
        .await
        .context("set-passwd request failed")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        bail!("setting CoolerControl password failed with {status}: {body}");
    }

    Ok(())
}

fn reset_password_to_default(coolercontrold: &PathBuf) -> Result<()> {
    let output = Command::new(coolercontrold)
        .arg("--reset-password")
        .output()
        .with_context(|| {
            format!(
                "failed to run `{}` --reset-password",
                coolercontrold.display()
            )
        })?;

    if !output.status.success() {
        bail!(
            "`{} --reset-password` failed with status {}: {}{}",
            coolercontrold.display(),
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(())
}

async fn load_ui_settings(client: &Client, base_url: &Url) -> Result<Value> {
    let url = base_url
        .join("settings/ui")
        .context("invalid UI settings URL")?;
    let response = client
        .get(url)
        .send()
        .await
        .context("UI settings GET failed")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        bail!("loading UI settings failed with {status}: {body}");
    }

    let body = response
        .text()
        .await
        .context("failed to read UI settings body")?;
    if body.trim().is_empty() {
        return Ok(json!({}));
    }

    serde_json::from_str(&body).with_context(|| "daemon returned invalid UI settings JSON")
}

async fn save_ui_settings(client: &Client, base_url: &Url, settings: &Value) -> Result<()> {
    if !settings.is_object() {
        return Err(anyhow!("UI settings response was not a JSON object"));
    }

    let url = base_url
        .join("settings/ui")
        .context("invalid UI settings URL")?;
    let response = client
        .put(url)
        .json(settings)
        .send()
        .await
        .context("UI settings PUT failed")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        bail!("saving UI settings failed with {status}: {body}");
    }

    Ok(())
}
