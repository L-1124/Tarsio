from typing import Any

def dumps(
    obj: Any,
    schema: list[tuple[Any, ...]],
    options: int = 0,
    context: dict[str, Any] | None = None,
) -> bytes: ...
def dumps_generic(
    obj: Any, options: int = 0, context: dict[str, Any] | None = None
) -> bytes: ...
def loads(
    data: bytes,
    schema: list[tuple[Any, ...]],
    options: int = 0,
    context: dict[str, Any] | None = None,
) -> dict[Any, Any]: ...
def loads_generic(
    data: bytes, options: int = 0, context: dict[str, Any] | None = None
) -> dict[Any, Any]: ...
