# Contributing

## Scope

starlark-cst is a parser. Input is bytes, output is a lossless concrete syntax
tree plus a list of errors.

It has no name resolution, no `load()` following, no type inference, and no
concept of labels, targets, packages or a build graph. A change that requires
knowing what a string refers to belongs in the consumer, such as
[`bazel-language-server`](https://github.com/barrettruth/bazel-language-server).

## Pull Requests

Bug fixes and documentation fixes are welcome. AI-generated contributions are
not accepted.

For new behavior, open an issue first unless the change is small and already
fits the project's scope.

Two properties hold for every input, valid or not, and a pull request that
breaks either will not be merged:

- `parse(src, dialect).syntax().to_string() == src`, byte for byte
- parsing never panics and always returns a tree

## Development

It is preferred to use the Nix development shell, which bundles all necessary
tools:

```sh
nix develop
```

The conformance corpus is harvested from real-world repositories and is not
checked in:

```sh
just corpus
```

## Checks

Run the local checks before opening a pull request:

```sh
nix develop --command just ci
```
