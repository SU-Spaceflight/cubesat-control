//! Contains all commands that the GC might send to the satellite.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum SystemCommands {}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum FlightComputerCommands {
    YawRight,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum GimbalCommands {
    TrimPan(f32),
    TrimTilt(f32),
    SetPan(f32),
    SetTilt(f32),
    ManualOverride { pan: f32, tilt: f32 },
    SetTrackingMode(bool),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "dest", content = "cmd", rename_all = "snake_case")]
pub enum CommandEnvelope {
    System(SystemCommands),
    Flight(FlightComputerCommands),
    Gimbal(GimbalCommands),
}
