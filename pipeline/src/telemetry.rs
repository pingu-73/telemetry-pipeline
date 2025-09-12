//! F1 Telemetry packet definitions
use serde::{Deserialize, Serialize};

/// zero-copy telemetry decoder that reads directly from bytes
pub struct TelemetryDecoder<'a> {
    data: &'a [u8],
    cursor: usize,
}

impl<'a> TelemetryDecoder<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, cursor: 0 }
    }

    /// read MessagePack format marker
    fn read_marker(&mut self) -> Result<u8, &'static str> {
        if self.cursor >= self.data.len() {
            return Err("Buffer underflow");
        }
        let marker = self.data[self.cursor];
        self.cursor += 1;
        Ok(marker)
    }

    /// skip a field without deserializing
    fn skip_field(&mut self) -> Result<(), &'static str> {
        let marker = self.read_marker()?;

        match marker {
            // +ve fixint (0x00-0x7f)
            0x00..=0x7f => Ok(()),
            // Fixmap (0x80-0x8f)
            0x80..=0x8f => {
                let len = (marker & 0x0f) as usize;
                for _ in 0..len * 2 {
                    self.skip_field()?;
                }
                Ok(())
            }
            // Fixarray (0x90-0x9f)
            0x90..=0x9f => {
                let len = (marker & 0x0f) as usize;
                for _ in 0..len {
                    self.skip_field()?;
                }
                Ok(())
            }
            // Fixstr (0xa0-0xbf)
            0xa0..=0xbf => {
                let len = (marker & 0x1f) as usize;
                self.cursor += len;
                Ok(())
            }
            // nil, false, true
            0xc0 | 0xc2 | 0xc3 => Ok(()),
            // float32
            0xca => {
                self.cursor += 4;
                Ok(())
            }
            // float64
            0xcb => {
                self.cursor += 8;
                Ok(())
            }
            // uint8
            0xcc => {
                self.cursor += 1;
                Ok(())
            }
            // uint16
            0xcd => {
                self.cursor += 2;
                Ok(())
            }
            // uint32
            0xce => {
                self.cursor += 4;
                Ok(())
            }
            // uint64
            0xcf => {
                self.cursor += 8;
                Ok(())
            }
            // int8
            0xd0 => {
                self.cursor += 1;
                Ok(())
            }
            // int16
            0xd1 => {
                self.cursor += 2;
                Ok(())
            }
            // int32
            0xd2 => {
                self.cursor += 4;
                Ok(())
            }
            // int64
            0xd3 => {
                self.cursor += 8;
                Ok(())
            }
            // str8
            0xd9 => {
                let len = self.data[self.cursor] as usize;
                self.cursor += 1 + len;
                Ok(())
            }
            // -ve fixint (0xe0-0xff)
            0xe0..=0xff => Ok(()),
            _ => Err("Unknown MessagePack marker"),
        }
    }

    /// read u64 directly from bytes
    pub fn read_u64(&mut self) -> Result<u64, &'static str> {
        let marker = self.read_marker()?;

        match marker {
            // +ve fixint
            0x00..=0x7f => Ok(marker as u64),
            // uint8
            0xcc => {
                let val = self.data[self.cursor];
                self.cursor += 1;
                Ok(val as u64)
            }
            // uint16
            0xcd => {
                let val = u16::from_be_bytes([self.data[self.cursor], self.data[self.cursor + 1]]);
                self.cursor += 2;
                Ok(val as u64)
            }
            // uint32
            0xce => {
                let val = u32::from_be_bytes([
                    self.data[self.cursor],
                    self.data[self.cursor + 1],
                    self.data[self.cursor + 2],
                    self.data[self.cursor + 3],
                ]);
                self.cursor += 4;
                Ok(val as u64)
            }
            // uint64
            0xcf => {
                let val = u64::from_be_bytes([
                    self.data[self.cursor],
                    self.data[self.cursor + 1],
                    self.data[self.cursor + 2],
                    self.data[self.cursor + 3],
                    self.data[self.cursor + 4],
                    self.data[self.cursor + 5],
                    self.data[self.cursor + 6],
                    self.data[self.cursor + 7],
                ]);
                self.cursor += 8;
                Ok(val)
            }
            _ => Err("Expected integer"),
        }
    }

    /// read u16 without allocation
    pub fn read_u16(&mut self) -> Result<u16, &'static str> {
        let val = self.read_u64()?;
        Ok(val as u16)
    }

    /// read priority field
    pub fn read_priority(&mut self) -> Result<u8, &'static str> {
        let val = self.read_u64()?;
        Ok(val as u8)
    }

    /// navigate to specific field in map without full deserialization
    pub fn find_field(&mut self, target_key: &str) -> Result<bool, &'static str> {
        // reset to start for simplicity
        self.cursor = 0;

        // read map header
        let marker = self.read_marker()?;
        let map_len = match marker {
            0x80..=0x8f => (marker & 0x0f) as usize,
            0xde => {
                let len = u16::from_be_bytes([self.data[self.cursor], self.data[self.cursor + 1]]);
                self.cursor += 2;
                len as usize
            }
            _ => return Err("Not a map"),
        };

        // scan through map entries
        for _ in 0..map_len {
            // check if this key matches
            let key_start = self.cursor;
            let key_marker = self.data[self.cursor];

            if key_marker >= 0xa0 && key_marker <= 0xbf {
                // fixstr
                let key_len = (key_marker & 0x1f) as usize;
                self.cursor += 1;

                if key_len == target_key.len() {
                    let key_bytes = &self.data[self.cursor..self.cursor + key_len];
                    if key_bytes == target_key.as_bytes() {
                        self.cursor += key_len;
                        return Ok(true);
                    }
                }
                self.cursor = key_start;
            }

            // skip this key
            self.skip_field()?;
            // skip the value
            self.skip_field()?;
        }

        Ok(false)
    }
}

