use std::thread::sleep;
use std::time::Duration;

const ICON_PATH: &str = "/usr/share/icons/Papirus/48x48/status/";

#[derive(Debug, PartialEq)]
enum State {
    CHARGING,
    DISCHARGING,
}

#[derive(Debug, PartialEq)]
enum ChargeState {
    HIGH,
    MEDIUM,
    LOW,
    CRITICAL,
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

    fn get_battery_charge_state(&self) -> ChargeState {
        match self.charge {
            0..=15 => ChargeState::CRITICAL,
            16..=30 => ChargeState::LOW,
            31..=75 => ChargeState::MEDIUM,
            76..=100 => ChargeState::HIGH,
            _ => ChargeState::HIGH,
        }
    }
}

fn get_battery_status(bat: &mut BAT) {
    let command = "upower -i /org/freedesktop/UPower/devices/battery_BAT1"
        .to_string();
    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg(command)
        .output()
        .expect("failed to execute process");
    let output = String::from_utf8_lossy(&output.stdout);
    let output = output.to_string();
    let output = output.split("\n");
    let mut charge = 0;
    let mut state = State::DISCHARGING;
    for line in output {
        if line.contains("percentage") {
            let line = line.split(":").collect::<Vec<&str>>();
            let line = line[1].trim();
            let line = line.split("%").collect::<Vec<&str>>();
            charge = line[0].parse::<i32>().unwrap();
        }
        if line.contains("state") {
            let line = line.split(":").collect::<Vec<&str>>();
            let line = line[1].trim();
            if line == "charging" {
                state = State::CHARGING;
            }
        }
    }
    bat.charge = charge;
    bat.state = state;
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

fn notify_user(bat: &mut BAT, charge_state: ChargeState) {
    match bat.state {
        State::DISCHARGING => {
            let urgency = "critical";
            match charge_state {
                ChargeState::LOW => {
                    // println!("Battery is at {}% charge", bat.charge);
                    let message = format!("Low Battery Alert: {}%", bat.charge);
                    let icon = format!("{}battery-caution.svg", ICON_PATH);
                    run_command(urgency, &icon, &message);
                }
                ChargeState::CRITICAL => {
                    // println!("Battery is at {}% charge", bat.charge);
                    let message = format!("Critical Battery Alert: {}%", bat.charge);
                    let icon = format!("{}battery-empty.svg", ICON_PATH);
                    run_command(urgency, &icon, &message);
                }
                _ => {}
            }
        }
        State::CHARGING => {
            let urgency = "normal";
            let message = format!("Battery is charging: {}%", bat.charge);
            match charge_state {
                ChargeState::HIGH => {
                    // println!("Battery is at {}% charge", bat.charge);
                    let icon = format!("{}battery-full-charging.svg", ICON_PATH);
                    run_command(urgency, &icon, &message);
                }
                ChargeState::MEDIUM => {
                    // println!("Battery is at {}% charge", bat.charge);
                    let icon = format!("{}battery-good-charging.svg", ICON_PATH);
                    run_command(urgency, &icon, &message);
                }
                _ => {
                    let icon = format!("{}battery-low-charging.svg", ICON_PATH);
                    run_command(urgency, &icon, &message);
                }
            }
        }
    }
}

fn main() {
    let mut bat = BAT::new(State::DISCHARGING, 0);

    let mut is_charging = 0;
    let mut is_low = 0;
    let mut is_crit = 0;

    loop {
        get_battery_status(&mut bat);
        // bat.state = State::DISCHARGING;

        if bat.state == State::DISCHARGING {
            if is_charging == 1 {

                is_charging = 0;
            }
            let charge_state = bat.get_battery_charge_state();
            match charge_state {
                ChargeState::LOW => {
                    if is_low == 0 {
                        // println!("Battery is at {}% charge", bat.charge);
                        notify_user(&mut bat, charge_state);
                        is_low = 1;
                    }
                }
                ChargeState::CRITICAL => {
                    if is_crit == 0 {
                        // println!("Battery is at {}% charge", bat.charge);
                        notify_user(&mut bat, charge_state);
                        is_crit = 1;
                    }
                }
                _ => {}
            }
        } 

        if bat.state == State::CHARGING && (is_charging == 0 || is_low == 1 || is_crit == 1) {
            let charge_state = bat.get_battery_charge_state();
            notify_user(&mut bat, charge_state);
            is_charging = 1;
            is_low = 0; // Reset flags
            is_crit = 0;
        }

        sleep(Duration::from_millis(500));
    }

}
