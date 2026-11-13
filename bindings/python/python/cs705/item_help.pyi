class Input:
    @property
    def variant(self) -> str: ...
    @property
    def key(self) -> int | None: ...
    @property
    def index(self) -> int | None: ...
    @property
    def context58(self) -> bytes | None: ...
    @staticmethod
    def Contents() -> Input: ...
    @staticmethod
    def Item(key: int, index: int, context58: bytes) -> Input: ...
class Request:
    def __init__(self, key: int | None, index: int | None, group: int | None, original_name: bytes, name: bytes, topic: int, chm: bytes, target: bytes) -> None: ...
    key: int | None
    index: int | None
    group: int | None
    topic: int
    original_name: bytes
    name: bytes
    chm: bytes
    target: bytes
class State:
    module: int
    procedure: int
    cookie: int
    d188: int
    d184: int
    def __init__(self, module: int, procedure: int, cookie: int, d188: int, d184: int) -> None: ...
class Api:
    load_library: int | None
    get_proc_address: int | None
    initialize_cookie: int | None
    window_before: int | None
    old_rect: tuple[int, int, int, int] | None
    display_window: int | None
    file_attributes: int | None
    window_after: int | None
    window_info_pointer: int | None
    window_info_rect: tuple[int, int, int, int] | None
    cursor: tuple[int, int] | None
    work_area: tuple[int, int, int, int] | None
    style: int | None
    def __init__(self) -> None: ...
class Call:
    api: str
    command: int | None
    def __init__(self, api: str, command: int | None) -> None: ...
class Placement:
    window: int
    x: int
    y: int
    style: int
    flags: int
    def __init__(self, window: int, x: int, y: int, style: int, flags: int) -> None: ...
class Effects:
    calls: list[Call]
    show_window: list[tuple[int, int]]
    placement: Placement | None
    display_window_observed: int | None
    message_resource_index: int | None
    message_icon: int | None
    def __init__(self) -> None: ...
def resolve(pe: bytes, input: Input, gui_language: int, application_dir: bytes) -> Request: ...
def drive(request: Request, state: State, api: Api, effects: Effects) -> int: ...
