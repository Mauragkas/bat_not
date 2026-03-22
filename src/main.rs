#![allow(unused)]
use log::{error, info, warn, LevelFilter};
use simplelog::*;
use std::fs::File;
use std::process::Stdio;
use tokio::process::Command;
use tokio::time::{sleep, Duration};

const ICON_PATH: &str = "/usr/share/icons/Papirus/48x48/status/";
// Adjusted grep slightly to ensure we catch lines correctly
const COMMAND: &str =
    "upower -i /org/freedesktop/UPower/devices/battery_BAT1 | grep -E \"(percentage:|state:)\"";
const LOG_FILE: &str = "bat_not.log";

#[derive(Debug, PartialEq, Copy, Clone)]
enum State {
    FullyCharged,
    Charging, // Fixed typo: Chaging -> Charging
    Discharging,
    Unknown, // Added Unknown state for safety
}

#[derive(Debug, PartialEq, Copy, Clone)]
enum ChargeState {
    High,
    Medium,
    Low,
    Critical,
}

#[derive(PartialEq, Debug)]
enum NotificationState {
    Init, // Added Init to prevent notifications on startup immediately if not needed
    LowAlert,
    CriticalAlert,
    Normal,
}

#[derive(Debug)]
struct BAT {
    state: State,
    charge: i32,
}

impl BAT {
    fn new(state: State, charge: i32) -> BAT {
        BAT { state, charge }
    }

    fn get_battery_charge_state(charge: i32) -> ChargeState {
        match charge {
            0..=15 => ChargeState::Critical,
            16..=30 => ChargeState::Low,
            31..=75 => ChargeState::Medium,
            76..=100 => ChargeState::High,
            _ => ChargeState::High,
        }
    }

    fn update(&mut self, state: State, charge: i32) {
        self.state = state;
        self.charge = charge;
    }
}

async fn fetch_battery_info() -> Result<(State, i32), Box<dyn std::error::Error>> {
    let output = Command::new("sh")
        .arg("-c")
        .arg(COMMAND)
        .stdout(Stdio::piped())
        .spawn()?
        .wait_with_output()
        .await?;

    if !output.status.success() {
        return Err(format!("Command execution failed with status: {}", output.status).into());
    }

    let output_str = String::from_utf8_lossy(&output.stdout);

    // Default to Unknown/0 so we can detect if parsing failed
    let mut state = State::Unknown;
    let mut percentage = -1;

    for line in output_str.lines() {
        if line.contains("percentage") {
            // Safer parsing
            if let Some(val_part) = line.split(':').nth(1) {
                let clean_val = val_part.trim().trim_end_matches('%');
                if let Ok(parsed) = clean_val.parse::<i32>() {
                    percentage = parsed;
                } else {
                    warn!("Failed to parse percentage from line: {}", line);
                }
            }
        }
        if line.contains("state") {
            if let Some(val_part) = line.split(':').nth(1) {
                match val_part.trim() {
                    "charging" => state = State::Charging,
                    "fully-charged" => state = State::FullyCharged,
                    "discharging" => state = State::Discharging,
                    other => {
                        // Don't crash on unknown states (e.g., "pending-charge")
                        warn!("Unknown battery state received: {}", other);
                        state = State::Discharging; // Default or keep Unknown
                    }
                }
            }
        }
    }

    if percentage == -1 || state == State::Unknown {
        return Err("Failed to parse battery info correctly".into());
    }

    Ok((state, percentage))
}

fn run_command(urgency: &str, icon: &str, message: &str) {
    let command = format!(
        "notify-send -u {} -i {} -a \"Battery Notification\" '{}'",
        urgency, icon, message
    );

    info!("Sending Notification: {}", message);

    let result = std::process::Command::new("sh")
        .arg("-c")
        .arg(command)
        .output();

    match result {
        Ok(_) => {}
        Err(e) => error!("Failed to execute notify-send: {}", e),
    }
}

