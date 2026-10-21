//! What sort of thing a device probably is.
//!
//! A guess from the evidence a scan gathers - the device's own name, its
//! maker, whether it hides its address - so the list shows a phone as a phone
//! without anyone tagging it. Names are the stronger signal (a device calling
//! itself "Galaxy-S23" settles it), makers the weaker one (Samsung also makes
//! TVs), and the user's choice overrides both.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DeviceKind {
    Router,
    Computer,
    Phone,
    Tablet,
    Tv,
    Speaker,
    Console,
    Printer,
    Camera,
    Storage,
    SmartHome,
    Wearable,
    Network,
    Unknown,
}

/// Name fragments, matched against the lower-cased name. Order matters: the
/// first hit wins, so specific phrases sit above the broad ones they contain.
const NAME_HINTS: &[(&str, DeviceKind)] = &[
    ("ipad", DeviceKind::Tablet),
    ("galaxy-tab", DeviceKind::Tablet),
    ("kindle", DeviceKind::Tablet),
    ("iphone", DeviceKind::Phone),
    ("android", DeviceKind::Phone),
    ("galaxy", DeviceKind::Phone),
    ("pixel", DeviceKind::Phone),
    ("redmi", DeviceKind::Phone),
    ("oneplus", DeviceKind::Phone),
    ("realme", DeviceKind::Phone),
    ("phone", DeviceKind::Phone),
    ("watch", DeviceKind::Wearable),
    ("macbook", DeviceKind::Computer),
    ("imac", DeviceKind::Computer),
    ("mac-mini", DeviceKind::Computer),
    ("macmini", DeviceKind::Computer),
    ("laptop", DeviceKind::Computer),
    ("desktop", DeviceKind::Computer),
    ("thinkpad", DeviceKind::Computer),
    ("surface", DeviceKind::Computer),
    ("raspberrypi", DeviceKind::Computer),
    ("appletv", DeviceKind::Tv),
    ("apple-tv", DeviceKind::Tv),
    ("chromecast", DeviceKind::Tv),
    ("firetv", DeviceKind::Tv),
    ("fire-tv", DeviceKind::Tv),
    ("roku", DeviceKind::Tv),
    ("bravia", DeviceKind::Tv),
    ("webos", DeviceKind::Tv),
    ("tizen", DeviceKind::Tv),
    ("shield", DeviceKind::Tv),
    ("homepod", DeviceKind::Speaker),
    ("sonos", DeviceKind::Speaker),
    ("echo", DeviceKind::Speaker),
    ("alexa", DeviceKind::Speaker),
    ("google-home", DeviceKind::Speaker),
    ("googlehome", DeviceKind::Speaker),
    ("nest-audio", DeviceKind::Speaker),
    ("nest-mini", DeviceKind::Speaker),
    ("xbox", DeviceKind::Console),
    ("playstation", DeviceKind::Console),
    ("ps4", DeviceKind::Console),
    ("ps5", DeviceKind::Console),
    ("nintendo", DeviceKind::Console),
    ("printer", DeviceKind::Printer),
    ("laserjet", DeviceKind::Printer),
    ("officejet", DeviceKind::Printer),
    ("deskjet", DeviceKind::Printer),
    ("epson", DeviceKind::Printer),
    ("doorbell", DeviceKind::Camera),
    ("camera", DeviceKind::Camera),
    ("ipcam", DeviceKind::Camera),
    ("diskstation", DeviceKind::Storage),
    ("synology", DeviceKind::Storage),
    ("qnap", DeviceKind::Storage),
    ("truenas", DeviceKind::Storage),
    ("nas", DeviceKind::Storage),
    ("tasmota", DeviceKind::SmartHome),
    ("shelly", DeviceKind::SmartHome),
    ("sonoff", DeviceKind::SmartHome),
    ("esp32", DeviceKind::SmartHome),
    ("esp8266", DeviceKind::SmartHome),
    ("esp-", DeviceKind::SmartHome),
    ("wled", DeviceKind::SmartHome),
    ("thermostat", DeviceKind::SmartHome),
    ("plug", DeviceKind::SmartHome),
    ("bulb", DeviceKind::SmartHome),
    ("extender", DeviceKind::Network),
    ("repeater", DeviceKind::Network),
    ("unifi", DeviceKind::Network),
    ("eero", DeviceKind::Network),
    ("orbi", DeviceKind::Network),
    ("deco", DeviceKind::Network),
];

