//! Defines the configuration options available.

use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct SatelliteConfig {
    pub network: NetworkConfig,
    pub hardware: HardwareConfig,
    pub control: ControlConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct NetworkConfig {
    pub ground_station_ip: String,
    pub telemetry_port: u16,
}

#[derive(Debug, Deserialize, Clone)]
pub struct HardwareConfig {
    pub pan_servo: ServoConfig,
    pub tilt_servo: ServoConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServoConfig {
    pub pin: u8,
    pub min_angle: f32,
    pub max_angle: f32,
    pub min_pulse_ms: f64,
    pub max_pulse_ms: f64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ControlConfig {
    pub pan_pid: PidConfig,
    pub tilt_pid: PidConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PidConfig {
    pub kp: f32,
    pub ki: f32,
    pub kd: f32,
    pub integral_limit: f32,
}
