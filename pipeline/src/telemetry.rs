//! F1 Telemetry packet definitions
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TelemetryPacket {
    // packet metadata
    pub t: u64,       // timestamp_ms
    pub id: u32,      // packet_id
    pub p: u8,        // priority
    
    // telemetry
    pub spd: u16,     // speed_kmh
    pub thr: f32,     // throttle_percent
    pub brk: f32,     // brake_percent
    pub str: f32,     // steering_angle
    pub g: i8,        // gear
    pub rpm: u16,     // engine_rpm
    pub drs: bool,    // drs_active
    
    // engine params
    pub oilp: f32,    // oil_pressure_bar
    pub oilt: i16,    // oil_temp_c
    pub h2ot: i16,    // water_temp_c
    pub tp: Vec<f32>, // tyre_pressure_psi [FL, FR, RL, RR]
    pub tt: Vec<i16>, // tyre_temp_surface_c
    
    // energy system
    pub ers: f32,     // ers_store_j
    pub mguk: f32,    // mguk_power_w
    pub fuel: f32,    // fuel_flow_rate_kg_h
}

impl TelemetryPacket {
    /// parse packet from MessagePack bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self, rmp_serde::decode::Error> {
        rmp_serde::from_slice(data)
    }
}