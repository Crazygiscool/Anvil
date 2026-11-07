use crate::discovery::model::{InterfaceKind, NetworkDiscovery, NetworkInterface};
use crate::platform::TextReader;
use std::path::Path;

pub fn collect_network(reader: &dyn TextReader) -> NetworkDiscovery {
    let mut interfaces = Vec::new();
    let root = Path::new("/sys/class/net");

    if let Ok(names) = reader.read_dir_names(root) {
        for name in names {
            let interface_path = root.join(&name);
            let wireless = reader.exists(&interface_path.join("wireless"));
            let device_present = reader.exists(&interface_path.join("device"));
            let driver_present = reader.exists(&interface_path.join("device/driver"));
            let address_present = reader
                .read_to_string(&interface_path.join("address"))
                .map(|value| !zero_address(&value))
                .unwrap_or(false);
            let stats_present = reader.exists(&interface_path.join("statistics"));
            let kind = classify(&name, wireless, device_present, driver_present);

            interfaces.push(NetworkInterface {
                name,
                kind,
                wireless,
                driver_present,
                device_present,
                address_present,
                stats_present,
            });
        }
    }

    interfaces.sort_by(|left, right| left.name.cmp(&right.name));
    NetworkDiscovery { interfaces }
}

fn classify(
    name: &str,
    wireless: bool,
    device_present: bool,
    driver_present: bool,
) -> InterfaceKind {
    if name == "lo" {
        InterfaceKind::Loopback
    } else if wireless || device_present || driver_present {
        InterfaceKind::Physical
    } else {
        InterfaceKind::Virtual
    }
}

fn zero_address(value: &str) -> bool {
    let value = value.trim().to_ascii_lowercase();
    value.is_empty() || value == "00:00:00:00:00:00" || value == "0:0:0:0:0:0"
}
