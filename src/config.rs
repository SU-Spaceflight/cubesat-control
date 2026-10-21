//! Defines the configuration options available.

use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct SatelliteConfig {
    pub network: NetworkConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct NetworkConfig {
    pub ground_station_ip: String,
    pub telemetry_port: u16,
}
