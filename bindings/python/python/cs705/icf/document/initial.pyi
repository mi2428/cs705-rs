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
    values: list[int]
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
