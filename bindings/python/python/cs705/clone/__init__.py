"""Clone protocols over explicitly supplied transports; no device discovery."""
from .._native import clone as _clone
from typing import Protocol

class Transport(Protocol):
    """Bounded synchronous binary I/O on an explicitly selected open transport.

    Calls use positional arguments on the invoking thread. Empty reads mean no
    data for this poll; writes return the actual prefix count. now_ms returns
    monotonic unsigned 64-bit milliseconds. pending_output includes OS queues.
    A session attempts close once, including after a provider exception; all
    methods, including close, must return or raise within the provider's bound.
    """

    def read(self, maximum: int, /) -> bytes: ...
    def write(self, data: bytes, /) -> int: ...
    def flush(self) -> None: ...
    def now_ms(self) -> int: ...
    def wait_ms(self, milliseconds: int, /) -> None: ...
    def pending_output(self) -> int: ...
    def reconfigure(self, settings_token: bytes, /) -> None: ...
    def close(self) -> None: ...

Direction = _clone.Direction
Error = _clone.Error
Profile = _clone.Profile
ContextFields = _clone.ContextFields
Identity = _clone.Identity
IdentityGate = _clone.IdentityGate
Frame = _clone.Frame
Framer = _clone.Framer
Extractor = _clone.Extractor
IMAGE_CAPACITY = _clone.IMAGE_CAPACITY
decode_hex_script = _clone.decode_hex_script
script_token = _clone.script_token
read_range = _clone.read_range
write_blocks = _clone.write_blocks
write_blocks_with_cap = _clone.write_blocks_with_cap
convert_image = _clone.convert_image
Echo = _clone.Echo
Operation = _clone.Operation
State = _clone.State
WriteDisposition = _clone.WriteDisposition
ErrorKind = _clone.ErrorKind
TransportSettings = _clone.TransportSettings
SessionConfig = _clone.SessionConfig
ErrorData = _clone.ErrorData
Failure = _clone.Failure
Outcome = _clone.Outcome
Session = _clone.Session
from . import search, information

__all__ = ["Direction", "Profile", "ContextFields", "Identity", "IdentityGate", "Frame", "Framer", "Extractor", "IMAGE_CAPACITY", "decode_hex_script", "script_token", "read_range", "write_blocks", "write_blocks_with_cap", "convert_image"]
__all__ += ["Echo", "Operation", "State", "WriteDisposition", "ErrorKind", "TransportSettings", "SessionConfig", "ErrorData", "Failure", "Outcome", "Session"]
__all__ += ["search", "information", "Transport", "Error"]
