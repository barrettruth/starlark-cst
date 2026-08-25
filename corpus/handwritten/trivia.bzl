#!/usr/bin/env starlark
# Leading comment block.
#
# Blank comment line above, trailing whitespace on the next line.

"""Module docstring."""

load(
    "@bazel_skylib//rules:write_file.bzl",  # trailing comment on a load item
    "write_file",
    _aliased = "copy_file",
)

CONSTANT = 1  # trailing comment


def f(
    a,  # first
    b = 2,  # second
    *args,
    **kwargs
):
    # Body comment.
    return a + b  # trailing


LIST = [
    "a",
    # comment between elements
    "b",
]

DICT = {
    "k": "v",  # entry comment
}

CONTINUED = "a" + \
    "b"

TRIPLE = """
  embedded # not a comment
"""

RAW = r"\n not an escape"
BYTES = b"bytes"
