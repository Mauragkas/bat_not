#![allow(unused)]
use std::sync::mpsc;
use std::thread;
use tokio::process::Command;
use tokio::time::{sleep, Duration};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};

const ICON_PATH: &str = "/usr/share/icons/Papirus/48x48/status/";
const COMMAND: &str = "upower -i /org/freedesktop/UPower/devices/battery_BAT1 | grep -E \"(percentage:|state:)\"";

#[derive(Debug, PartialEq, Copy, Clone)]
enum State {
    FullyCharged,
    Chaging,
    Discharging,
}

#[derive(Debug, PartialEq, Copy, Clone)]
enum ChargeState {
    High,
    Medium,
    Low,
    Critical,
}

#[derive(PartialEq)]
enum NotificationState {
    A,
    B,
    C
}

#[derive(Debug)]
struct BAT {
    state: State,
    charge: i32,
}

impl BAT {
    fn new(state: State, charge: i32) -> BAT {
        BAT {
            state,
            charge,
        }
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

    fn copy_bat(&self) -> BAT {
        BAT {
            state: self.state,
            charge: self.charge,
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

    let output_str = String::from_utf8_lossy(&output.stdout);
    let mut state = State::Discharging;
    let mut percentage = 0;

    for lines in output_str.lines() {
        if lines.contains("percentage") {
            let line = lines.split(":").collect::<Vec<&str>>();
            let line = line[1].trim();
            let line = line.split("%").collect::<Vec<&str>>();
            percentage = line[0].parse::<i32>().unwrap();
        }
        if lines.contains("state") {
            let line = lines.split(":").collect::<Vec<&str>>();
            let line = line[1].trim();
            if line == "charging" {
                state = State::Chaging;
            } else if line == "fully-charged" {
                state = State::FullyCharged;
            }
        }
    }

    Ok((state, percentage))
}

fn run_command(urgency: &str, icon: &str, message: &str) {
    let command = format!(
        "notify-send -u {} -i {} -a \"Battery Notification\" '{}'",
        urgency, icon, message
    );
    std::process::Command::new("sh")
        .arg("-c")
        .arg(command)
        .output()
        .expect("failed to execute process");
}

async fn notify_user(bat: &BAT) {

    let charge_state = BAT::get_battery_charge_state(bat.charge);

    let urgency = match bat.state {
        State::Chaging => "normal",
        State::Discharging => match charge_state {
            ChargeState::Critical => "critical",
            ChargeState::Low => "normal",
            ChargeState::Medium => "normal",
            ChargeState::High => "normal",
        },
        State::FullyCharged => "normal",
    };

    let message = match (bat.state, charge_state) {
        (State::Chaging, _) => format!("Battery is charging: {}%", bat.charge),
        (State::Discharging, ChargeState::Critical) => format!("Critical Battery Alert: {}%", bat.charge),
        (State::Discharging, ChargeState::Low) => format!("Low Battery Alert: {}%", bat.charge),
        (State::FullyCharged, _) => "Battery is fully charged".to_string(),
        _ => "".to_string(),
    };

    let icon = match (bat.state, charge_state) {
        (State::Chaging, ChargeState::High) => format!("{}battery-full-charging.svg", ICON_PATH),
        (State::Chaging, ChargeState::Medium) => format!("{}battery-good-charging.svg", ICON_PATH),
        (State::Chaging, ChargeState::Low) => format!("{}battery-low-charging.svg", ICON_PATH),
        (State::Chaging, ChargeState::Critical) => format!("{}battery-low-charging.svg", ICON_PATH),
        (State::Discharging, ChargeState::Low) => format!("{}battery-caution.svg", ICON_PATH),
        (State::Discharging, ChargeState::Critical) => format!("{}battery-empty.svg", ICON_PATH),
        (State::FullyCharged, _) => format!("{}battery-full.svg", ICON_PATH),
        _ => "".to_string(),
    };

    if !message.is_empty() || !icon.is_empty() {
        run_command(urgency, &icon, &message);
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut bat = BAT::new(State::Discharging, 0);
    let mut notification_state = NotificationState::A;

    loop {
        match fetch_battery_info().await {
            Ok((state, charge)) => {
                if state != bat.state || charge != bat.charge {
                    bat.update(state, charge);
                    // println!("Battery info updated: {:?}", bat);
                }

                if bat.state == State::Discharging {
                    if BAT::get_battery_charge_state(bat.charge) == ChargeState::Low {
                        if notification_state != NotificationState::A {
                            notification_state = NotificationState::A;
                            notify_user(&bat).await;
                        }
                    } else if BAT::get_battery_charge_state(bat.charge) == ChargeState::Critical {
                        if notification_state != NotificationState::B {
                            notification_state = NotificationState::B;
                            notify_user(&bat).await;
                        }
                    }
                } else {
                    if notification_state != NotificationState::C {
                        notification_state = NotificationState::C;
                        notify_user(&bat).await;
                    }
                }
            }
            Err(e) => eprintln!("Failed to fetch battery info: {}", e),
        }

        // Wait for 10 seconds before fetching again
        sleep(Duration::from_millis(500)).await;
    }

    Ok(())
}