from . import CellNode
class Command:
    depth: int
    separator: bool
    flags1e: int
    shortcut18: int
    image_index: int | None
    def __init__(self, node: CellNode, depth: int, separator: bool, flags1e: int, shortcut18: int, image_index: int | None) -> None: ...
    def node_snapshot(self) -> CellNode: ...
    def set_node(self, v: CellNode) -> None: ...