/// Makers whose products are mostly one kind of thing. Mixed makers - Apple,
/// Samsung, Google, TP-Link - are deliberately absent: their name says less
/// than nothing, and a confident wrong icon is worse than a neutral one.
const VENDOR_HINTS: &[(&str, DeviceKind)] = &[
    ("espressif", DeviceKind::SmartHome),
    ("tuya", DeviceKind::SmartHome),
    ("shelly", DeviceKind::SmartHome),
    ("itead", DeviceKind::SmartHome),
    ("signify", DeviceKind::SmartHome),
    ("philips lighting", DeviceKind::SmartHome),
    ("lifx", DeviceKind::SmartHome),
    ("broadlink", DeviceKind::SmartHome),
    ("ecobee", DeviceKind::SmartHome),
    ("synology", DeviceKind::Storage),
    ("qnap", DeviceKind::Storage),
    ("western digital", DeviceKind::Storage),
    ("roku", DeviceKind::Tv),
    ("vizio", DeviceKind::Tv),
    ("hisense", DeviceKind::Tv),
    ("sonos", DeviceKind::Speaker),
    ("bose", DeviceKind::Speaker),
    ("nintendo", DeviceKind::Console),
    ("sony interactive", DeviceKind::Console),
    ("brother", DeviceKind::Printer),
    ("seiko epson", DeviceKind::Printer),
    ("canon", DeviceKind::Printer),
    ("lexmark", DeviceKind::Printer),
    ("kyocera", DeviceKind::Printer),
    ("ricoh", DeviceKind::Printer),
    ("xerox", DeviceKind::Printer),
    ("hikvision", DeviceKind::Camera),
    ("dahua", DeviceKind::Camera),
    ("ezviz", DeviceKind::Camera),
    ("reolink", DeviceKind::Camera),
    ("wyze", DeviceKind::Camera),
    ("arlo", DeviceKind::Camera),
    ("axis communications", DeviceKind::Camera),
    ("fitbit", DeviceKind::Wearable),
    ("garmin", DeviceKind::Wearable),
    ("ubiquiti", DeviceKind::Network),
    ("ruckus", DeviceKind::Network),
    ("aruba", DeviceKind::Network),
    ("raspberry pi", DeviceKind::Computer),
    // Wi-Fi and Ethernet chips that ship inside laptops and desktops.
    ("intel corporate", DeviceKind::Computer),
    ("azurewave", DeviceKind::Computer),
    ("liteon", DeviceKind::Computer),
    ("rivet networks", DeviceKind::Computer),
    ("dell", DeviceKind::Computer),
    ("lenovo", DeviceKind::Computer),
    ("asustek", DeviceKind::Computer),
    ("micro-star", DeviceKind::Computer),
];

/// The best guess for a device that is neither the router nor this machine.
pub fn guess(name: Option<&str>, vendor: Option<&str>, private_address: bool) -> DeviceKind {
    if let Some(name) = name {
        let name = name.to_ascii_lowercase();
        if let Some((_, kind)) = NAME_HINTS.iter().find(|(hint, _)| name.contains(hint)) {
            return *kind;
        }
    }
    if let Some(vendor) = vendor {
        let vendor = vendor.to_ascii_lowercase();
        if let Some((_, kind)) = VENDOR_HINTS.iter().find(|(hint, _)| vendor.contains(hint)) {
            return *kind;
        }
    }
    // Phones randomise their address per network by default; laptops can,
    // but almost never do. The likeliest reading of a private address.
    if private_address {
        return DeviceKind::Phone;
    }
    DeviceKind::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_outranks_its_maker() {
        assert_eq!(guess(Some("Galaxy-S23"), Some("Samsung Electronics"), false), DeviceKind::Phone);
        assert_eq!(guess(Some("Imeshs-iPad"), Some("Apple"), false), DeviceKind::Tablet);
        assert_eq!(guess(Some("DESKTOP-7Q2LM"), None, false), DeviceKind::Computer);
    }

    #[test]
    fn single_purpose_makers_decide_when_the_name_cannot() {
        assert_eq!(guess(None, Some("Espressif"), false), DeviceKind::SmartHome);
        assert_eq!(guess(None, Some("Synology"), false), DeviceKind::Storage);
        assert_eq!(guess(None, Some("Apple"), false), DeviceKind::Unknown, "Apple makes too much to say");
    }

    #[test]
    fn a_private_address_reads_as_a_phone_only_as_a_last_resort() {
        assert_eq!(guess(None, None, true), DeviceKind::Phone);
        assert_eq!(guess(Some("LivingRoom-TV-roku"), None, true), DeviceKind::Tv);
    }
}
