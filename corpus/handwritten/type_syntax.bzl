"""Bazel 9 type-annotation syntax. Accepted by released bazel 9.2.0 with no
flags; rejected by buildifier 8.5.1."""

#: A doc comment in the Sphinx style Bazel 9 added.
type Strings = list[str]

count: int = 3

def annotated(name: str, deps: list[str] = [], *, tags: list[str] = []) -> str:
    return name

def with_ellipsis(f: typing.Callable[..., int]) -> int:
    return f()

def uses_conditional_keywords(x):
    if isinstance(x, int):
        return cast(int, x)
    return 0
