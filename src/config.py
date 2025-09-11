# using  med freq as baseline
BASE_FREQUENCY_HZ = 500  # F1 game uses 20-60 Hz

# UDP Configs
UDP_PORT = 20777  # same as F1 game
UDP_PACKET_SIZE = 512

# timing constraints
MAX_LATENCY_MS = 10  # =<10ms end2end
PACKET_INTERVAL_MS = 1000.0 / BASE_FREQUENCY_HZ  # 2ms at 500Hz

# telemetry priority list
PRIORITY_CHANNELS = [
    'speed', 'throttle', 'brake', 'gear', 'rpm',  # for race engineer
    'tyre_pressure', 'tyre_temp',  # for strategy
    'fuel_flow', 'ers_deployment'  # for energy management
]

# target 
TARGET_CAR_NUMBER = 44
TARGET_SESSION = 'Race'
LOAD_FULL_RACE = True
LOAD_ALL_LAPS = True
MAX_LAPS_TO_LOAD = None

DEFAULT_YEAR = 2024
DEFAULT_RACE = 'Silverstone'
CACHE_DIR = './f1_cache'