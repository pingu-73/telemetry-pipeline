import os
import fastf1
from fastf1 import Cache
import pandas as pd
import numpy as np
from typing import Generator, Dict
import time

from telemetry_packet import F1TelemetryPacket, PacketPriority
from config import *

class F1DataSource:
    def __init__(self, year: int = DEFAULT_YEAR, race: str = DEFAULT_RACE):
        self.year = year
        self.race = race
        self.session = None
        self.car_data = None
        self.lap_data = None
        self.packet_counter = 0
        self.lap_boundaries = []  # start_idx, end_idx, lap_number, lap_time
        self.current_lap = 1
        self.total_laps = 0
        
        if not os.path.exists(CACHE_DIR):
            os.makedirs(CACHE_DIR)
            print(f"[INIT] Created cache directory: {CACHE_DIR}")
        
        Cache.enable_cache(CACHE_DIR)
        
        print(f"[INIT] F1 Data Source for {year} {race}")
        print(f"[INIT] Simulating F1 telemetry at {BASE_FREQUENCY_HZ}Hz")
    

    def load_session(self, session_type: str = TARGET_SESSION) -> bool:
        try:
            print(f"\n[LOAD] Fetching {self.year} {self.race} {session_type}...")
            print("[LOAD] Wait for a while...")
            
            self.session = fastf1.get_session(self.year, self.race, session_type)
            self.session.load(telemetry=True)
            
            driver_laps = self.session.laps.pick_drivers(TARGET_CAR_NUMBER)
            
            if driver_laps.empty:
                print(f"[ERROR] No data for car #{TARGET_CAR_NUMBER}")
                return False
            
            print(f"[INFO] Found {len(driver_laps)} laps for car #{TARGET_CAR_NUMBER}")
            
            if LOAD_ALL_LAPS and len(driver_laps) > 1:
                complete_laps = driver_laps[driver_laps['LapTime'].notna()]
                
                if MAX_LAPS_TO_LOAD:
                    complete_laps = complete_laps.iloc[:MAX_LAPS_TO_LOAD]
                    print(f"[INFO] Limited to {MAX_LAPS_TO_LOAD} laps per config")


                telemetry_list = []
                cumulative_time = pd.Timedelta(0)
                cumulative_samples = 0

                # Track lap boundaries
                self.lap_boundaries = []
                self.total_laps = len(complete_laps)

                for idx, lap in complete_laps.iterrows():
                    try:
                        lap_telemetry = lap.get_telemetry()
                        if lap_telemetry is not None and not lap_telemetry.empty:
                            # time cumulative across laps
                            lap_telemetry = lap_telemetry.copy()
                            lap_telemetry['Time'] = lap_telemetry['Time'] + cumulative_time
                            
                            # store lap boundary info
                            start_idx = cumulative_samples
                            end_idx = cumulative_samples + len(lap_telemetry) - 1
                            lap_info = (
                                start_idx,
                                end_idx,
                                lap['LapNumber'],
                                lap['LapTime']
                            )
                            self.lap_boundaries.append(lap_info)

                            telemetry_list.append(lap_telemetry)
                            # print(f"  Lap {lap['LapNumber']}: {len(lap_telemetry)} samples, "f"LapTime: {lap['LapTime']}")
                            
                            # update cumulative time for next lap
                            cumulative_samples += len(lap_telemetry)
                            if not lap_telemetry.empty:
                                cumulative_time = lap_telemetry['Time'].iloc[-1]
                    except Exception as e:
                        print(f"  Warning: Could not load lap {lap['LapNumber']}: {e}")
                        continue
                
                if telemetry_list:
                    self.car_data = pd.concat(telemetry_list, ignore_index=True)
                    print(f"[INFO] Combined telemetry: {len(self.car_data)} total samples")
                    print(f"[INFO] Total race time covered: {cumulative_time}")
                    print(f"[INFO] Loaded {self.total_laps} laps for streaming")
                else:
                    # fallback
                    print("[WARN] No valid telemetry found, using fastest lap")
                    fastest_lap = driver_laps.pick_fastest()
                    self.car_data = fastest_lap.get_telemetry()
                    self.total_laps = 1
                    self.lap_boundaries = [(0, len(self.car_data)-1, fastest_lap['LapNumber'], fastest_lap['LapTime'])]
            else:
                fastest_lap = driver_laps.pick_fastest()
                self.car_data = fastest_lap.get_telemetry()
            
            self._clean_telemetry_data()
            
            print(f"[LOAD] Loaded {len(self.car_data)} telemetry samples")
            print(f"[INFO] Original sample rate: ~{self._calculate_original_rate():.0f}Hz")
            print(f"[INFO] Interpolate to: {BASE_FREQUENCY_HZ}Hz")
            
            # DBG: telemetry data
            print("\n[DIAGNOSTIC] Telemetry data range:")
            print(f"  Speed: {self.car_data['Speed'].min():.0f} - {self.car_data['Speed'].max():.0f} km/h")
            print(f"  Throttle: {self.car_data['Throttle'].min():.0f} - {self.car_data['Throttle'].max():.0f}%")
            print(f"  Brake: {self.car_data['Brake'].min():.1f} - {self.car_data['Brake'].max():.1f}")
            print(f"  Gear: {self.car_data['nGear'].min()} - {self.car_data['nGear'].max()}")
            print(f"  RPM: {self.car_data['RPM'].min():.0f} - {self.car_data['RPM'].max():.0f}")
            
            # DBG: data variation
            print("\n[DIAGNOSTIC] Data variation check:")
            speed_changes = self.car_data['Speed'].diff().abs().sum()
            throttle_changes = self.car_data['Throttle'].diff().abs().sum()
            brake_applications = (self.car_data['Brake'] > 0.5).sum()
            
            print(f"  Total speed changes: {speed_changes:.0f} km/h")
            print(f"  Total throttle changes: {throttle_changes:.0f}%")
            print(f"  Heavy brake applications: {brake_applications}")
            
            if speed_changes < 1000:
                print("  !!  WARNING: Low variation in data - may show static telemetry!")
            
            return True
            
        except Exception as e:
            print(f"[ERROR] Failed to load session: {e}")
            import traceback
            traceback.print_exc()
            return False
        
    def _get_current_lap_info(self, telemetry_idx: int, total_interpolated_samples: int) -> tuple:
        if not self.lap_boundaries:
            return 1, 0.0, None
        
        # calculate scaling factor for interpolated data
        original_total_samples = sum(end - start + 1 for start, end, _, _ in self.lap_boundaries)
        scale_factor = total_interpolated_samples / original_total_samples if original_total_samples > 0 else 1
        
        # scale boundaries to match interpolated data
        for start_idx, end_idx, lap_num, lap_time in self.lap_boundaries:
            scaled_start = int(start_idx * scale_factor)
            scaled_end = int((end_idx + 1) * scale_factor) - 1
            
            if scaled_start <= telemetry_idx <= scaled_end:
                lap_length = scaled_end - scaled_start + 1
                lap_progress = ((telemetry_idx - scaled_start) / lap_length) * 100 if lap_length > 0 else 0
                return lap_num, lap_progress, lap_time
        
        # If we're beyond all boundaries, return the last lap
        return self.lap_boundaries[-1][2], 100.0, self.lap_boundaries[-1][3]

    def _clean_telemetry_data(self):
        if self.car_data is None:
            return
        
        # brake column in FastF1 api is boolean or NaN
        # converting to float
        if 'Brake' in self.car_data.columns:
            brake_data = self.car_data['Brake'].copy()

            brake_data = brake_data.replace({False: 0.0, True: 1.0})

            self.car_data['Brake'] = pd.to_numeric(brake_data, errors='coerce').fillna(0.0)
        
        numeric_columns = ['Speed', 'Throttle', 'RPM', 'nGear', 'DRS']
        for col in numeric_columns:
            if col in self.car_data.columns:
                self.car_data[col] = pd.to_numeric(self.car_data[col], errors='coerce').fillna(0)
    

    def _calculate_original_rate(self) -> float:
        if self.car_data is None or len(self.car_data) < 2:
            return 0.0
        
        time_diff = (self.car_data['Time'].iloc[-1] - self.car_data['Time'].iloc[0]).total_seconds()
        samples = len(self.car_data)
        
        return samples / time_diff if time_diff > 0 else 0.0
    

    def _interpolate_to_realistic_rate(self, df: pd.DataFrame, target_hz: int = BASE_FREQUENCY_HZ) -> pd.DataFrame:
        if df is None or df.empty:
            return df
        
        print(f"[INTERP] Upsampling to {target_hz}Hz...")
        
        df = df.copy()
        df = df.drop_duplicates(subset=['Time'], keep='first')
        df = df.sort_values('Time')
        
        start_time = df['Time'].iloc[0]
        end_time = df['Time'].iloc[-1]
        duration = (end_time - start_time).total_seconds()
        
        print(f"[INTERP] Duration to interpolate: {duration:.1f} seconds")
        
        target_samples = int(duration * target_hz)
        time_index = pd.to_timedelta(np.linspace(0, duration, target_samples), unit='s')
        
        df_indexed = df.set_index('Time')
        
        for col in df_indexed.columns:
            if col not in ['Date', 'Time', 'Driver']:  # skip non-numeric col
                df_indexed[col] = pd.to_numeric(df_indexed[col], errors='coerce')
        
        continuous_cols = ['Speed', 'Throttle', 'Brake', 'RPM', 'DRS']
        discrete_cols = ['nGear']
        
        df_uniform = pd.DataFrame(index=time_index + start_time)
        
        # interpolate cont col
        for col in continuous_cols:
            if col in df_indexed.columns:
                series = pd.to_numeric(df_indexed[col], errors='coerce').fillna(0)
                df_uniform[col] = series.reindex(df_uniform.index, method='nearest').interpolate(method='linear').fillna(0)
        
        # fwd fill dis col
        for col in discrete_cols:
            if col in df_indexed.columns:
                df_uniform[col] = df_indexed[col].reindex(df_uniform.index, method='nearest').ffill().fillna(0)
        
        # remaining numeric col
        for col in df_indexed.columns:
            if col not in df_uniform.columns and col not in ['Date', 'Time', 'Driver']:
                try:
                    series = pd.to_numeric(df_indexed[col], errors='coerce')
                    df_uniform[col] = series.reindex(df_uniform.index, method='nearest').interpolate(method='linear').fillna(0)
                except:  # noqa: E722
                    pass  # skip if can't be interpolated
        
        df_uniform = df_uniform.reset_index()
        df_uniform.rename(columns={'index': 'Time'}, inplace=True)
        
        print(f"[INTERP] Generated {len(df_uniform)} samples ({target_hz}Hz)")
        print(f"[INTERP] This represents {len(df_uniform)/target_hz:.1f} seconds of racing")
        
        return df_uniform
    

    def _simulate_sensor_data(self, base_telemetry: pd.Series) -> Dict:
        rpm = float(base_telemetry.get('RPM', 8000))
        if pd.isna(rpm):
            rpm = 8000.0
            
        throttle_raw = base_telemetry.get('Throttle', 0)
        if pd.isna(throttle_raw):
            throttle_raw = 0
        throttle = float(throttle_raw) / 100.0
        
        speed = float(base_telemetry.get('Speed', 0))
        if pd.isna(speed):
            speed = 0.0
            
        brake_raw = base_telemetry.get('Brake', 0)

        if isinstance(brake_raw, bool):
            brake = 1.0 if brake_raw else 0.0
        elif pd.isna(brake_raw):
            brake = 0.0
        else:
            brake = float(brake_raw)
            
        drs = float(base_telemetry.get('DRS', 0))
        if pd.isna(drs):
            drs = 0.0
        
        # simulating oil pr based on RPM
        oil_pressure = 4.0 + (rpm / 15000) * 2.0
        
        # simulating temp based on throttle and speed
        oil_temp = 90 + throttle * 20 + np.random.normal(0, 2)
        water_temp = 85 + throttle * 25 + np.random.normal(0, 2)
        exhaust_temp = 600 + throttle * 300
        
        # simulating ERS deployment
        ers_store = 4000000 * (0.5 + 0.5 * np.sin(self.packet_counter / 100))  # Joules
        mguk_power = throttle * 120000 if drs > 0 else 0  # Watts
        
        # simulating fuel flow (max 100kg/h)
        fuel_flow = throttle * 100  # kg/h
        
        # tyre temps inc with speed and brake
        speed_factor = speed / 350 if speed > 0 else 0
        brake_factor = brake
        base_tyre_temp = 80
        
        tyre_temps = [
            int(base_tyre_temp + speed_factor * 20 + brake_factor * 30),  # FL
            int(base_tyre_temp + speed_factor * 20 + brake_factor * 30),  # FR
            int(base_tyre_temp + speed_factor * 15),  # RL
            int(base_tyre_temp + speed_factor * 15),  # RR
        ]
        
        return {
            'oil_pressure': oil_pressure,
            'oil_temp': int(oil_temp),
            'water_temp': int(water_temp),
            'exhaust_temp': int(exhaust_temp),
            'ers_store': ers_store,
            'mguk_power': mguk_power,
            'fuel_flow': fuel_flow,
            'tyre_temps': tyre_temps,
            'tyre_core_temps': [t + 5 for t in tyre_temps]
        }

    def generate_packets(self, realtime: bool = True) -> Generator[F1TelemetryPacket, None, None]:
        if self.car_data is None:
            print("[ERROR] No data loaded")
            return
        
        telemetry = self._interpolate_to_realistic_rate(self.car_data)
        total_interpolated_samples = len(telemetry)
        
        print(f"\n[INFO] Generating packets for {len(telemetry)} telemetry points")
        print(f"[INFO] Streaming {self.total_laps} lap(s)")
        print(f"[INFO] Data preview - First: Speed={telemetry['Speed'].iloc[0]:.0f}, "
            f"Last: Speed={telemetry['Speed'].iloc[-1]:.0f}")
        
        total_duration = len(telemetry) / BASE_FREQUENCY_HZ
        print(f"[INFO] Total stream duration: {total_duration:.1f} seconds\n")
        
        start_time = time.time()
        base_timestamp = int(start_time * 1000)
        
        last_lap_num = 0
        lap_start_packet = 0
        
        for idx in range(len(telemetry)):
            row = telemetry.iloc[idx]
            
            current_lap_num, lap_progress, lap_time = self._get_current_lap_info(idx, total_interpolated_samples)
            
            if current_lap_num != last_lap_num:
                if last_lap_num > 0:
                    lap_packets = self.packet_counter - lap_start_packet
                    print(f"\n[LAP COMPLETE] Lap {last_lap_num} finished - {lap_packets} packets sent")
                
                print(f"\n{'='*60}")
                print(f"[LAP START] Now streaming Lap {int(current_lap_num)} of {self.total_laps}")
                if lap_time:
                    print(f"[LAP INFO] Lap time: {lap_time}")
                print(f"{'='*60}\n")
                
                last_lap_num = current_lap_num
                lap_start_packet = self.packet_counter
            
            # update current lap for external access
            self.current_lap = int(current_lap_num)
            
            # simulate additional sensors
            sensor_data = self._simulate_sensor_data(row)
            
            # determine packet priority based on conditions
            priority = PacketPriority.HIGH
            if sensor_data['water_temp'] > 120:
                priority = PacketPriority.CRITICAL
            elif row.get('DRS', 0) > 0:
                priority = PacketPriority.HIGH
            
            def safe_int(value, default=0):
                if pd.isna(value):
                    return default
                try:
                    return int(float(value))
                except:  # noqa: E722
                    return default
            
            def safe_float(value, default=0.0):
                if pd.isna(value):
                    return default
                try:
                    return float(value)
                except:  # noqa: E722
                    return default
            
            packet = F1TelemetryPacket(
                timestamp_ms=base_timestamp + int(self.packet_counter * PACKET_INTERVAL_MS),
                car_number=TARGET_CAR_NUMBER,
                packet_id=self.packet_counter,
                priority=priority,
                
                # telemetry
                speed_kmh=safe_int(row.get('Speed', 0)),
                throttle_percent=safe_float(row.get('Throttle', 0)) / 100.0,
                brake_percent=safe_float(row.get('Brake', 0)),
                steering_angle=0.0,
                gear=safe_int(row.get('nGear', 0)),
                engine_rpm=safe_int(row.get('RPM', 0)),
                drs_active=bool(safe_float(row.get('DRS', 0))),
                
                # engine params
                oil_pressure_bar=sensor_data['oil_pressure'],
                oil_temp_c=sensor_data['oil_temp'],
                water_temp_c=sensor_data['water_temp'],
                exhaust_temp_c=sensor_data['exhaust_temp'],
                
                # tyre data
                tyre_pressure_psi=[23.0, 23.0, 21.0, 21.0],
                tyre_temp_surface_c=sensor_data['tyre_temps'],
                tyre_temp_core_c=sensor_data['tyre_core_temps'],
                
                # energy systems
                ers_store_j=sensor_data['ers_store'],
                mguk_power_w=sensor_data['mguk_power'],
                
                # fuel
                fuel_flow_rate_kg_h=sensor_data['fuel_flow'],
                fuel_remaining_kg=110.0 - (self.packet_counter * 0.0001)  # simulate fuel usage from 110kg
            )
            
            self.packet_counter += 1
            
            # DBG
            if self.packet_counter % 1000 == 0:
                elapsed = time.time() - start_time
                pps = self.packet_counter / elapsed if elapsed > 0 else 0
                overall_progress = (idx / len(telemetry)) * 100
                
                print(f"[DEBUG] Packet {self.packet_counter}: "
                    f"Speed={packet.speed_kmh} "
                    f"Throttle={packet.throttle_percent:.1%} "
                    f"Brake={packet.brake_percent:.1%} "
                    f"Gear={packet.gear}")
                print(f"[STREAM] Lap {int(current_lap_num)}/{self.total_laps} "
                    f"({lap_progress:.1f}% of lap) | "
                    f"Overall: {overall_progress:.1f}% | "
                    f"{pps:.0f} pps")
            
            # are we at the end
            if idx == len(telemetry) - 1:
                lap_packets = self.packet_counter - lap_start_packet
                print(f"\n[LAP COMPLETE] Lap {int(current_lap_num)} finished - {lap_packets} packets sent")
                print(f"\n{'='*60}")
                print(f"[COMPLETE] Finished streaming all {self.total_laps} laps")
                print(f"[INFO] Total packets sent: {self.packet_counter}")
                print(f"[INFO] Total time streamed: {total_duration:.1f} seconds")
                print(f"{'='*60}")
            
            yield packet

    def _print_lap_metrics(self, lap_num: int, lap_progress: float, pps: float):
        pass  # handled by UDPTelemetryStreamer