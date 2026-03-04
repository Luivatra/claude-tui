use anyhow::Result;
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Debug, Clone, Default)]
pub struct UsageData {
    pub five_hour_percent: Option<u8>,
    pub five_hour_resets_at: Option<String>,
    pub seven_day_percent: Option<u8>,
    pub seven_day_resets_at: Option<String>,
}

impl UsageData {
    /// Format reset time as relative string (e.g., "2h", "Thu 3pm")
    pub fn format_reset(&self, is_five_hour: bool) -> String {
        let reset_str = if is_five_hour {
            &self.five_hour_resets_at
        } else {
            &self.seven_day_resets_at
        };

        match reset_str {
            Some(s) => format_relative_time(s),
            None => "--".to_string(),
        }
    }
}

fn format_relative_time(iso_str: &str) -> String {
    // Parse ISO 8601 timestamp: "2026-03-04T15:00:00.389857+00:00"
    // Extract date and time parts
    let parts: Vec<&str> = iso_str.split('T').collect();
    if parts.len() != 2 {
        return "--".to_string();
    }

    let date_parts: Vec<&str> = parts[0].split('-').collect();
    let time_str = parts[1].split('.').next().unwrap_or("00:00:00");
    let time_parts: Vec<&str> = time_str.split(':').collect();

    if date_parts.len() != 3 || time_parts.len() < 2 {
        return "--".to_string();
    }

    let year: i32 = date_parts[0].parse().unwrap_or(0);
    let month: u32 = date_parts[1].parse().unwrap_or(0);
    let day: u32 = date_parts[2].parse().unwrap_or(0);
    let hour: u32 = time_parts[0].parse().unwrap_or(0);

    // Get current time (UTC)
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Convert reset time to unix timestamp (approximate)
    let days_since_epoch = days_from_date(year, month, day);
    let reset_ts = (days_since_epoch as u64) * 86400 + (hour as u64) * 3600;

    if reset_ts <= now {
        return "soon".to_string();
    }

    let diff_secs = reset_ts - now;
    let diff_hours = diff_secs / 3600;
    let diff_days = diff_hours / 24;

    // Format hour as 12h with am/pm
    let (h12, ampm) = if hour == 0 {
        (12, "am")
    } else if hour < 12 {
        (hour, "am")
    } else if hour == 12 {
        (12, "pm")
    } else {
        (hour - 12, "pm")
    };

    if diff_hours < 24 {
        format!("{}h ({}{})", diff_hours, h12, ampm)
    } else if diff_days < 7 {
        // Show day of week + hour
        let dow = day_of_week(year, month, day);
        let day_name = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"][dow as usize];
        format!("{} {}{}", day_name, h12, ampm)
    } else {
        format!("{}d", diff_days)
    }
}

fn days_from_date(year: i32, month: u32, day: u32) -> i64 {
    // Simplified days since epoch calculation
    let y = year as i64;
    let m = month as i64;
    let d = day as i64;

    // Days from year
    let mut days = (y - 1970) * 365 + (y - 1969) / 4 - (y - 1901) / 100 + (y - 1601) / 400;

    // Days from month (approximate)
    let month_days = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    if (1..=12).contains(&m) {
        days += month_days[(m - 1) as usize];
    }

    // Leap year adjustment
    if m > 2 && (y % 4 == 0 && (y % 100 != 0 || y % 400 == 0)) {
        days += 1;
    }

    days + d - 1
}

fn day_of_week(year: i32, month: u32, day: u32) -> u32 {
    // Zeller's formula simplified
    let days = days_from_date(year, month, day);
    // Jan 1, 1970 was Thursday (4)
    ((days % 7 + 4) % 7) as u32
}

#[derive(Deserialize)]
struct Credentials {
    #[serde(rename = "claudeAiOauth")]
    claude_ai_oauth: Option<OAuthData>,
}

#[derive(Deserialize)]
struct OAuthData {
    #[serde(rename = "accessToken")]
    access_token: String,
}

#[derive(Deserialize)]
struct UsageResponse {
    five_hour: Option<UsageWindow>,
    seven_day: Option<UsageWindow>,
}

#[derive(Deserialize)]
struct UsageWindow {
    utilization: Option<f64>,
    resets_at: Option<String>,
}

fn get_credentials_path(config_dir: Option<&PathBuf>) -> Option<PathBuf> {
    // Use provided config dir first
    if let Some(dir) = config_dir {
        let path = dir.join(".credentials.json");
        if path.exists() {
            return Some(path);
        }
    }

    // Check CLAUDE_CONFIG_DIR env var
    if let Ok(config_dir) = std::env::var("CLAUDE_CONFIG_DIR") {
        let path = PathBuf::from(config_dir).join(".credentials.json");
        if path.exists() {
            return Some(path);
        }
    }

    // Fallback to ~/.claude
    let home = dirs::home_dir()?;
    let path = home.join(".claude").join(".credentials.json");
    if path.exists() {
        return Some(path);
    }

    None
}

fn fetch_usage_inner(config_dir: Option<&PathBuf>) -> Result<UsageData> {
    let creds_path = get_credentials_path(config_dir)
        .ok_or_else(|| anyhow::anyhow!("No credentials file found"))?;
    let creds_content = fs::read_to_string(&creds_path)?;
    let creds: Credentials = serde_json::from_str(&creds_content)?;

    let token = creds
        .claude_ai_oauth
        .ok_or_else(|| anyhow::anyhow!("No OAuth token"))?
        .access_token;

    let client = reqwest::blocking::Client::new();
    let resp = client
        .get("https://api.anthropic.com/api/oauth/usage")
        .header("Authorization", format!("Bearer {}", token))
        .header("anthropic-beta", "oauth-2025-04-20")
        .header("Content-Type", "application/json")
        .timeout(Duration::from_secs(5))
        .send()?;

    let usage: UsageResponse = resp.json()?;

    Ok(UsageData {
        five_hour_percent: usage
            .five_hour
            .as_ref()
            .and_then(|w| w.utilization)
            .map(|u| (u as u8).min(100)),
        five_hour_resets_at: usage.five_hour.as_ref().and_then(|w| w.resets_at.clone()),
        seven_day_percent: usage
            .seven_day
            .as_ref()
            .and_then(|w| w.utilization)
            .map(|u| (u as u8).min(100)),
        seven_day_resets_at: usage.seven_day.as_ref().and_then(|w| w.resets_at.clone()),
    })
}

pub struct UsageFetcher {
    data: Arc<Mutex<UsageData>>,
}

impl UsageFetcher {
    pub fn new(refresh_interval: Duration, config_dir: Option<PathBuf>) -> Self {
        let data = Arc::new(Mutex::new(UsageData::default()));
        let data_clone = Arc::clone(&data);

        // Spawn background thread to fetch usage periodically
        std::thread::spawn(move || loop {
            if let Ok(usage) = fetch_usage_inner(config_dir.as_ref()) {
                if let Ok(mut d) = data_clone.lock() {
                    *d = usage;
                }
            }
            std::thread::sleep(refresh_interval);
        });

        Self { data }
    }

    pub fn get(&self) -> UsageData {
        self.data.lock().map(|d| d.clone()).unwrap_or_default()
    }
}
