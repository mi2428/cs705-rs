from collections.abc import Iterable
from cs705.clone import Identity, ContextFields
from . import Session

class InitialRequest:
    def __init__(self, mode: int, notification: int, additional: int) -> None: ...
    mode: int
    notification: int
    additional: int
    @staticmethod
    def information(session: Session, identity: Identity, scalar1: int) -> InitialRequest | None: ...
class InitialState:
    def __init__(self) -> None: ...
    saved_selector: int
    @property
    def saved_context(self) -> InitialContext | None: ...
    @saved_context.setter
    def saved_context(self, value: ContextFields | None) -> None: ...
class InitialContext:
    """Live saved-context field; values returns a list copy, not a mutable loan."""
    @property
    def values(self) -> list[int]: ...
    @values.setter
    def values(self, value: Iterable[int]) -> None: ...
    def snapshot(self) -> ContextFields: ...
class InitialBoundary:
    Reconfigure: InitialBoundary
    Additional: InitialBoundary
    Rebuild: InitialBoundary
    ReleaseView: InitialBoundary
    Title: InitialBoundary
    @staticmethod
    def Sync(event: int) -> InitialBoundary: ...
    @property
    def kind(self) -> str: ...
    @property
    def event(self) -> int | None: ...
