class StackFrame:
    """解析器栈帧, 用于替代递归调用栈."""

    __slots__ = (
        "container",
        "key",
        "state",
        "size",
        "index",
        "key_tag",
        "key_type",
        "value_tag",
        "value_type",
    )

    def __init__(
        self,
        container: Any,
        key: Any = None,
        state: int = 0,
        size: int = 0,
        index: int = 0,
    ):
        self.container = container
        self.key = key  # 对于 Map 是当前的 Key, 对于 Struct 是当前的 Tag
        self.state = state  # 0: Init, 1: ReadingKey, 2: ReadingValue, etc.
        self.size = size  # List/Map 的大小
        self.index = index  # 当前处理的索引

        # 临时存储头部信息
        self.key_tag: int = 0
        self.key_type: int = 0
        self.value_tag: int = 0
        self.value_type: int = 0
