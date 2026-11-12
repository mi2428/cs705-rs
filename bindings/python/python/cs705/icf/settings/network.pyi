from enum import Enum
from ipaddress import IPv4Address

class Field(Enum):
    Address = 0
    Gateway = 1
    PrimaryDns = 2
    SecondaryDns = 3

class Configuration:
    address: IPv4Address
    gateway: IPv4Address
    primary_dns: IPv4Address
    secondary_dns: IPv4Address
    prefix: int
    def __init__(self, address: IPv4Address, gateway: IPv4Address, primary_dns: IPv4Address, secondary_dns: IPv4Address, prefix: int) -> None: ...

class Effect:
    stored: bool
    notify_flags: int
    gray: bool
    def __init__(self, stored: bool, notify_flags: int, gray: bool) -> None: ...

def parse_typed(source: bytes, allow_zero: bool) -> IPv4Address | None: ...
def prefix_mask(prefix: int) -> int: ...
def correct_address(address: IPv4Address, prefix: int) -> IPv4Address: ...
def correct_gateway(address: IPv4Address, gateway: IPv4Address, prefix: int) -> IPv4Address: ...