/// minimal telemetry data for critical fields only
pub struct FastTelemetry<'a> {
    decoder: TelemetryDecoder<'a>,
    // cache essential fields
    packet_id: Option<u32>,
    priority: Option<u8>,
    speed: Option<u16>,
}

impl<'a> FastTelemetry<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            decoder: TelemetryDecoder::new(data),
            packet_id: None,
            priority: None,
            speed: None,
        }
    }

    /// get packet ID with lazy parsing
    pub fn packet_id(&mut self) -> Result<u32, &'static str> {
        if let Some(id) = self.packet_id {
            return Ok(id);
        }

        if self.decoder.find_field("id")? {
            let id = self.decoder.read_u64()? as u32;
            self.packet_id = Some(id);
            Ok(id)
        } else {
            Err("Packet ID field not found")
        }
    }

    /// get priority with lazy parsing
    pub fn priority(&mut self) -> Result<u8, &'static str> {
        if let Some(p) = self.priority {
            return Ok(p);
        }

        if self.decoder.find_field("p")? {
            let p = self.decoder.read_priority()?;
            self.priority = Some(p);
            Ok(p)
        } else {
            Ok(1) // default priority
        }
    }

    /// get speed with lazy parsing (for load simulation)
    pub fn speed(&mut self) -> Result<u16, &'static str> {
        if let Some(spd) = self.speed {
            return Ok(spd);
        }

        if self.decoder.find_field("spd")? {
            let spd = self.decoder.read_u16()?;
            self.speed = Some(spd);
            Ok(spd)
        } else {
            Err("Speed field not found")
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TelemetryPacket {
    pub t: u64,       // timestamp_ms
    pub id: u32,      // packet_id
    pub p: u8,        // priority
    pub spd: u16,     // speed_kmh
    pub thr: f32,     // throttle_percent
    pub brk: f32,     // brake_percent
    pub str: f32,     // steering_angle
    pub g: i8,        // gear
    pub rpm: u16,     // engine_rpm
    pub drs: bool,    // drs_active
    pub oilp: f32,    // oil_pressure_bar
    pub oilt: i16,    // oil_temp_c
    pub h2ot: i16,    // water_temp_c
    pub tp: Vec<f32>, // tyre_pressure_psi
    pub tt: Vec<i16>, // tyre_temp_surface_c
    pub ers: f32,     // ers_store_j
    pub mguk: f32,    // mguk_power_w
    pub fuel: f32,    // fuel_flow_rate_kg_h
}

impl TelemetryPacket {
    pub fn from_bytes(data: &[u8]) -> Result<Self, rmp_serde::decode::Error> {
        rmp_serde::from_slice(data)
    }
}
