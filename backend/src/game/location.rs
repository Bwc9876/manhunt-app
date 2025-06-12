use serde::{Deserialize, Serialize};

/// A "part" of a location
pub type LocationComponent = f64;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
/// Some location in the world as gotten from a Geolocation API
pub struct Location {
    /// Latitude
    pub lat: LocationComponent,
    /// Longitude
    pub long: LocationComponent,
    /// The bearing (float normalized from 0 to 1) optional as GPS can't always determine
    pub heading: Option<LocationComponent>,
}

pub trait LocationService {
    fn get_loc(&self) -> Location;
}
