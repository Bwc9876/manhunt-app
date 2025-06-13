use tauri::AppHandle;
use tauri_plugin_geolocation::{GeolocationExt, PositionOptions};

use crate::game::{Location, LocationService};

pub struct TauriLocation(AppHandle);

impl TauriLocation {
    pub fn new(app: AppHandle) -> Self {
        Self(app)
    }
}

const OPTIONS: PositionOptions = PositionOptions {
    enable_high_accuracy: true,
    timeout: 10000, // Unused in our case, set to default
    maximum_age: 2000,
};

impl LocationService for TauriLocation {
    fn get_loc(&self) -> Option<Location> {
        match self.0.geolocation().get_current_position(Some(OPTIONS)) {
            Ok(pos) => {
                let coords = pos.coords;
                let loc = Location {
                    lat: coords.latitude,
                    long: coords.longitude,
                    heading: coords.heading,
                };
                Some(loc)
            }
            Err(why) => {
                eprintln!("Failed to get loc: {why:?}");
                None
            }
        }
    }
}
