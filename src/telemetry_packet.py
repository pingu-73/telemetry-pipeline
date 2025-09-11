from dataclasses import dataclass
from typing import List
import msgpack
from enum import IntEnum

class PacketPriority(IntEnum):
    CRITICAL = 0  # brake failure, etc
    HIGH = 1      # speed, throttle, brake
    MEDIUM = 2    # tyres, fuel
    LOW = 3

@dataclass
class F1TelemetryPacket:
    # packet metadata
    timestamp_ms: int
    car_number: int
    packet_id: int
    priority: int = PacketPriority.HIGH
    
    # core telemetry
    speed_kmh: int = 0
    throttle_percent: float = 0.0
    brake_percent: float = 0.0
    steering_angle: float = 0.0  # -1.0 to 1.0
    gear: int = 0
    engine_rpm: int = 0
    drs_active: bool = False
    
    # engine params
    oil_pressure_bar: float = 0.0
    oil_temp_c: int = 0
    water_temp_c: int = 0
    exhaust_temp_c: int = 0
    
    # tyre data
    tyre_pressure_psi: List[float] = None  # [FL, FR, RL, RR]
    tyre_temp_surface_c: List[int] = None
    tyre_temp_core_c: List[int] = None
    
    # energy systems (ERS)
    ers_store_j: float = 0.0  # Joules
    ers_deploy_mode: int = 0
    mguk_power_w: float = 0.0  # Watts
    mguh_power_w: float = 0.0
    
    # fuel system
    fuel_flow_rate_kg_h: float = 0.0
    fuel_remaining_kg: float = 0.0
    
    def __post_init__(self):
        if self.tyre_pressure_psi is None:
            self.tyre_pressure_psi = [23.0, 23.0, 21.0, 21.0]
        if self.tyre_temp_surface_c is None:
            self.tyre_temp_surface_c = [90, 90, 85, 85]
        if self.tyre_temp_core_c is None:
            self.tyre_temp_core_c = [95, 95, 90, 90]
    
    def to_udp_bytes(self) -> bytes:
        # different packet types carry different data, only essential data for this packet type
        data = {
            't': self.timestamp_ms,
            'id': self.packet_id,
            'p': self.priority,
            'spd': self.speed_kmh,
            'thr': round(self.throttle_percent, 2),
            'brk': round(self.brake_percent, 2),
            'str': round(self.steering_angle, 3),
            'g': self.gear,
            'rpm': self.engine_rpm,
            'drs': self.drs_active,
            'oilp': round(self.oil_pressure_bar, 1),
            'oilt': self.oil_temp_c,
            'h2ot': self.water_temp_c,
            'tp': [round(p, 1) for p in self.tyre_pressure_psi],
            'tt': self.tyre_temp_surface_c,
            'ers': round(self.ers_store_j, 0),
            'mguk': round(self.mguk_power_w, 0),
            'fuel': round(self.fuel_flow_rate_kg_h, 2)
        }
        return msgpack.packb(data)
    
    @classmethod
    def from_udp_bytes(cls, data: bytes) -> 'F1TelemetryPacket': #udp decoding
        unpacked = msgpack.unpackb(data, raw=False)
        return cls(
            timestamp_ms=unpacked['t'],
            car_number=1,
            packet_id=unpacked['id'],
            priority=unpacked.get('p', PacketPriority.HIGH),
            speed_kmh=unpacked['spd'],
            throttle_percent=unpacked['thr'],
            brake_percent=unpacked['brk'],
            steering_angle=unpacked['str'],
            gear=unpacked['g'],
            engine_rpm=unpacked['rpm'],
            drs_active=unpacked['drs'],
            oil_pressure_bar=unpacked['oilp'],
            oil_temp_c=unpacked['oilt'],
            water_temp_c=unpacked['h2ot'],
            tyre_pressure_psi=unpacked['tp'],
            tyre_temp_surface_c=unpacked['tt'],
            ers_store_j=unpacked['ers'],
            mguk_power_w=unpacked['mguk'],
            fuel_flow_rate_kg_h=unpacked['fuel']
        )
    
    def get_packet_size_bytes(self) -> int:
        return len(self.to_udp_bytes())
    
    def is_critical(self) -> bool:
        return (self.brake_percent > 0.95 or 
                self.water_temp_c > 130 or
                self.oil_pressure_bar < 2.0)