async fn notify_user(bat: &BAT) {
    let charge_state = BAT::get_battery_charge_state(bat.charge);

    let urgency = match bat.state {
        State::Charging => "normal",
        State::Discharging => match charge_state {
            ChargeState::Critical => "critical",
            ChargeState::Low => "normal",
            ChargeState::Medium => "normal",
            ChargeState::High => "normal",
        },
        State::FullyCharged => "normal",
        State::Unknown => "normal",
    };

    let message = match (bat.state, charge_state) {
        (State::Charging, _) => format!("Battery is charging: {}%", bat.charge),
        (State::Discharging, ChargeState::Critical) => {
            format!("Critical Battery Alert: {}%", bat.charge)
        }
        (State::Discharging, ChargeState::Low) => format!("Low Battery Alert: {}%", bat.charge),
        (State::FullyCharged, _) => "Battery is fully charged".to_string(),
        _ => "".to_string(),
    };

    let icon = match (bat.state, charge_state) {
        (State::Charging, ChargeState::High) => format!("{}battery-full-charging.svg", ICON_PATH),
        (State::Charging, ChargeState::Medium) => format!("{}battery-good-charging.svg", ICON_PATH),
        (State::Charging, ChargeState::Low) => format!("{}battery-low-charging.svg", ICON_PATH),
        (State::Charging, ChargeState::Critical) => {
            format!("{}battery-low-charging.svg", ICON_PATH)
        }
        (State::Discharging, ChargeState::Low) => format!("{}battery-caution.svg", ICON_PATH),
        (State::Discharging, ChargeState::Critical) => format!("{}battery-empty.svg", ICON_PATH),
        (State::FullyCharged, _) => format!("{}battery-full.svg", ICON_PATH),
        _ => "".to_string(),
    };

    if !message.is_empty() && !icon.is_empty() {
        run_command(urgency, &icon, &message);
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    CombinedLogger::init(vec![
        TermLogger::new(
            LevelFilter::Info,
            Config::default(),
            TerminalMode::Mixed,
            ColorChoice::Auto,
        ),
        WriteLogger::new(
            LevelFilter::Info,
            Config::default(),
            File::create(LOG_FILE).unwrap(),
        ),
    ])
    .unwrap();

    info!("Starting Battery Notification Daemon...");

    let mut bat = BAT::new(State::Unknown, -1);
    let mut notification_state = NotificationState::Init;

    loop {
        match fetch_battery_info().await {
            Ok((state, charge)) => {
                let charge_state = BAT::get_battery_charge_state(charge);

                if state != bat.state || (charge - bat.charge).abs() >= 5 {
                    info!("Status update: {:?} - {}%", state, charge);
                }

                // Update internal tracking
                bat.update(state, charge);

                // Logic Machine
                match bat.state {
                    State::Discharging => {
                        match charge_state {
                            ChargeState::Critical => {
                                if notification_state != NotificationState::CriticalAlert {
                                    notification_state = NotificationState::CriticalAlert;
                                    notify_user(&bat).await;
                                }
                            }
                            ChargeState::Low => {
                                if notification_state != NotificationState::LowAlert
                                    && notification_state != NotificationState::CriticalAlert
                                {
                                    // We prevent going from Critical -> Low (upwards) to avoid spam if it fluctuates at the border
                                    // unless we want reminders. Here I assume we only notify going down.
                                    notification_state = NotificationState::LowAlert;
                                    notify_user(&bat).await;
                                }
                            }
                            _ => {
                                // Reset state if we go back to Medium/High
                                notification_state = NotificationState::Normal;
                            }
                        }
                    }
                    State::Charging | State::FullyCharged => {
                        if notification_state != NotificationState::Normal {
                            notification_state = NotificationState::Normal;
                            // Optionally notify when plugged in
                            notify_user(&bat).await;
                        }
                    }
                    _ => {}
                }
            }
            Err(e) => {
                error!("Failed to fetch battery info: {}", e);
            }
        }

        // Wait for 2 seconds (500ms is very fast for polling a battery)
        sleep(Duration::from_secs(2)).await;
    }
}
